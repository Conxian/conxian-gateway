//! Unified Installer & Management CLI for the Conxian Sovereign Settlement Suite.
//!
//! Subcommands:
//!   init     Generate `.env` and `docker-compose.yml` deployment templates.
//!   doctor   Run preflight diagnostic checks (secrets, trust tier, RPC, ports).
//!   start    Print startup instructions for the local gateway stack.
//!   status   Probe gateway `/api/v1/health` and `/metrics` endpoints.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const SENTINEL_API_TOKEN: &str = "sentinel_API_TOKEN";
const VALID_TRUST_TIERS: [&str; 4] = ["T1", "T2", "T3", "T4"];
const VALID_NETWORKS: [&str; 3] = ["mainnet", "testnet", "simulated"];
const VALID_MODES: [&str; 2] = ["production", "development"];
const DEFAULT_API_PORT: u16 = 3000;

/// Gateway deployment configuration. Mirrors the canonical `docker-compose.yml`
/// environment surface so `conxian-cli init` produces templates the gateway
/// actually reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallerConfig {
    pub api_token: String,
    pub network: String,
    pub trust_tier: String,
    pub mode: String,
    pub bitcoin_rpc_url: String,
    pub stacks_rpc_url: String,
    pub api_port: u16,
    pub gateway_state_path: String,
    pub persistence_mode: String,
    pub allow_unknown_state_filesystem: bool,
}

impl Default for InstallerConfig {
    fn default() -> Self {
        Self {
            api_token: SENTINEL_API_TOKEN.to_string(),
            network: "mainnet".to_string(),
            trust_tier: "T1".to_string(),
            mode: "production".to_string(),
            bitcoin_rpc_url: "https://bitcoin-rpc.publicnode.com".to_string(),
            stacks_rpc_url: "https://api.mainnet.hiro.so".to_string(),
            api_port: DEFAULT_API_PORT,
            gateway_state_path: "/usr/app/gateway_state.json".to_string(),
            persistence_mode: "exclusive-local-writer".to_string(),
            allow_unknown_state_filesystem: false,
        }
    }
}

impl InstallerConfig {
    /// Build config from process environment, falling back to defaults so
    /// `doctor`/`status` inspect the live deployment rather than placeholders.
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = env::var("API_TOKEN") {
            cfg.api_token = v;
        }
        if let Ok(v) = env::var("CONXIAN_NETWORK") {
            cfg.network = v;
        }
        if let Ok(v) = env::var("CONXIAN_TRUST_TIER") {
            cfg.trust_tier = v;
        }
        if let Ok(v) = env::var("BITCOIN_RPC_URL") {
            cfg.bitcoin_rpc_url = v;
        }
        if let Ok(v) = env::var("STACKS_RPC_URL") {
            cfg.stacks_rpc_url = v;
        }
        if let Ok(v) = env::var("API_PORT") {
            if let Ok(p) = v.parse() {
                cfg.api_port = p;
            }
        }
        if let Ok(v) = env::var("GATEWAY_STATE_PATH") {
            cfg.gateway_state_path = v;
        }
        if let Ok(v) = env::var("GATEWAY_PERSISTENCE_MODE") {
            cfg.persistence_mode = v;
        }
        if let Ok(v) = env::var("GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM") {
            cfg.allow_unknown_state_filesystem = v == "true";
        }
        cfg
    }
}

/// Render a `.env` file containing the environment the gateway actually reads.
pub fn generate_env_file(config: &InstallerConfig, output_path: &Path) -> Result<()> {
    let content = format!(
        "# Conxian Gateway Environment Configuration\n\
         API_TOKEN={}\n\
         CONXIAN_NETWORK={}\n\
         CONXIAN_TRUST_TIER={}\n\
         BITCOIN_RPC_URL={}\n\
         STACKS_RPC_URL={}\n\
         API_PORT={}\n\
         GATEWAY_STATE_PATH={}\n\
         GATEWAY_PERSISTENCE_MODE={}\n\
         GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM={}\n",
        config.api_token,
        config.network,
        config.trust_tier,
        config.bitcoin_rpc_url,
        config.stacks_rpc_url,
        config.api_port,
        config.gateway_state_path,
        config.persistence_mode,
        config.allow_unknown_state_filesystem
    );
    fs::write(output_path, content)
        .with_context(|| format!("Failed to write env file to {:?}", output_path))?;
    Ok(())
}

/// Render a single sovereign-lane `docker-compose.yml`. `version:` is omitted
/// (deprecated in modern Docker Compose); the image is versioned `latest`.
pub fn generate_docker_compose(config: &InstallerConfig, output_path: &Path) -> Result<()> {
    let content = format!(
        "services:\n\
         \x20 sovereign-node:\n\
         \x20   build: .\n\
         \x20   image: conxian-gateway:latest\n\
         \x20   container_name: conxian-sovereign-node\n\
         \x20   environment:\n\
         \x20     - API_TOKEN={api_token}\n\
         \x20     - CONXIAN_NETWORK={network}\n\
         \x20     - CONXIAN_TRUST_TIER={tier}\n\
         \x20     - BITCOIN_RPC_URL={btc}\n\
         \x20     - STACKS_RPC_URL={stx}\n\
         \x20     - GATEWAY_STATE_PATH={state}\n\
         \x20     - GATEWAY_PERSISTENCE_MODE={persist}\n\
         \x20     - GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM={allow_unknown}\n\
         \x20     - RUST_LOG=info\n\
         \x20   ports:\n\
         \x20     - \"{port}:3000\"\n\
         \x20   volumes:\n\
         \x20     - sovereign_data:/usr/app\n\
         \x20   restart: unless-stopped\n\n\
         volumes:\n\
         \x20 sovereign_data:\n",
        api_token = config.api_token,
        network = config.network,
        tier = config.trust_tier,
        btc = config.bitcoin_rpc_url,
        stx = config.stacks_rpc_url,
        state = config.gateway_state_path,
        persist = config.persistence_mode,
        allow_unknown = config.allow_unknown_state_filesystem,
        port = config.api_port,
    );
    fs::write(output_path, content)
        .with_context(|| format!("Failed to write docker-compose.yml to {:?}", output_path))?;
    Ok(())
}

#[derive(Debug)]
pub struct DoctorReport {
    pub secret_valid: bool,
    pub tier_valid: bool,
    pub network_valid: bool,
    pub btc_rpc_reachable: bool,
    pub stacks_rpc_reachable: bool,
    pub port_available: bool,
}

impl DoctorReport {
    pub fn all_pass(&self) -> bool {
        self.secret_valid
            && self.tier_valid
            && self.network_valid
            && self.btc_rpc_reachable
            && self.stacks_rpc_reachable
            && self.port_available
    }
}

/// Best-effort TCP reachability probe: resolve host:port and connect with a
/// short timeout. Avoids RPC auth, proving network reachability only.
fn tcp_reachable(url_str: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url_str) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addr = match (host, port).to_socket_addrs() {
        Ok(mut addrs) => addrs.next(),
        Err(_) => None,
    };
    match addr {
        Some(addr) => TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok(),
        None => false,
    }
}

pub fn run_doctor_checks(config: &InstallerConfig) -> DoctorReport {
    let secret_valid = !config.api_token.is_empty()
        && config.api_token != SENTINEL_API_TOKEN
        && !config.api_token.starts_with("sentinel_");
    let tier_valid = VALID_TRUST_TIERS.contains(&config.trust_tier.as_str());
    let network_valid = VALID_NETWORKS.contains(&config.network.as_str());
    let btc_rpc_reachable = url::Url::parse(&config.bitcoin_rpc_url).is_ok()
        && tcp_reachable(&config.bitcoin_rpc_url);
    let stacks_rpc_reachable = url::Url::parse(&config.stacks_rpc_url).is_ok()
        && tcp_reachable(&config.stacks_rpc_url);
    let port_available = TcpListener::bind(("127.0.0.1", config.api_port)).is_ok();

    DoctorReport {
        secret_valid,
        tier_valid,
        network_valid,
        btc_rpc_reachable,
        stacks_rpc_reachable,
        port_available,
    }
}

/// Synchronous HTTP GET via `minreq`, run on the blocking pool.
fn http_get(url: &str) -> Result<(u16, String)> {
    let resp = minreq::get(url)
        .with_timeout(5)
        .send()
        .with_context(|| format!("Failed to reach {url}"))?;
    let status = resp.status_code;
    let body = resp.as_str().unwrap_or("").to_string();
    Ok((status, body))
}

/// Probe a single gateway endpoint, returning a short human-readable line.
pub async fn probe_endpoint(base_url: &str, path: &str) -> String {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let owned = url.clone();
    match tokio::task::spawn_blocking(move || http_get(&owned)).await {
        Ok(Ok((200, body))) => {
            let clipped: String = body.chars().take(120).collect();
            format!("{path} -> 200 OK ({clipped})")
        }
        Ok(Ok((code, _))) => format!("{path} -> HTTP {code}"),
        Ok(Err(e)) => format!("{path} -> unreachable: {e}"),
        Err(e) => format!("{path} -> task join error: {e}"),
    }
}

/// Probe `/api/v1/health` and `/metrics` and report both.
pub async fn run_status(gateway_url: &str) {
    for path in ["/api/v1/health", "/metrics"] {
        println!("  [STATUS] {}", probe_endpoint(gateway_url, path).await);
    }
}

fn print_usage() {
    println!("Conxian Sovereign Settlement Suite - Unified Installer & CLI (v{VERSION})");
    println!("Usage: conxian-cli <subcommand> [options]\n");
    println!("Subcommands:");
    println!("  init     Generate deployment configuration (.env, docker-compose.yml)");
    println!("           Options: --tier T1|T2|T3|T4  --mode production|development  --network mainnet|testnet|simulated");
    println!("  doctor   Run preflight diagnostics (secret entropy, trust tier, RPC reachability, port availability)");
    println!("  start    Print startup instructions for the local gateway stack");
    println!("  status   Probe gateway /api/v1/health and /metrics endpoints");
}

/// Parse `--key value` / `--key=value` pairs (dependency-free, no clap).
fn parse_flags(args: &[String]) -> Vec<(String, String)> {
    let mut flags = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if let Some(rest) = arg.strip_prefix("--") {
            if let Some((k, v)) = rest.split_once('=') {
                flags.push((k.to_string(), v.to_string()));
            } else if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                flags.push((rest.to_string(), args[i + 1].clone()));
                i += 1;
            } else {
                flags.push((rest.to_string(), String::new()));
            }
        }
        i += 1;
    }
    flags
}

fn apply_flags(config: &mut InstallerConfig, flags: &[(String, String)]) -> Result<()> {
    for (k, v) in flags {
        match k.as_str() {
            "tier" => config.trust_tier = v.clone(),
            "mode" => config.mode = v.clone(),
            "network" => config.network = v.clone(),
            "port" => config.api_port = v.parse().context("--port must be a valid u16")?,
            _ => anyhow::bail!("unknown option --{k}"),
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

    let flags = parse_flags(&args[2..]);

    match args[1].as_str() {
        "init" => {
            let mut config = InstallerConfig::from_env();
            apply_flags(&mut config, &flags)?;

            if !VALID_TRUST_TIERS.contains(&config.trust_tier.as_str()) {
                anyhow::bail!("invalid --tier '{}' (expected T1|T2|T3|T4)", config.trust_tier);
            }
            if !VALID_NETWORKS.contains(&config.network.as_str()) {
                anyhow::bail!("invalid --network '{}' (expected mainnet|testnet|simulated)", config.network);
            }
            if !VALID_MODES.contains(&config.mode.as_str()) {
                anyhow::bail!("invalid --mode '{}' (expected production|development)", config.mode);
            }

            println!(
                "Initializing Conxian Sovereign Gateway deployment (tier {}, mode {}, network {})...",
                config.trust_tier, config.mode, config.network
            );
            let env_path = PathBuf::from(".env.production");
            let compose_path = PathBuf::from("docker-compose.yml");

            generate_env_file(&config, &env_path)?;
            generate_docker_compose(&config, &compose_path)?;

            println!("  [SUCCESS] Created env file: {:?}", env_path);
            println!("  [SUCCESS] Created Docker Compose file: {:?}", compose_path);
            println!("\nConfiguration ready. Run `conxian-cli doctor` to verify the environment.");
        }
        "doctor" => {
            println!("Running Conxian Preflight Diagnostic Checks...");
            let mut config = InstallerConfig::from_env();
            apply_flags(&mut config, &flags)?;
            let report = run_doctor_checks(&config);

            println!(
                "  - API Token Entropy (non-sentinel): {}",
                if report.secret_valid { "PASS" } else { "FAIL" }
            );
            println!(
                "  - Trust Tier (T1-T4): {}",
                if report.tier_valid { "PASS" } else { "FAIL" }
            );
            println!(
                "  - Network (mainnet/testnet/simulated): {}",
                if report.network_valid { "PASS" } else { "FAIL" }
            );
            println!(
                "  - Bitcoin L1 RPC Reachability: {}",
                if report.btc_rpc_reachable { "PASS" } else { "FAIL" }
            );
            println!(
                "  - Stacks L2 RPC Reachability: {}",
                if report.stacks_rpc_reachable { "PASS" } else { "FAIL" }
            );
            println!(
                "  - API Port {} Availability: {}",
                config.api_port,
                if report.port_available { "PASS" } else { "FAIL (in use)" }
            );

            if report.all_pass() {
                println!("\nAll preflight checks PASS. Ready for deployment.");
            } else {
                println!("\nPreflight warnings detected. Review the parameters above.");
            }
        }
        "start" => {
            println!("Starting Conxian Sovereign Gateway deployment stack...");
            println!("  Docker:  docker compose up -d");
            println!("  Binary:  cargo run --bin gateway");
            println!("Then run `conxian-cli status` to confirm the gateway is serving.");
        }
        "status" => {
            println!("Probing Conxian Gateway health and metrics...");
            let gateway_url =
                env::var("GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
            run_status(&gateway_url).await;
        }
        _ => print_usage(),
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
        assert!(content.contains("API_TOKEN=sentinel_API_TOKEN"));
        assert!(content.contains("CONXIAN_TRUST_TIER=T1"));
        assert!(content.contains("CONXIAN_NETWORK=mainnet"));
        assert!(content.contains("API_PORT=3000"));
        let _ = fs::remove_file(env_file);
    }

    #[test]
    fn test_generate_docker_compose() {
        let config = InstallerConfig::default();
        let temp_dir = std::env::temp_dir();
        let compose_file = temp_dir.join("test_compose.tmp");
        generate_docker_compose(&config, &compose_file).unwrap();

        let content = fs::read_to_string(&compose_file).unwrap();
        assert!(content.contains("image: conxian-gateway:latest"));
        assert!(content.contains("container_name: conxian-sovereign-node"));
        assert!(content.contains("API_TOKEN=sentinel_API_TOKEN"));
        assert!(!content.contains("version:"));
        let _ = fs::remove_file(compose_file);
    }

    #[test]
    fn test_doctor_checks_secret_and_tier() {
        let mut config = InstallerConfig::default();
        config.api_token = "real_production_token".to_string();
        config.trust_tier = "T3".to_string();
        let report = run_doctor_checks(&config);
        assert!(report.secret_valid);
        assert!(report.tier_valid);
        assert!(report.network_valid);

        config.api_token = "sentinel_API_TOKEN".to_string();
        let report2 = run_doctor_checks(&config);
        assert!(!report2.secret_valid);

        config.trust_tier = "T9".to_string();
        let report3 = run_doctor_checks(&config);
        assert!(!report3.tier_valid);
    }

    #[test]
    fn test_parse_flags() {
        let args = vec![
            "--tier".to_string(),
            "T2".to_string(),
            "--mode=production".to_string(),
        ];
        let flags = parse_flags(&args);
        assert_eq!(flags.len(), 2);
        assert_eq!(flags[0], ("tier".to_string(), "T2".to_string()));
        assert_eq!(flags[1], ("mode".to_string(), "production".to_string()));
    }

    #[tokio::test]
    async fn test_probe_endpoint_mock_server() {
        let app = Router::new().route("/api/v1/health", get(|| async { "OK" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = tx.send(());
            axum::serve(listener, app).await.unwrap();
        });
        rx.await.unwrap();

        let url = format!("http://{}", addr);
        let status = probe_endpoint(&url, "/api/v1/health").await;
        assert!(status.contains("200 OK"));
    }
}
