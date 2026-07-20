#![cfg(feature = "bitvmx-eval")]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use conxian_bitvmx_eval::model::{
    BACKEND, MANIFEST_SCHEMA_VERSION, REPORT_SCHEMA_VERSION, UPSTREAM_REVISION,
};
use conxian_bitvmx_eval::{
    run_manifest, sha256_file, ArtifactSpec, ExecutableSpec, ExecutionSpec, FixtureSpec,
    LimitsSpec, Manifest, SandboxSpec, WARNING,
};

struct Harness {
    root: PathBuf,
    helper: PathBuf,
}

static HARNESS: OnceLock<Harness> = OnceLock::new();
static CASE_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn harness() -> &'static Harness {
    HARNESS.get_or_init(|| {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("conxian-bitvmx-eval-{nonce}"));
        fs::create_dir_all(&root).expect("create test root");
        let helper = root.join("synthetic-helper");
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/helper.rs");
        let status = Command::new("rustc")
            .args([
                "--edition=2021",
                source.to_str().expect("fixture path"),
                "-O",
                "-o",
            ])
            .arg(&helper)
            .status()
            .expect("invoke rustc for source fixture");
        assert!(status.success(), "synthetic helper compilation failed");
        Harness { root, helper }
    })
}

fn activate_external_sandbox_marker() {
    env::set_var("BITVMX_EVAL_SANDBOX_ACTIVE", "1");
    env::set_var("BITVMX_EVAL_SANDBOX_MODE", "network-deny");
}

fn base_manifest(helper: &Path, revision_file: &Path, fixture: &Path, expected: &str) -> Manifest {
    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        manifest_version: 1,
        warning: WARNING.to_string(),
        experimental: true,
        production_supported: false,
        cryptographic_verification: false,
        backend: BACKEND.to_string(),
        upstream_revision: UPSTREAM_REVISION.to_string(),
        executable: ExecutableSpec {
            path: helper.display().to_string(),
            sha256: sha256_file(helper).expect("hash helper"),
            revision_file: revision_file.display().to_string(),
        },
        fixture: FixtureSpec {
            id: "synthetic-source-helper".to_string(),
            path: fixture.display().to_string(),
            sha256: sha256_file(fixture).expect("hash fixture"),
            kind: "synthetic-source".to_string(),
            expected_result_class: expected.to_string(),
            expected_return_value: match expected {
                "halt_success" => Some(0),
                "halt_failure" => Some(7),
                "limit_reached" => None,
                other => panic!("unsupported test expectation: {other}"),
            },
        },
        execution: ExecutionSpec {
            workload: "small".to_string(),
            input_hex: Some("00ff".to_string()),
            limit_steps: Some(100),
            trace: false,
            no_hash: true,
        },
        limits: LimitsSpec {
            max_rss_bytes: None,
            timeout_seconds: None,
            max_output_bytes: None,
        },
        sandbox: SandboxSpec {
            mode: "external-preflight".to_string(),
            network_policy: "deny".to_string(),
        },
        artifacts: Vec::<ArtifactSpec>::new(),
    }
}

fn case(name: &str, scenario: &str, expected: &str) -> (PathBuf, PathBuf, PathBuf) {
    activate_external_sandbox_marker();
    let harness = harness();
    let index = CASE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = harness.root.join(format!("{name}-{index}"));
    fs::create_dir_all(&directory).expect("create case directory");
    let fixture = directory.join("fixture.source");
    fs::write(&fixture, scenario).expect("write fixture");
    let revision_file = directory.join("synthetic-helper.revision");
    fs::write(&revision_file, format!("{UPSTREAM_REVISION}\n")).expect("write revision sidecar");
    let manifest_path = directory.join("manifest.json");
    let report_path = directory.join("report.json");
    let manifest = base_manifest(&harness.helper, &revision_file, &fixture, expected);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write manifest");
    (manifest_path, report_path, fixture)
}

fn rewrite_manifest(path: &Path, mut update: impl FnMut(&mut Manifest)) {
    let contents = fs::read_to_string(path).expect("read manifest");
    let mut manifest: Manifest = serde_json::from_str(&contents).expect("parse manifest");
    update(&mut manifest);
    fs::write(
        path,
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("rewrite manifest");
}

fn rejected(manifest: &Path, report: &Path, expected: &str) {
    let error = run_manifest(manifest, report).expect_err("evaluation should reject");
    let message = error.to_string();
    assert!(
        message.contains(expected),
        "{message} does not contain {expected}"
    );
}

#[test]
fn success_report_has_non_production_contract() {
    let (manifest, report, _) = case("success", "success", "halt_success");
    let result = run_manifest(&manifest, &report).expect("success fixture should run");
    assert_eq!(result.schema_version, REPORT_SCHEMA_VERSION);
    assert_eq!(result.warning, WARNING);
    assert!(result.experimental);
    assert!(!result.production_supported);
    assert!(!result.cryptographic_verification);
    assert_eq!(result.backend, BACKEND);
    assert_eq!(result.upstream_revision, UPSTREAM_REVISION);
    assert_eq!(result.expected_result_class, "halt_success");
    assert_eq!(result.actual_result_class.as_deref(), Some("halt_success"));
    assert_eq!(result.executed_steps, Some(7));
    assert!(result.proof_size_bytes.is_none());
    assert_eq!(result.proof_size_reason, "not_applicable_cpu_backend");
    assert!(result.failure.is_none());
    assert!(report.is_file());
    let report_json: serde_json::Value =
        serde_json::from_slice(&fs::read(report).expect("read report")).expect("parse report");
    assert_eq!(report_json["experimental"], true);
    assert_eq!(report_json["production_supported"], false);
    assert_eq!(report_json["cryptographic_verification"], false);
    assert_eq!(report_json["proof_size_bytes"], serde_json::Value::Null);
}

#[test]
fn expected_halt_failure_is_classified_without_verification() {
    let (manifest, report, _) = case("expected-failure", "failure", "halt_failure");
    let result = run_manifest(&manifest, &report).expect("expected failure class is valid");
    assert_eq!(result.actual_result_class.as_deref(), Some("halt_failure"));
    assert_eq!(result.expected_result_class, "halt_failure");
    assert!(result.failure.is_none());
}

#[test]
fn missing_binary_is_rejected() {
    let (manifest, report, _) = case("missing-binary", "success", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.executable.path = manifest
            .executable
            .path
            .replace("synthetic-helper", "missing-helper");
    });
    rejected(&manifest, &report, "executable");
}

#[test]
fn revision_mismatch_is_rejected() {
    let (manifest, report, _) = case("revision-mismatch", "success", "halt_success");
    let contents = fs::read_to_string(&manifest).expect("read manifest");
    let parsed: Manifest = serde_json::from_str(&contents).expect("parse manifest");
    fs::write(parsed.executable.revision_file, "not-the-pinned-revision\n")
        .expect("rewrite sidecar");
    rejected(&manifest, &report, "revision mismatch");
}

#[test]
fn executable_hash_mismatch_is_rejected() {
    let (manifest, report, _) = case("hash-mismatch", "success", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.executable.sha256 = "00".repeat(32);
    });
    rejected(&manifest, &report, "executable SHA-256 mismatch");
}

#[test]
fn corrupted_fixture_is_rejected() {
    let (manifest, report, fixture) = case("corrupted-fixture", "success", "halt_success");
    fs::write(fixture, "corrupted-after-manifest").expect("corrupt fixture");
    rejected(&manifest, &report, "fixture SHA-256 mismatch");
}

#[test]
fn malformed_output_is_rejected() {
    let (manifest, report, _) = case("malformed-output", "malformed", "halt_success");
    rejected(&manifest, &report, "malformed_or_unrecognized_output");
}

#[test]
fn nonzero_exit_is_rejected() {
    let (manifest, report, _) = case("nonzero-exit", "nonzero", "halt_success");
    rejected(&manifest, &report, "nonzero_exit");
}

#[test]
fn timeout_is_rejected() {
    let (manifest, report, _) = case("timeout", "timeout", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.limits.timeout_seconds = Some(1);
    });
    rejected(&manifest, &report, "timeout");
}

#[test]
fn output_limit_is_rejected() {
    let (manifest, report, _) = case("output-limit", "output", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.limits.max_output_bytes = Some(1024);
    });
    rejected(&manifest, &report, "output_limit");
}

#[test]
fn rss_limit_is_rejected_when_observable() {
    let (manifest, report, _) = case("rss-limit", "rss", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.limits.max_rss_bytes = Some(8 * 1024 * 1024);
        manifest.limits.timeout_seconds = Some(2);
    });
    rejected(&manifest, &report, "rss_limit");
}

#[test]
fn unexpected_result_class_is_rejected() {
    let (manifest, report, _) = case("unexpected-result", "unexpected", "halt_failure");
    rejected(&manifest, &report, "unexpected_result_class");
}

#[test]
fn malformed_manifest_is_rejected() {
    activate_external_sandbox_marker();
    let harness = harness();
    let manifest = harness.root.join("malformed-manifest.json");
    let report = harness.root.join("malformed-manifest-report.json");
    fs::write(&manifest, b"{not-json").expect("write malformed manifest");
    rejected(&manifest, &report, "invalid JSON");
}
