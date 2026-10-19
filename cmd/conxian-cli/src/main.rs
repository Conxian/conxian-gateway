use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct InstallerConfig {
    pub auth_token: String,
    pub trust_tier: String,
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
            bitcoin_rpc_url: "http://127.0.0.1:8332".to_string(),
            stacks_rpc_url: "https://api.mainnet.hiro.so".to_string(),
            database_url: "sqlite://gateway_state.db".to_string(),
            gateway_port: 8080,
            odata_v4_webhook_url: Some(
                "https://erp.client-domain.com/odata/v4/BankStatements".to_string(),
            ),
        }
    }
}

pub fn generate_env_file(config: &InstallerConfig, output_path: &Path) -> Result<()> {
    let content = format!(
        "# Conxian Gateway Environment Configuration\n\
         API_TOKEN={}\n\
         CONXIAN_TRUST_TIER={}\n\
         BITCOIN_RPC_URL={}\n\
         STACKS_RPC_URL={}\n\
         DATABASE_URL={}\n\
         GATEWAY_PORT={}\n\
         ODATA_V4_WEBHOOK_URL={}\n",
        config.auth_token,
        config.trust_tier,
        config.bitcoin_rpc_url,
        config.stacks_rpc_url,
        config.database_url,
        config.gateway_port,
        config.odata_v4_webhook_url.as_deref().unwrap_or("")
    );
    fs::write(output_path, content)
        .with_context(|| format!("Failed to write env file to {:?}", output_path))?;
    Ok(())
}

pub fn generate_docker_compose(config: &InstallerConfig, output_path: &Path) -> Result<()> {
    let content = format!(
        "version: '3.8'\n\n\
         services:\n\
         \x20 conxian-gateway:\n\
         \x20   image: conxian/gateway:v0.1.5\n\
         \x20   container_name: conxian-gateway\n\
         \x20   restart: unless-stopped\n\
         \x20   ports:\n\
         \x20     - \"{}:8080\"\n\
         \x20   environment:\n\
         \x20     - API_TOKEN={}\n\
         \x20     - CONXIAN_TRUST_TIER={}\n\
         \x20     - BITCOIN_RPC_URL={}\n\
         \x20     - STACKS_RPC_URL={}\n\
         \x20     - DATABASE_URL={}\n\
         \x20   volumes:\n\
         \x20     - gateway_data:/var/lib/conxian\n\n\
         volumes:\n\
         \x20 gateway_data:\n",
        config.gateway_port,
        config.auth_token,
        config.trust_tier,
        config.bitcoin_rpc_url,
        config.stacks_rpc_url,
        config.database_url
    );
    fs::write(output_path, content)
        .with_context(|| format!("Failed to write docker-compose.yml to {:?}", output_path))?;
    Ok(())
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

pub async fn probe_gateway_status(gateway_url: &str) -> Result<String> {
    let mut health_url = url::Url::parse(gateway_url).context("Invalid Gateway base URL")?;
    anyhow::ensure!(
        matches!(health_url.scheme(), "http" | "https")
            && health_url.host_str().is_some()
            && health_url.username().is_empty()
            && health_url.password().is_none()
            && health_url.query().is_none()
            && health_url.fragment().is_none()
            && matches!(health_url.path().trim_end_matches('/'), "" | "/api/v1"),
        "Gateway base URL must be an HTTP(S) origin or /api/v1 base without credentials, query or fragment"
    );
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
    println!("  start    Print startup instructions or execute local gateway docker stack");
    println!("  status   Probe Gateway HTTP liveness only");
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
            println!("For docker deployment, run: docker compose up -d");
            println!("For binary deployment, run: cargo run --bin gateway");
        }
        "status" => {
            println!("Probing Conxian Gateway Health...");
            let gateway_url =
                env::var("GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
            let message = probe_gateway_status(&gateway_url).await?;
            println!("  [STATUS] {}", message);
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
        assert!(content.contains("API_TOKEN=CONXIAN_PROD_AUTH_TOKEN_REQUIRED"));
        assert!(!content.contains("CONXIAN_GATEWAY_AUTH_TOKEN="));
        assert!(content.contains("CONXIAN_TRUST_TIER=T1"));
        assert!(content.contains("GATEWAY_PORT=8080"));
        let _ = fs::remove_file(env_file);
    }

    #[test]
    fn test_generate_docker_compose() {
        let config = InstallerConfig::default();
        let temp_dir = std::env::temp_dir();
        let compose_file = temp_dir.join("test_compose.tmp");
        generate_docker_compose(&config, &compose_file).unwrap();

        let content = fs::read_to_string(&compose_file).unwrap();
        assert!(content.contains("image: conxian/gateway:v0.1.5"));
        assert!(content.contains("container_name: conxian-gateway"));
        assert!(content.contains("- API_TOKEN=CONXIAN_PROD_AUTH_TOKEN_REQUIRED"));
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
}
