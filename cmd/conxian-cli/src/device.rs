//! CLI device verbs: the local end of the pull-based device lifecycle.
//!
//! These verbs are the shared surface for both tracks of the design:
//!
//! * **Track A (local install)** — `conxian-cli migrate` applies the schema to a
//!   local database, idempotently, with no server involved.
//! * **Track B (fleet agent)** — `conxian-cli device pull` runs one pull cycle:
//!   fetch a signed manifest from the control plane, verify it against a pinned
//!   signer key, plan, apply with consent, and report the result back.
//!
//! The same `conxian-device` contract types are used by both. The CLI never
//! applies anything it could not verify, and never applies without consent.

use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use conxian_device::{
    apply, plan, sign_manifest, verify_manifest, ActionExecutor, ActionKind, Consent,
    DesiredStateManifest, DeviceError, ObservedState, SignedManifest, StepOutcome,
};

use crate::{emit, OutputMode};

/// Path to the pinned trusted signer key (hex x-only pubkey) for this device.
fn trusted_signer_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("CONXIAN_DEVICE_SIGNER") {
        return Some(PathBuf::from(p));
    }
    crate::config_path().map(|p| p.with_file_name("device-signer.hex"))
}

/// Path to the desired-state manifest file used by the offline/`--file` path.
fn manifest_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("CONXIAN_DEVICE_MANIFEST") {
        return Some(PathBuf::from(p));
    }
    crate::config_path().map(|p| p.with_file_name("desired-state.json"))
}

/// Read the pinned trusted signer key, or fail closed.
fn read_trusted_signer() -> Result<String> {
    let path = trusted_signer_path()
        .context("No device signer path (set $CONXIAN_DEVICE_SIGNER or $HOME)")?;
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("No pinned signer key at {}", path.display()))?;
    Ok(raw.trim().to_string())
}

/// `conxian-cli device pull` — one full pull cycle against the control plane.
///
/// Sequence (the design's §2 loop, all local-initiated):
///   1. GET the signed desired-state manifest for this device.
///   2. Verify it against the pinned signer key (fail closed).
///   3. Plan against observed local state.
///   4. Apply with consent (interactive prompt, or `--yes`).
///   5. Report the outcome back to the server.
pub async fn run_device_pull(args: &[String], mode: OutputMode) -> Result<()> {
    let flags = parse_flags(args);
    let gateway = crate::resolve_gateway(&flags);
    let device_id = flags
        .get("device")
        .cloned()
        .or_else(|| env::var("CONXIAN_DEVICE_ID").ok())
        .context("--device <id> or $CONXIAN_DEVICE_ID is required")?;
    let enrollment_token = env::var("CONXIAN_ENROLLMENT_TOKEN")
        .context("$CONXIAN_ENROLLMENT_TOKEN is required (the device's enrollment credential)")?;

    // 1. Pull.
    let signed = fetch_signed_manifest(&gateway, &device_id, &enrollment_token).await?;

    // 2. Verify against the pinned key.
    let trusted = read_trusted_signer()?;
    let now = chrono::Utc::now().to_rfc3339();
    verify_manifest(&signed, &trusted, Some(&now)).map_err(map_device_err)?;
    emit(mode, "verified", &format!("manifest verified for device {device_id}"));

    // 3. Plan.
    let observed = read_observed();
    let p = plan(&signed.manifest, &observed).map_err(map_device_err)?;
    if p.is_noop() {
        emit(mode, "converged", "nothing to do");
        report_outcome(&gateway, &device_id, &enrollment_token, &p, "noop", &[]).await?;
        return Ok(());
    }
    if p.has_blockers() {
        report_outcome(&gateway, &device_id, &enrollment_token, &p, "blocked", &[]).await?;
        anyhow::bail!("plan is blocked; resolve local preconditions before applying");
    }

    // 4. Apply with consent.
    let consent = if flags.contains_key("yes") || confirm_apply(&p) {
        Consent::Granted
    } else {
        Consent::NotGranted
    };
    if consent == Consent::NotGranted {
        report_outcome(&gateway, &device_id, &enrollment_token, &p, "declined", &[]).await?;
        emit(mode, "declined", "no changes applied");
        return Ok(());
    }

    let executor = LocalExecutor;
    let report = apply(&p, consent, &executor)
        .await
        .map_err(map_device_err)?
        .context("apply produced no report")?;

    // 5. Report.
    let outcomes: Vec<String> = report
        .outcomes
        .iter()
        .map(|(a, o)| format!("{}:{}", a.label(), outcome_label(o)))
        .collect();
    let status = if report.all_succeeded() {
        write_observed(&report.satisfied_after)?;
        "applied"
    } else {
        "failed"
    };
    report_outcome(&gateway, &device_id, &enrollment_token, &p, status, &outcomes).await?;

    if report.all_succeeded() {
        emit(mode, "applied", &format!("{} step(s) applied", report.outcomes.len()));
        Ok(())
    } else {
        anyhow::bail!("apply failed; see the reported outcomes")
    }
}

async fn fetch_signed_manifest(
    gateway: &str,
    device_id: &str,
    token: &str,
) -> Result<SignedManifest> {
    let mut url = crate::normalize_api_base(gateway)?;
    url.set_path(&format!("/api/v1/devices/{device_id}/desired-state"));
    let target = url.to_string();
    let token = token.to_string();
    let body = tokio::task::spawn_blocking(move || {
        let mut resp = minreq::get(target.as_str())
            .with_header("Authorization", format!("Bearer {token}"))
            .with_timeout(15)
            .with_follow_redirects(false)
            .send_lazy()
            .map_err(|_| anyhow::anyhow!("desired-state request failed"))?;
        let status = resp.status_code;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut resp, &mut text)
            .map_err(|_| anyhow::anyhow!("failed to read desired-state body"))?;
        anyhow::ensure!(status == 200, "desired-state returned HTTP {status}");
        Ok::<_, anyhow::Error>(text)
    })
    .await
    .context("task join error during desired-state fetch")??;
    serde_json::from_str(&body).context("desired-state response was not a signed manifest")
}

async fn report_outcome(
    gateway: &str,
    device_id: &str,
    token: &str,
    p: &conxian_device::Plan,
    status: &str,
    outcomes: &[String],
) -> Result<()> {
    let mut url = crate::normalize_api_base(gateway)?;
    url.set_path(&format!("/api/v1/devices/{device_id}/report"));
    let target = url.to_string();
    let token = token.to_string();
    let payload = serde_json::json!({
        "device": device_id,
        "manifestDigest": p.manifest_digest,
        "status": status,
        "outcomes": outcomes,
    })
    .to_string();
    tokio::task::spawn_blocking(move || {
        let resp = minreq::post(target.as_str())
            .with_header("Authorization", format!("Bearer {token}"))
            .with_header("Content-Type", "application/json")
            .with_timeout(15)
            .with_follow_redirects(false)
            .with_body(payload)
            .send()
            .map_err(|_| anyhow::anyhow!("report request failed"))?;
        anyhow::ensure!(
            (200..300).contains(&resp.status_code),
            "report returned HTTP {}",
            resp.status_code
        );
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("task join error during report")??;
    Ok(())
}

/// `conxian-cli device sign --manifest <path> --key <hex>` — used by the
/// control plane side (or a test) to produce a signed manifest file.
pub fn run_device_sign(args: &[String]) -> Result<()> {
    let flags = parse_flags(args);
    let key = flags
        .get("key")
        .cloned()
        .or_else(|| env::var("CONXIAN_DEVICE_SIGNING_KEY").ok())
        .context("--key <hex> or $CONXIAN_DEVICE_SIGNING_KEY is required")?;
    let in_path = flags
        .get("manifest")
        .map(PathBuf::from)
        .context("--manifest <path> is required")?;
    let raw = fs::read_to_string(&in_path)
        .with_context(|| format!("Cannot read manifest at {}", in_path.display()))?;
    let manifest: DesiredStateManifest = serde_json::from_str(&raw)
        .context("Manifest file is not a valid desired-state manifest")?;
    let signed = sign_manifest(&manifest, &key).map_err(|e| anyhow::anyhow!("{e}"))?;
    let out = flags.get("out").map(PathBuf::from);
    let json = serde_json::to_string_pretty(&signed)?;
    match out {
        Some(p) => {
            fs::write(&p, json).with_context(|| format!("Cannot write {}", p.display()))?;
            println!("Signed manifest written to {}", p.display());
        }
        None => println!("{json}"),
    }
    Ok(())
}

/// `conxian-cli device verify` — verify a signed manifest against the pinned key.
pub fn run_device_verify(args: &[String]) -> Result<()> {
    let flags = parse_flags(args);
    let path = flags
        .get("file")
        .map(PathBuf::from)
        .or_else(manifest_path)
        .context("--file <path> is required (or set $CONXIAN_DEVICE_MANIFEST)")?;
    let signed = read_signed_manifest(&path)?;
    let trusted = read_trusted_signer()?;
    let now = chrono::Utc::now().to_rfc3339();
    verify_manifest(&signed, &trusted, Some(&now)).map_err(map_device_err)?;
    println!(
        "  [VERIFIED] manifest for device '{}' signed by {}",
        signed.manifest.device, signed.signer
    );
    Ok(())
}

/// `conxian-cli device plan` — verify then plan, printing the diff. No changes.
pub fn run_device_plan(args: &[String]) -> Result<()> {
    let flags = parse_flags(args);
    let signed = load_signed(args, &flags)?;
    let trusted = read_trusted_signer()?;
    let now = chrono::Utc::now().to_rfc3339();
    verify_manifest(&signed, &trusted, Some(&now)).map_err(map_device_err)?;

    let observed = read_observed();
    let p = plan(&signed.manifest, &observed).map_err(map_device_err)?;
    println!(
        "Plan for device '{}' ({} step(s), digest {}):",
        p.device,
        p.steps.len(),
        &p.manifest_digest[..12.min(p.manifest_digest.len())]
    );
    for step in &p.steps {
        println!("  [{}] {}", status_label(&step.status), step.action.label());
    }
    if p.is_noop() {
        println!("  Nothing to do; the device is converged.");
    }
    if p.has_blockers() {
        println!("  BLOCKED: resolve the preconditions above before applying.");
    }
    Ok(())
}

/// `conxian-cli device apply` — verify, plan, then apply with consent.
///
/// Consent is required: `--yes` (or an interactive prompt on a TTY) records it.
pub async fn run_device_apply(args: &[String]) -> Result<()> {
    let flags = parse_flags(args);
    let signed = load_signed(args, &flags)?;
    let trusted = read_trusted_signer()?;
    let now = chrono::Utc::now().to_rfc3339();
    verify_manifest(&signed, &trusted, Some(&now)).map_err(map_device_err)?;

    let observed = read_observed();
    let p = plan(&signed.manifest, &observed).map_err(map_device_err)?;
    if p.is_noop() {
        println!("Nothing to do; the device is converged.");
        return Ok(());
    }
    if p.has_blockers() {
        anyhow::bail!("plan is blocked; resolve preconditions before applying");
    }

    let consent = if flags.contains_key("yes") || confirm_apply(&p) {
        Consent::Granted
    } else {
        Consent::NotGranted
    };
    if consent == Consent::NotGranted {
        println!("Declined; no changes applied.");
        return Ok(());
    }

    let executor = LocalExecutor;
    let report = apply(&p, consent, &executor)
        .await
        .map_err(map_device_err)?
        .context("apply produced no report")?;

    for (action, outcome) in &report.outcomes {
        println!("  [{}] {}", outcome_label(outcome), action.label());
    }
    if report.all_succeeded() {
        write_observed(&report.satisfied_after)?;
        println!("Applied {} step(s).", report.outcomes.len());
        Ok(())
    } else {
        anyhow::bail!("apply failed; see the outcome above")
    }
}

/// `conxian-cli migrate --database-url <url>` — apply the schema locally,
/// idempotently (Track A). No server, no agent. Uses `psql` when available so
/// the CLI does not need a Postgres client library, and fails closed otherwise.
pub async fn run_migrate(args: &[String]) -> Result<()> {
    let flags = parse_flags(args);
    let db_url = flags
        .get("database-url")
        .cloned()
        .or_else(|| env::var("DATABASE_URL").ok())
        .or_else(|| env::var("MOR_WEBHOOK_DATABASE_URL").ok())
        .context("--database-url <url> or $DATABASE_URL is required")?;

    let sql = include_str!("../../../migrations/0002_onboarding_tenants.sql");
    let executor = LocalExecutor;
    let outcome = executor.apply_sql(&db_url, sql).await;
    match outcome {
        StepOutcome::Applied => {
            println!("  [APPLIED] schema converged for control-plane");
            Ok(())
        }
        StepOutcome::NoOp => {
            println!("  [NOOP] schema already converged");
            Ok(())
        }
        StepOutcome::Failed { reason } => anyhow::bail!("migrate failed: {reason}"),
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn load_signed(_args: &[String], flags: &std::collections::HashMap<String, String>) -> Result<SignedManifest> {
    if let Some(url) = flags.get("gateway") {
        // The pull path is wired in the agent loop; here we only read a file.
        let _ = url;
    }
    let path = flags
        .get("file")
        .map(PathBuf::from)
        .or_else(manifest_path)
        .context("--file <path> is required (or set $CONXIAN_DEVICE_MANIFEST)")?;
    read_signed_manifest(&path)
}

fn read_signed_manifest(path: &Path) -> Result<SignedManifest> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Cannot read manifest at {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("{} is not a valid signed manifest", path.display()))
}

fn read_observed() -> ObservedState {
    let path = match crate::config_path() {
        Some(p) => p.with_file_name("observed-state.json"),
        None => return ObservedState::default(),
    };
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_observed(satisfied: &[String]) -> Result<()> {
    let Some(path) = crate::config_path().map(|p| p.with_file_name("observed-state.json")) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let state = ObservedState {
        satisfied: satisfied.to_vec(),
        env_keys_present: Vec::new(),
    };
    fs::write(&path, serde_json::to_string_pretty(&state)?).ok();
    Ok(())
}

fn parse_flags(args: &[String]) -> std::collections::HashMap<String, String> {
    let mut flags = std::collections::HashMap::new();
    let mut index = 0;
    while index < args.len() {
        if let Some(name) = args[index].strip_prefix("--") {
            let has_value = args
                .get(index + 1)
                .is_some_and(|next| !next.starts_with("--"));
            if has_value {
                flags.insert(name.to_string(), args[index + 1].clone());
                index += 2;
            } else {
                flags.insert(name.to_string(), String::new());
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    flags
}

fn map_device_err(e: DeviceError) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

fn status_label(s: &conxian_device::StepStatus) -> &'static str {
    use conxian_device::StepStatus::*;
    match s {
        Pending => "PENDING",
        Satisfied => "OK",
        Blocked { .. } => "BLOCKED",
    }
}

fn outcome_label(o: &StepOutcome) -> &'static str {
    match o {
        StepOutcome::Applied => "APPLIED",
        StepOutcome::NoOp => "NOOP",
        StepOutcome::Failed { .. } => "FAILED",
    }
}

fn confirm_apply(p: &conxian_device::Plan) -> bool {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return false;
    }
    println!(
        "About to apply {} action(s) to device '{}'. Continue? [y/N] ",
        p.steps.len(),
        p.device
    );
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// The real local executor: runs the typed action against the machine.
///
/// Only the known action kinds exist; there is no shell-out to arbitrary code.
/// `migrateDatabase` shells to `psql` (fail-closed if absent); `ensureEnv`
/// reads the local environment; image pull and service restart shell to
/// `docker` (fail-closed if absent).
struct LocalExecutor;

impl LocalExecutor {
    async fn apply_sql(&self, db_url: &str, sql: &str) -> StepOutcome {
        let db_url = db_url.to_string();
        let sql = sql.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let mut child = std::process::Command::new("psql")
                .arg(&db_url)
                .arg("-v")
                .arg("ON_ERROR_STOP=1")
                .arg("-q")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("psql not available: {e}"))?;
            {
                use std::io::Write as _;
                let mut stdin = child.stdin.take().ok_or("no stdin")?;
                stdin
                    .write_all(sql.as_bytes())
                    .map_err(|e| format!("write sql: {e}"))?;
            }
            let out = child.wait_with_output().map_err(|e| format!("psql: {e}"))?;
            if out.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
            }
        })
        .await;

        match result {
            Ok(Ok(())) => StepOutcome::Applied,
            Ok(Err(reason)) => StepOutcome::Failed { reason },
            Err(e) => StepOutcome::Failed {
                reason: format!("task join error: {e}"),
            },
        }
    }
}

#[async_trait::async_trait]
impl ActionExecutor for LocalExecutor {
    async fn execute(&self, action: &ActionKind) -> StepOutcome {
        match action {
            ActionKind::EnsureEnv { keys } => {
                let missing: Vec<&String> = keys
                    .iter()
                    .filter(|k| env::var(k.as_str()).map(|v| v.is_empty()).unwrap_or(true))
                    .collect();
                if missing.is_empty() {
                    StepOutcome::Applied
                } else {
                    StepOutcome::Failed {
                        reason: format!(
                            "missing local value for key(s): {}",
                            missing
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    }
                }
            }
            ActionKind::MigrateDatabase { target } => {
                let db_url = env::var("DATABASE_URL")
                    .or_else(|_| env::var("MOR_WEBHOOK_DATABASE_URL"))
                    .unwrap_or_default();
                if db_url.is_empty() {
                    return StepOutcome::Failed {
                        reason: "DATABASE_URL is not set".into(),
                    };
                }
                let sql = match target.as_str() {
                    "control-plane" => include_str!("../../../migrations/0002_onboarding_tenants.sql"),
                    other => {
                        return StepOutcome::Failed {
                            reason: format!("unknown migration target: {other}"),
                        }
                    }
                };
                self.apply_sql(&db_url, sql).await
            }
            ActionKind::PullImage { image } => {
                if !image.contains("@sha256:") {
                    return StepOutcome::Failed {
                        reason: "image must be pinned by digest (tag is mutable)".into(),
                    };
                }
                docker_action(&["pull", image]).await
            }
            ActionKind::RestartService { service } => {
                docker_action(&["restart", service]).await
            }
        }
    }
}

async fn docker_action(args: &[&str]) -> StepOutcome {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let result = tokio::task::spawn_blocking(move || {
        let child = std::process::Command::new("docker")
            .args(&owned)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("docker not available: {e}"))?;
        let out = child.wait_with_output().map_err(|e| format!("docker: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    })
    .await;
    match result {
        Ok(Ok(())) => StepOutcome::Applied,
        Ok(Err(reason)) => StepOutcome::Failed { reason },
        Err(e) => StepOutcome::Failed {
            reason: format!("task join error: {e}"),
        },
    }
}

/// Exposed for tests: build a plan without touching disk.
#[cfg(test)]
pub fn plan_for_test(
    manifest: &DesiredStateManifest,
    observed: &ObservedState,
) -> Result<conxian_device::Plan> {
    plan(manifest, observed).map_err(map_device_err)
}

/// Exposed for tests: run the whole verify→plan→apply path in memory.
#[cfg(test)]
pub async fn apply_signed_for_test<E: ActionExecutor>(
    signed: &SignedManifest,
    trusted: &str,
    observed: &ObservedState,
    consent: Consent,
    executor: &E,
) -> Result<Option<conxian_device::ApplyReport>> {
    let now = chrono::Utc::now().to_rfc3339();
    verify_manifest(signed, trusted, Some(&now)).map_err(map_device_err)?;
    let p = plan(&signed.manifest, observed).map_err(map_device_err)?;
    apply(&p, consent, executor).await.map_err(map_device_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const SK: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    fn manifest() -> DesiredStateManifest {
        let mut m = DesiredStateManifest::new("dev-1", "2026-11-14T00:00:00Z");
        m.actions.push(ActionKind::MigrateDatabase {
            target: "control-plane".into(),
        });
        m.actions.push(ActionKind::RestartService {
            service: "gateway".into(),
        });
        m
    }

    struct CountingExecutor {
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ActionExecutor for CountingExecutor {
        async fn execute(&self, _action: &ActionKind) -> StepOutcome {
            self.calls.fetch_add(1, Ordering::SeqCst);
            StepOutcome::Applied
        }
    }

    #[tokio::test]
    async fn signed_manifest_verifies_and_applies_with_consent() {
        let signed = sign_manifest(&manifest(), SK).unwrap();
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
        };
        let report = apply_signed_for_test(
            &signed,
            &signed.signer,
            &ObservedState::default(),
            Consent::Granted,
            &exec,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(report.all_succeeded());
        assert_eq!(exec.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn tampered_manifest_is_refused_before_apply() {
        let mut signed = sign_manifest(&manifest(), SK).unwrap();
        signed.manifest.actions[0] = ActionKind::MigrateDatabase {
            target: "attacker".into(),
        };
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
        };
        let err = apply_signed_for_test(
            &signed,
            &signed.signer,
            &ObservedState::default(),
            Consent::Granted,
            &exec,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("signature"));
        assert_eq!(
            exec.calls.load(Ordering::SeqCst),
            0,
            "nothing may run for an unverified manifest"
        );
    }

    #[tokio::test]
    async fn without_consent_nothing_applies() {
        let signed = sign_manifest(&manifest(), SK).unwrap();
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
        };
        let report = apply_signed_for_test(
            &signed,
            &signed.signer,
            &ObservedState::default(),
            Consent::NotGranted,
            &exec,
        )
        .await
        .unwrap();
        assert!(report.is_none());
        assert_eq!(exec.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn replan_after_apply_is_a_noop() {
        let signed = sign_manifest(&manifest(), SK).unwrap();
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
        };
        let report = apply_signed_for_test(
            &signed,
            &signed.signer,
            &ObservedState::default(),
            Consent::Granted,
            &exec,
        )
        .await
        .unwrap()
        .unwrap();
        let observed = ObservedState {
            satisfied: report.satisfied_after.clone(),
            ..Default::default()
        };
        let p = plan_for_test(&signed.manifest, &observed).unwrap();
        assert!(p.is_noop());
    }
}
