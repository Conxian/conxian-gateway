use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct InstallerConfig {
    pub auth_token: String,
    pub trust_tier: String,
    pub network: String,
    pub bitcoin_rpc_url: String,
    pub stacks_rpc_url: String,
    pub database_url: String,
    pub gateway_port: u16,
    pub odata_v4_webhook_url: Option<String>,
}

impl Default for InstallerConfig {
    fn default() -> Self {
        Self {
            auth_token: "CONXIAN_PROD_AUTH_TOKEN_REQUIRED".to_string(),
            trust_tier: "T1".to_string(),
            network: "mainnet".to_string(),
            bitcoin_rpc_url: "http://127.0.0.1:8332".to_string(),
            stacks_rpc_url: "https://api.mainnet.hiro.so".to_string(),
            database_url: "sqlite://gateway_state.db".to_string(),
            gateway_port: 3000,
            odata_v4_webhook_url: Some(
                "https://erp.client-domain.com/odata/v4/BankStatements".to_string(),
            ),
        }
    }
}

fn ensure_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => anyhow::bail!("Refusing to replace an existing or inaccessible configuration path"),
    }
}

fn write_new_private(path: &Path, content: &str) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .context("Cannot create configuration; existing files and symlinks are preserved")?;
    file.write_all(content.as_bytes())
        .context("Failed to write new configuration")?;
    Ok(())
}

fn dotenv_literal(value: &str) -> Result<String> {
    anyhow::ensure!(
        !value
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\0' | '\'' | '\\')),
        "Template input contains an unsupported dotenv delimiter"
    );
    // Compose single-quoted values are literal: dollar signs are not expanded.
    Ok(format!("'{value}'"))
}

pub fn generate_env_file(config: &InstallerConfig, output_path: &Path) -> Result<()> {
    anyhow::ensure!(config.gateway_port != 0, "API port must be non-zero");
    let content = format!(
        "# Incomplete Gateway template; no credentials or readiness are established.\n\
         # Supply an accepted image@sha256:digest; no registry image is assumed.\n\
         GATEWAY_IMAGE=\n\
         API_TOKEN={}\n\
         FIAT_WEBHOOK_SECRET='sentinel_FIAT_WEBHOOK_SECRET'\n\
         SETTLEMENT_INGRESS_SECRET='sentinel_SETTLEMENT_INGRESS_SECRET'\n\
         HMAC_SECRET='sentinel_HMAC_SECRET'\n\
         OFFLINE_QUEUE_SECRET='sentinel_OFFLINE_QUEUE_SECRET'\n\
         # RPC endpoints must be reachable from the container; loopback is container-local.\n\
         BITCOIN_RPC_URL={}\n\
         STACKS_RPC_URL={}\n\
         API_PORT={}\n\
         CONXIAN_NETWORK={}\n\
         GATEWAY_STATE_PATH='/usr/app/gateway_state.json'\n\
         GATEWAY_PERSISTENCE_MODE='exclusive-local-writer'\n\
         GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM='false'\n\
         RAMP_MODE='disabled'\n\
         STITCH_MODE='disabled'\n\
         OZOW_MODE='disabled'\n\
         PAPSS_MODE='disabled'\n\
         ALCHEMY_PAY_MODE='disabled'\n\
         BANXA_MODE='disabled'\n\
         INFOBIP_MODE='disabled'\n",
        dotenv_literal(&config.auth_token)?,
        dotenv_literal(&config.bitcoin_rpc_url)?,
        dotenv_literal(&config.stacks_rpc_url)?,
        config.gateway_port,
        config.network
    );
    // Legacy InstallerConfig trust/database/OData fields remain serializable,
    // but are not advertised as inputs to the current Gateway binary.
    write_new_private(output_path, &content)
}

pub fn generate_docker_compose(config: &InstallerConfig, output_path: &Path) -> Result<()> {
    anyhow::ensure!(config.gateway_port != 0, "API port must be non-zero");
    let content = r#"# Run with: docker compose --env-file .env.production up -d
# One local writer only; this template does not qualify a deployment.
services:
  conxian-gateway:
    image: "${GATEWAY_IMAGE:?Set GATEWAY_IMAGE to a reviewed image@sha256:digest}"
    restart: unless-stopped
    working_dir: /usr/app
    env_file:
      - .env.production
    environment:
      API_PORT: "${API_PORT:?Set API_PORT}"
    ports:
      - "127.0.0.1:${API_PORT:?Set API_PORT}:${API_PORT:?Set API_PORT}"
    volumes:
      - gateway_data:/usr/app
volumes:
  gateway_data:
"#;
    write_new_private(output_path, content)
}

pub struct DoctorReport {
    pub secret_valid: bool,
    pub tier_valid: bool,
    // Historical field names retained; these indicate URL syntax, not probes.
    pub btc_rpc_reachable: bool,
    pub stacks_rpc_reachable: bool,
}

pub fn run_doctor_checks(config: &InstallerConfig) -> DoctorReport {
    let token = config.auth_token.trim();
    let secret_valid = !token.is_empty()
        && !token.starts_with("sentinel_")
        && token != "CONXIAN_PROD_AUTH_TOKEN_REQUIRED";
    let tier_valid = config.trust_tier == "T1" || config.trust_tier == "T2";
    let btc_rpc_reachable = url::Url::parse(&config.bitcoin_rpc_url).is_ok();
    let stacks_rpc_reachable = url::Url::parse(&config.stacks_rpc_url).is_ok();

    DoctorReport {
        secret_valid,
        tier_valid,
        btc_rpc_reachable,
        stacks_rpc_reachable,
    }
}

/// Validates a Gateway base URL and normalizes it to the `/api/v1` API base.
///
/// Accepts an HTTP(S) origin or an explicit `/api/v1` base, and rejects
/// credentials, query strings and fragments so a caller cannot smuggle an
/// ambiguous or credential-bearing base past the CLI.
fn normalize_api_base(base_url: &str) -> Result<url::Url> {
    let mut api_base = url::Url::parse(base_url).context("Invalid Gateway base URL")?;
    anyhow::ensure!(
        matches!(api_base.scheme(), "http" | "https")
            && api_base.host_str().is_some()
            && api_base.username().is_empty()
            && api_base.password().is_none()
            && api_base.query().is_none()
            && api_base.fragment().is_none()
            && matches!(api_base.path().trim_end_matches('/'), "" | "/api/v1"),
        "Gateway base URL must be an HTTP(S) origin or /api/v1 base without credentials, query or fragment"
    );
    api_base.set_path("/api/v1");
    Ok(api_base)
}

/// A tenant id is server-generated (`t_<hex>`); reject anything else so it can
/// be placed in a path segment without ambiguity.
fn validate_tenant_id(tenant_id: &str) -> Result<()> {
    anyhow::ensure!(
        !tenant_id.is_empty()
            && tenant_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "Tenant id contains unsupported characters"
    );
    Ok(())
}

/// The provisioning receipt returned by `POST /api/v1/onboard`. `api_key` is the
/// plaintext key, present only on first provision or rotation.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OnboardReceipt {
    pub tenant_id: String,
    pub state: String,
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub endpoint_url: Option<String>,
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Provisions a tenant through the single `POST /api/v1/onboard` call.
///
/// The onboarding token is sent as a bearer and never logged or echoed. A
/// non-200 response fails closed with the status only; the caller does not get a
/// half-parsed receipt.
pub async fn provision_tenant(
    gateway_base_url: &str,
    onboarding_token: &str,
    idempotency_key: &str,
    tier: Option<&str>,
) -> Result<OnboardReceipt> {
    anyhow::ensure!(
        !onboarding_token.trim().is_empty(),
        "Onboarding token is empty; set ONBOARDING_TOKEN"
    );
    anyhow::ensure!(
        !idempotency_key.trim().is_empty(),
        "Idempotency key must be non-empty"
    );
    let mut url = normalize_api_base(gateway_base_url)?;
    url.set_path("/api/v1/onboard");

    let mut payload = serde_json::json!({ "idempotency_key": idempotency_key });
    if let Some(tier) = tier {
        payload["tier"] = serde_json::json!(tier);
    }
    let body = serde_json::to_string(&payload).context("Failed to encode onboarding request")?;

    let token = onboarding_token.to_string();
    let target = url.to_string();
    let (status, text) = tokio::task::spawn_blocking(move || {
        let mut resp = minreq::post(target.as_str())
            .with_header("Authorization", format!("Bearer {token}"))
            .with_header("Content-Type", "application/json")
            .with_body(body)
            .with_timeout(10)
            .with_follow_redirects(false)
            .with_max_headers_size(8192)
            .with_max_status_line_length(1024)
            .send_lazy()
            .map_err(|_| anyhow::anyhow!("Onboarding request failed"))?;
        let status = resp.status_code;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut resp, &mut text)
            .map_err(|_| anyhow::anyhow!("Failed to read onboarding response body"))?;
        Ok::<_, anyhow::Error>((status, text))
    })
    .await
    .context("Task join error during onboarding request")??;

    anyhow::ensure!(status == 200, "Onboarding returned HTTP {status}");
    serde_json::from_str::<OnboardReceipt>(&text).context("Onboarding receipt was not valid JSON")
}

/// Fetches the published limits for a tenant using the minted managed key.
///
/// This is the self-verification step of the onboarding lane: a 200 here proves
/// the minted key authenticates through the same managed-auth path as any other
/// key, so a returned key is a working key, not an untested string.
pub async fn fetch_tenant_limits(
    gateway_base_url: &str,
    api_key: &str,
    tenant_id: &str,
) -> Result<serde_json::Value> {
    anyhow::ensure!(!api_key.trim().is_empty(), "Managed API key is empty");
    validate_tenant_id(tenant_id)?;
    let mut url = normalize_api_base(gateway_base_url)?;
    url.set_path(&format!("/api/v1/tenants/{tenant_id}/limits"));

    let key = api_key.to_string();
    let target = url.to_string();
    let (status, text) = tokio::task::spawn_blocking(move || {
        let mut resp = minreq::get(target.as_str())
            .with_header("Authorization", format!("Bearer {key}"))
            .with_timeout(10)
            .with_follow_redirects(false)
            .with_max_headers_size(8192)
            .with_max_status_line_length(1024)
            .send_lazy()
            .map_err(|_| anyhow::anyhow!("Limits request failed"))?;
        let status = resp.status_code;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut resp, &mut text)
            .map_err(|_| anyhow::anyhow!("Failed to read limits response body"))?;
        Ok::<_, anyhow::Error>((status, text))
    })
    .await
    .context("Task join error during limits request")??;

    anyhow::ensure!(
        status == 200,
        "Managed-key verification returned HTTP {status} (the minted key was rejected)"
    );
    serde_json::from_str(&text).context("Limits response was not valid JSON")
}

pub async fn probe_gateway_status(gateway_url: &str) -> Result<String> {
    let mut health_url = normalize_api_base(gateway_url)?;
    health_url.set_path("/api/v1/health");
    tokio::task::spawn_blocking(move || {
        let resp = minreq::get(health_url.as_str())
            .with_timeout(3)
            .with_follow_redirects(false)
            .with_max_headers_size(8192)
            .with_max_status_line_length(1024)
            .send_lazy()
            .map_err(|_| anyhow::anyhow!("Gateway liveness probe failed"))?;
        anyhow::ensure!(
            resp.status_code == 200,
            "Gateway liveness returned HTTP {}",
            resp.status_code
        );
        // Inspect headers/status only: never download or echo a provider body.
        Ok("Gateway liveness observed (HTTP 200); readiness and provider connectivity not established".to_string())
    })
    .await
    .context("Task join error during gateway health probe")?
}

fn print_usage() {
    println!("Conxian Sovereign Settlement Suite - Unified Installer & CLI (v0.1.5)");
    println!("Usage: conxian-cli <subcommand> [options]\n");
    println!("Subcommands:");
    println!("  init     Initialize client deployment configuration (.env, docker-compose.yml)");
    println!("  doctor   Inspect default installer configuration (no connectivity probes)");
    println!("  start    Print startup instructions (does not launch a process)");
    println!("  status   Probe Gateway HTTP liveness only");
    println!("  onboard  Provision a tenant via POST /v1/onboard and verify the minted key\n");
    println!("onboard options:");
    println!("  --gateway <url>          Gateway base URL (default: $GATEWAY_URL or http://127.0.0.1:8080)");
    println!("  --tier <tier>            Requested tier (default: sandbox)");
    println!("  --idempotency-key <key>  Idempotency key (default: generated per run)");
    println!("  --write-env <path>       Write the minted key to a 0600 env file (optional)");
    println!("  Onboarding token is read from $ONBOARDING_TOKEN only (never a flag).");
}

/// Parses `--flag value` pairs into a map. Unknown flags are ignored so the
/// command stays forward-compatible.
fn parse_flags(args: &[String]) -> std::collections::HashMap<String, String> {
    let mut flags = std::collections::HashMap::new();
    let mut index = 0;
    while index < args.len() {
        if let Some(name) = args[index].strip_prefix("--") {
            if index + 1 < args.len() {
                flags.insert(name.to_string(), args[index + 1].clone());
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    flags
}

/// A short, opaque idempotency key so a repeated invocation without an explicit
/// key is still safe to retry by the operator re-running the same command.
fn default_idempotency_key() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("cli-{nanos:x}")
}

async fn run_onboard(args: &[String]) -> Result<()> {
    let flags = parse_flags(args);
    let gateway_url = flags
        .get("gateway")
        .cloned()
        .or_else(|| env::var("GATEWAY_URL").ok())
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let tier = flags.get("tier").cloned();
    let idempotency_key = flags
        .get("idempotency-key")
        .cloned()
        .unwrap_or_else(default_idempotency_key);
    let onboarding_token = env::var("ONBOARDING_TOKEN")
        .context("ONBOARDING_TOKEN must be set (the CLI never takes it as a flag)")?;

    println!("Provisioning a tenant via POST /v1/onboard ...");
    let receipt = provision_tenant(
        &gateway_url,
        &onboarding_token,
        &idempotency_key,
        tier.as_deref(),
    )
    .await?;

    println!("  [SUCCESS] tenant_id: {}", receipt.tenant_id);
    println!("  [SUCCESS] state:     {}", receipt.state);
    if let Some(endpoint) = &receipt.endpoint_url {
        println!("  [SUCCESS] endpoint:  {}", endpoint);
    }
    if let Some(key_id) = &receipt.key_id {
        println!("  [SUCCESS] key_id:    {}", key_id);
    }

    // Self-verification: use the minted key against a managed route. A 200
    // proves the key authenticates through the same path as any other key.
    match receipt.api_key.as_deref() {
        Some(api_key) => {
            println!("Verifying the minted key against /v1/tenants/{{id}}/limits ...");
            let limits = fetch_tenant_limits(&gateway_url, api_key, &receipt.tenant_id).await?;
            println!(
                "  [VERIFIED] managed key accepted; published limits: {}",
                serde_json::to_string(&limits).unwrap_or_else(|_| "{}".to_string())
            );
            if let Some(path) = flags.get("write-env") {
                let content = format!(
                    "# Managed tenant credential for {} (0600). Keep out of version control.\n\
                     GATEWAY_MANAGED_API_KEY={}\n\
                     GATEWAY_TENANT_ID={}\n",
                    receipt.tenant_id,
                    dotenv_literal(api_key)?,
                    dotenv_literal(&receipt.tenant_id)?
                );
                write_new_private(Path::new(path), &content)?;
                println!("  [SUCCESS] wrote managed credential to {}", path);
            } else {
                println!("  Note: re-run with --write-env <path> to persist the key; it is not shown again.");
            }
        }
        None => {
            println!(
                "  Note: no new key was minted (idempotent replay); the tenant already exists."
            );
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "init" => {
            println!("Initializing Conxian Sovereign Gateway Deployment...");
            let config = InstallerConfig::default();
            let env_path = PathBuf::from(".env.production");
            let compose_path = PathBuf::from("docker-compose.yml");

            ensure_absent(&env_path)?;
            ensure_absent(&compose_path)?;
            generate_env_file(&config, &env_path)?;
            generate_docker_compose(&config, &compose_path)?;

            println!("  [SUCCESS] Created env file: {:?}", env_path);
            println!(
                "  [SUCCESS] Created Docker Compose file: {:?}",
                compose_path
            );
            println!("\nTemplates created. Complete the current startup secret and persistence contract before use.");
        }
        "doctor" => {
            println!("Running Conxian Preflight Diagnostic Checks...");
            let config = InstallerConfig::default();
            let report = run_doctor_checks(&config);

            println!(
                "  - Default installer API token configured (not a strength test): {}",
                if report.secret_valid {
                    "PASS"
                } else {
                    "FAIL (sentinel or empty)"
                }
            );
            println!(
                "  - Trust Tier Verification (T1/T2): {}",
                if report.tier_valid { "PASS" } else { "FAIL" }
            );
            println!(
                "  - Bitcoin RPC URL syntax (connectivity not tested): {}",
                if report.btc_rpc_reachable {
                    "PASS (Valid URL)"
                } else {
                    "FAIL"
                }
            );
            println!(
                "  - Stacks RPC URL syntax (connectivity not tested): {}",
                if report.stacks_rpc_reachable {
                    "PASS (Valid URL)"
                } else {
                    "FAIL"
                }
            );

            if report.secret_valid
                && report.tier_valid
                && report.btc_rpc_reachable
                && report.stacks_rpc_reachable
            {
                println!("\nConfiguration checks passed; deployment readiness is not established.");
            } else {
                println!("\nSystem Diagnostics Warnings Detected. Review above parameters.");
                anyhow::bail!("Default installer configuration is incomplete; runtime readiness was not tested");
            }
        }
        "start" => {
            println!("Starting Conxian Sovereign Gateway Deployment Stack...");
            println!("Startup instructions only; no process has been launched.");
            println!("Complete .env.production, then run: docker compose --env-file .env.production up -d");
            println!("For binary deployment, run: cargo run --bin gateway");
        }
        "status" => {
            println!("Probing Conxian Gateway Health...");
            let gateway_url =
                env::var("GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
            let message = probe_gateway_status(&gateway_url).await?;
            println!("  [STATUS] {}", message);
        }
        "onboard" => {
            run_onboard(&args[2..]).await?;
        }
        _ => {
            print_usage();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};

    #[test]
    fn test_installer_config_default_and_env_gen() {
        let config = InstallerConfig::default();
        let temp_dir = std::env::temp_dir();
        let env_file = temp_dir.join("test_env.tmp");
        generate_env_file(&config, &env_file).unwrap();

        let content = fs::read_to_string(&env_file).unwrap();
        assert!(content.contains("API_TOKEN='CONXIAN_PROD_AUTH_TOKEN_REQUIRED'"));
        assert!(!content.contains("CONXIAN_GATEWAY_AUTH_TOKEN="));
        assert!(content.contains("API_PORT=3000"));
        assert!(content.contains("CONXIAN_NETWORK=mainnet"));
        assert!(!content.contains("GATEWAY_PORT="));
        let _ = fs::remove_file(env_file);
    }

    #[test]
    fn test_generate_docker_compose() {
        let config = InstallerConfig::default();
        let temp_dir = std::env::temp_dir();
        let compose_file = temp_dir.join("test_compose.tmp");
        generate_docker_compose(&config, &compose_file).unwrap();

        let content = fs::read_to_string(&compose_file).unwrap();
        assert!(content.contains("${GATEWAY_IMAGE:?"));
        assert!(content.contains("gateway_data:/usr/app"));
        assert!(content.contains(".env.production"));
        assert!(!content.contains("API_TOKEN="));
        assert!(!content.contains("CONXIAN_GATEWAY_AUTH_TOKEN="));
        let _ = fs::remove_file(compose_file);
    }

    #[test]
    fn test_doctor_checks() {
        let mut config = InstallerConfig::default();
        assert!(!run_doctor_checks(&config).secret_valid);
        config.auth_token = "installer-test-token".to_string();
        let report = run_doctor_checks(&config);
        assert!(report.secret_valid);
        assert!(report.tier_valid);
        assert!(report.btc_rpc_reachable);
        assert!(report.stacks_rpc_reachable);

        config.auth_token = "sentinel_API_TOKEN".to_string();
        let report2 = run_doctor_checks(&config);
        assert!(!report2.secret_valid);
    }

    #[test]
    fn dotenv_values_are_literal_and_reject_delimiter_injection() {
        assert_eq!(
            dotenv_literal("$SYNTHETIC # value").unwrap(),
            "'$SYNTHETIC # value'"
        );
        for value in [
            "private\nAPI_PORT=9",
            "private\rvalue",
            "private\0value",
            "private'value",
            "private\\value",
        ] {
            let error = dotenv_literal(value).unwrap_err().to_string();
            assert!(!error.contains("private"));
        }
    }

    #[tokio::test]
    async fn test_probe_gateway_status_mock_server() {
        let app = Router::new().route("/api/v1/health", get(|| async { "private-test-body" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            let _ = tx.send(());
            axum::serve(listener, app).await.unwrap();
        });

        rx.await.unwrap();

        let url = format!("http://{}", addr);
        let status = probe_gateway_status(&url).await.unwrap();
        assert!(status.contains("liveness observed"));
        assert!(status.contains("readiness and provider connectivity not established"));
        assert!(!status.contains("private-test-body"));
        let api_base_status = probe_gateway_status(&format!("{url}/api/v1/"))
            .await
            .unwrap();
        assert_eq!(status, api_base_status);
    }

    #[tokio::test]
    async fn status_rejects_redirects_errors_and_historical_health_routes() {
        use axum::http::StatusCode;
        use axum::response::Redirect;

        let app = Router::new()
            .route("/health", get(|| async { "private-historical-body" }))
            .route(
                "/api/v1/health",
                get(|| async { Redirect::temporary("/health") }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let error = probe_gateway_status(&format!("http://{addr}"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("HTTP 307"));
        assert!(!error.to_string().contains("private-historical-body"));
        server.abort();

        let app = Router::new().route(
            "/api/v1/health",
            get(|| async { (StatusCode::SERVICE_UNAVAILABLE, "private-error-body") }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let error = probe_gateway_status(&format!("http://{addr}"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("HTTP 503"));
        assert!(!error.to_string().contains("private-error-body"));
        server.abort();
    }

    #[tokio::test]
    async fn status_rejects_ambiguous_or_credential_bearing_bases_without_echoing_them() {
        for base in [
            "file:///tmp/private-test-value",
            "http://private-test-value@127.0.0.1",
            "http://127.0.0.1?private-test-value",
            "http://127.0.0.1#private-test-value",
            "http://127.0.0.1/private-test-value",
        ] {
            let error = probe_gateway_status(base).await.unwrap_err();
            assert!(!error.to_string().contains("private-test-value"));
        }
    }

    #[tokio::test]
    async fn status_times_out_before_slow_health_headers_and_recovers_on_the_next_probe() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let slow = Arc::new(AtomicBool::new(true));
        let state = slow.clone();
        let app = Router::new().route(
            "/api/v1/health",
            get(move || {
                let state = state.clone();
                async move {
                    if state.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    }
                    "private-health-body"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let start = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(6),
            probe_gateway_status(&format!("http://{addr}")),
        )
        .await
        .unwrap();
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(6));
        slow.store(false, Ordering::SeqCst);
        assert!(probe_gateway_status(&format!("http://{addr}"))
            .await
            .is_ok());
        server.abort();
    }

    #[test]
    fn normalize_api_base_accepts_origin_and_api_base() {
        assert_eq!(
            normalize_api_base("https://api.example.test")
                .unwrap()
                .as_str(),
            "https://api.example.test/api/v1"
        );
        assert_eq!(
            normalize_api_base("https://api.example.test/api/v1/")
                .unwrap()
                .as_str(),
            "https://api.example.test/api/v1"
        );
    }

    #[test]
    fn onboard_and_limits_validate_inputs_without_network() {
        // Empty onboarding token / idempotency key and bad tenant ids fail before
        // any request is made, so no network is touched in these assertions.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(runtime
            .block_on(provision_tenant("http://127.0.0.1:1", "", "idem", None))
            .is_err());
        assert!(runtime
            .block_on(provision_tenant("http://127.0.0.1:1", "tok", "  ", None))
            .is_err());
        assert!(runtime
            .block_on(fetch_tenant_limits("http://127.0.0.1:1", "", "t_abc"))
            .is_err());
        assert!(runtime
            .block_on(fetch_tenant_limits("http://127.0.0.1:1", "key", "bad/id"))
            .is_err());
    }

    #[tokio::test]
    async fn onboard_provisions_and_verifies_the_minted_key_end_to_end() {
        use axum::http::HeaderMap;
        use axum::Json;
        use serde_json::json;

        // A minimal stand-in for the gateway's onboarding surface. It asserts the
        // bearer is the onboarding token, mints a key, and serves limits only for
        // the matching managed key — so the CLI's self-verification is exercised.
        async fn onboard(
            headers: HeaderMap,
            Json(body): Json<serde_json::Value>,
        ) -> Json<serde_json::Value> {
            assert_eq!(
                headers.get("authorization").and_then(|h| h.to_str().ok()),
                Some("Bearer onboarding-token")
            );
            assert!(body.get("idempotency_key").is_some());
            Json(json!({
                "tenant_id": "t_abc",
                "state": "active",
                "tier": "sandbox",
                "endpoint_url": "https://api.example.test/api/v1/agent?tenant=t_abc",
                "key_id": "0123456789ab",
                "api_key": "cxn_agent_minted"
            }))
        }
        async fn limits(headers: HeaderMap) -> Json<serde_json::Value> {
            assert_eq!(
                headers.get("authorization").and_then(|h| h.to_str().ok()),
                Some("Bearer cxn_agent_minted")
            );
            Json(json!({ "tier": "sandbox", "requests_per_minute": 60 }))
        }

        let app = Router::new()
            .route("/api/v1/onboard", axum::routing::post(onboard))
            .route("/api/v1/tenants/{tenant_id}/limits", get(limits));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let base = format!("http://{addr}");
        let receipt = provision_tenant(&base, "onboarding-token", "idem-1", Some("sandbox"))
            .await
            .unwrap();
        assert_eq!(receipt.tenant_id, "t_abc");
        assert_eq!(receipt.state, "active");
        assert_eq!(receipt.api_key.as_deref(), Some("cxn_agent_minted"));
        let limits = fetch_tenant_limits(
            &base,
            receipt.api_key.as_deref().unwrap(),
            &receipt.tenant_id,
        )
        .await
        .unwrap();
        assert_eq!(limits["tier"], "sandbox");
        server.abort();
    }

    #[tokio::test]
    async fn onboard_fails_closed_on_a_rejected_key_verification() {
        // Limits always 401: the CLI must treat the minted key as unusable.
        let app = Router::new().route(
            "/api/v1/tenants/{tenant_id}/limits",
            get(|| async { (axum::http::StatusCode::UNAUTHORIZED, "no") }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let error = fetch_tenant_limits(&format!("http://{addr}"), "cxn_agent_bad", "t_abc")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("HTTP 401"));
        server.abort();
    }
}
