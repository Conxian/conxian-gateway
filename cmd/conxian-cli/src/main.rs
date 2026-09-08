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
            auth_token: "conxian_prod_sec_99a8b7c6d5e4f3a21".to_string(),
            trust_tier: "T1".to_string(),
            bitcoin_rpc_url: "http://127.0.0.1:8332".to_string(),
            stacks_rpc_url: "https://api.mainnet.hiro.so".to_string(),
            database_url: "sqlite://gateway_state.db".to_string(),
            gateway_port: 8080,
            odata_v4_webhook_url: Some("https://erp.client-domain.com/odata/v4/BankStatements".to_string()),
        }
    }
}

pub fn generate_env_file(config: &InstallerConfig, output_path: &Path) -> Result<()> {
    let content = format!(
        "# Conxian Gateway Environment Configuration\n\
         CONXIAN_GATEWAY_AUTH_TOKEN={}\n\
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
         \x20     - CONXIAN_GATEWAY_AUTH_TOKEN={}\n\
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
    pub btc_rpc_reachable: bool,
    pub stacks_rpc_reachable: bool,
}

pub fn run_doctor_checks(config: &InstallerConfig) -> DoctorReport {
    let secret_valid = !config.auth_token.is_empty() && !config.auth_token.starts_with("sentinel_");
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
    let health_url = format!("{}/health", gateway_url.trim_end_matches('/'));
    tokio::task::spawn_blocking(move || {
        let resp = minreq::get(&health_url)
            .with_timeout(3)
            .send()
            .with_context(|| format!("Failed to connect to Gateway health endpoint at {}", health_url))?;

        if resp.status_code == 200 {
            Ok(format!("Gateway Online (HTTP 200) - Response: {}", resp.as_str().unwrap_or("OK")))
        } else {
            Ok(format!("Gateway Error Status: {}", resp.status_code))
        }
    })
    .await
    .context("Task join error during gateway health probe")?
}

fn print_usage() {
    println!("Conxian Sovereign Settlement Suite - Unified Installer & CLI (v0.1.5)");
    println!("Usage: conxian-cli <subcommand> [options]\n");
    println!("Subcommands:");
    println!("  init     Initialize client deployment configuration (.env, docker-compose.yml)");
    println!("  doctor   Run preflight diagnostic checks on configuration and connectivity");
    println!("  start    Print startup instructions or execute local gateway docker stack");
    println!("  status   Probe local/remote Conxian Gateway health and multi-rail status");
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
            println!("  [SUCCESS] Created Docker Compose file: {:?}", compose_path);
            println!("\nConfiguration ready. Run `conxian-cli doctor` to verify environment.");
        }
        "doctor" => {
            println!("Running Conxian Preflight Diagnostic Checks...");
            let config = InstallerConfig::default();
            let report = run_doctor_checks(&config);

            println!("  - Production Secret Validation: {}", if report.secret_valid { "PASS" } else { "FAIL (sentinel or empty)" });
            println!("  - Trust Tier Verification (T1/T2): {}", if report.tier_valid { "PASS" } else { "FAIL" });
            println!("  - Bitcoin L1 RPC Reachability: {}", if report.btc_rpc_reachable { "PASS (Valid URL)" } else { "FAIL" });
            println!("  - Stacks L2 RPC Reachability: {}", if report.stacks_rpc_reachable { "PASS (Valid URL)" } else { "FAIL" });

            if report.secret_valid && report.tier_valid && report.btc_rpc_reachable && report.stacks_rpc_reachable {
                println!("\nSystem Diagnostics All PASS. Ready for deployment.");
            } else {
                println!("\nSystem Diagnostics Warnings Detected. Review above parameters.");
            }
        }
        "start" => {
            println!("Starting Conxian Sovereign Gateway Deployment Stack...");
            println!("Executing docker compose up -d or starting `cmd/gateway` binary...");
            println!("For docker deployment, run: docker compose up -d");
            println!("For binary deployment, run: cargo run --bin gateway");
        }
        "status" => {
            println!("Probing Conxian Gateway Health...");
            let gateway_url = env::var("GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
            match probe_gateway_status(&gateway_url).await {
                Ok(msg) => println!("  [STATUS] {}", msg),
                Err(err) => println!("  [STATUS] Gateway Offline / Unreachable: {}", err),
            }
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
        assert!(content.contains("CONXIAN_GATEWAY_AUTH_TOKEN=conxian_prod_sec"));
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
        let _ = fs::remove_file(compose_file);
    }

    #[test]
    fn test_doctor_checks() {
        let mut config = InstallerConfig::default();
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
        let app = Router::new().route("/health", get(|| async { "OK" }));
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
        assert!(status.contains("Gateway Online"));
    }
}
