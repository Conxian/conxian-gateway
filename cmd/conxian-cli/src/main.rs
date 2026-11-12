use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// CLI version, sourced from the workspace so it cannot drift from the package.
pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Global output mode. `--json` makes every command machine-readable so an
/// autonomous agent (the primary ICP) can consume the CLI without parsing prose.
#[derive(Clone, Copy, PartialEq)]
pub enum OutputMode {
    Human,
    Json,
}

/// A single place to print a result line, so `--json` and human output cannot
/// diverge. In JSON mode each call emits one JSON object per line (JSONL).
pub fn emit(mode: OutputMode, level: &str, message: &str) {
    match mode {
        OutputMode::Json => {
            let line = serde_json::json!({ "level": level, "message": message });
            println!("{line}");
        }
        OutputMode::Human => println!("{message}"),
    }
}

/// Emits a structured JSON payload (the command's result) in JSON mode, or a
/// human summary in human mode.
pub fn emit_data(mode: OutputMode, value: &serde_json::Value) {
    if mode == OutputMode::Json {
        println!("{value}");
    }
}

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

/// Prompt for confirmation on stderr/stdin. Returns false when stdin is not a
/// terminal (so a non-interactive/agent run never deletes without `--force`).
fn confirm_removal(path: &Path) -> bool {
    use std::io::{IsTerminal, Write as _};
    if !std::io::stdin().is_terminal() {
        return false;
    }
    print!("Remove {}? [y/N] ", path.display());
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
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

/// Public doctor output deliberately excludes secret state. The CLI's default
/// installer template is not an authenticated environment read, so reporting
/// token validity would be both misleading and a sensitive log disclosure.
fn doctor_configuration_ok(
    report: &DoctorReport,
    btc_probe: Option<bool>,
    stacks_probe: Option<bool>,
) -> bool {
    let btc_ok = report.btc_rpc_reachable && btc_probe.unwrap_or(true);
    let stacks_ok = report.stacks_rpc_reachable && stacks_probe.unwrap_or(true);
    report.tier_valid && btc_ok && stacks_ok
}

/// Human label for an RPC check: syntax-only, or syntax + (optional) reachability.
fn describe_rpc(syntax_ok: bool, probed: Option<bool>) -> String {
    match (syntax_ok, probed) {
        (false, _) => "FAIL (invalid URL)".to_string(),
        (true, None) => "PASS (valid URL)".to_string(),
        (true, Some(true)) => "PASS (reachable)".to_string(),
        (true, Some(false)) => "FAIL (unreachable)".to_string(),
    }
}

/// Connectivity probe for an RPC endpoint: a short TCP connect + HTTP POST of a
/// minimal JSON-RPC `getblockchaininfo`/`getinfo`-style body. Returns Ok(true) on
/// any HTTP response (reachable), Ok(false) on a transport failure, and never
/// reads or echoes the response body. Used by `doctor --probe` to distinguish a
/// reachable endpoint from a syntactically-valid one. Read-only; no auth is
/// attempted (an authenticated endpoint returns 401, which still proves
/// reachability).
pub async fn probe_rpc_reachable(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return false;
    }
    let target = parsed.as_str().to_string();
    tokio::task::spawn_blocking(move || {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"getblockchaininfo","params":[]}"#;
        // Any HTTP status (including 401/405) proves the host answered.
        minreq::post(&target)
            .with_header("content-type", "application/json")
            .with_body(body)
            .with_timeout(4)
            .with_follow_redirects(false)
            .with_max_headers_size(8192)
            .send_lazy()
            .is_ok()
    })
    .await
    .unwrap_or(false)
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

/// A generic authenticated JSON request to the gateway API. Returns the HTTP
/// status and the raw body text. `bearer` may be empty for unauthenticated
/// endpoints (e.g. `/billing/quote`). Never logs or echoes the bearer.
pub async fn api_request(
    gateway_base_url: &str,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<serde_json::Value>,
) -> Result<(u16, String)> {
    let mut url = normalize_api_base(gateway_base_url)?;
    url.set_path(path);
    let target = url.to_string();
    let method = method.to_string();
    let bearer = bearer.unwrap_or("").to_string();
    let body = body.map(|b| b.to_string());
    tokio::task::spawn_blocking(move || {
        let mut req = match method.as_str() {
            "GET" => minreq::get(target.as_str()),
            "POST" => minreq::post(target.as_str()),
            "DELETE" => minreq::delete(target.as_str()),
            other => return Err(anyhow::anyhow!("Unsupported HTTP method {other}")),
        };
        if !bearer.is_empty() {
            req = req.with_header("Authorization", format!("Bearer {bearer}"));
        }
        if let Some(body) = body {
            req = req
                .with_header("Content-Type", "application/json")
                .with_body(body);
        }
        let mut resp = req
            .with_timeout(15)
            .with_follow_redirects(false)
            .with_max_headers_size(16384)
            .with_max_status_line_length(1024)
            .send_lazy()
            .map_err(|_| anyhow::anyhow!("Gateway request failed"))?;
        let status = resp.status_code;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut resp, &mut text)
            .map_err(|_| anyhow::anyhow!("Failed to read response body"))?;
        Ok::<_, anyhow::Error>((status, text))
    })
    .await
    .context("Task join error during gateway request")?
}

/// Path of the persistent CLI profile store (`~/.config/conxian/config.json`).
/// Holds the default gateway URL and the managed key path, so a customer does
/// not have to export environment variables for every invocation.
pub fn config_path() -> Option<PathBuf> {
    let base = env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("conxian").join("config.json"))
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub gateway_url: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
}

pub fn load_profile() -> Profile {
    config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_profile(profile: &Profile) -> Result<()> {
    let path = config_path().context("No HOME/XDG_CONFIG_HOME to store CLI config")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Cannot create CLI config directory")?;
    }
    let json = serde_json::to_string_pretty(profile).context("Failed to encode profile")?;
    fs::write(&path, json).context("Failed to write CLI config")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Resolves the gateway base URL from (in order): an explicit flag, the
/// `GATEWAY_URL` env var, the stored profile, then localhost.
pub fn resolve_gateway(flags: &std::collections::HashMap<String, String>) -> String {
    flags
        .get("gateway")
        .cloned()
        .or_else(|| env::var("GATEWAY_URL").ok())
        .or_else(|| load_profile().gateway_url)
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string())
}

/// Per-subcommand help. Mirrors docker/kubectl/gh: `conxian-cli <cmd> --help`
/// shows the flags and behavior for that command only.
fn print_command_help(topic: &str) {
    let text = match topic {
        "init" => "conxian-cli init\n\n  Write hardened .env.production + docker-compose.yml templates.\n  Refuses to overwrite an existing file. No credentials or readiness are\n  established; complete the secret and persistence contract before use.",
        "uninstall" => "conxian-cli uninstall [--force]\n\n  Remove the templates generated by `init`. Removes a file only if it matches\n  the generated template; a user-authored file is left in place. `--force`\n  skips the interactive prompt (required when stdin is not a terminal).\n  The gateway data volume and any database are NOT removed.",
        "doctor" => "conxian-cli doctor [--probe] [--json]\n\n  Preflight checks: API token configured, trust tier, RPC URL syntax.\n  --probe adds a real reachability check for the Bitcoin and Stacks RPC\n  endpoints (a short TCP+HTTP probe; never reads the response body).",
        "start" => "conxian-cli start\n\n  Print the startup instructions. It does NOT launch a process.",
        "status" => "conxian-cli status [--gateway <url>]\n\n  Probe the gateway liveness endpoint (or the site's multi-service /api/status).",
        "onboard" => "conxian-cli onboard [--tier <tier>] [--write-env <path>]\n\n  Provision a tenant (POST /onboard) and verify the minted key. The onboarding\n  token is read from $ONBOARDING_TOKEN only (never a flag).",
        "deploy" => "conxian-cli deploy --tenant <id> [--key-file <path>]\n\n  Deploy a provisioned tenant (POST /tenants/{id}/deploy).",
        "limits" | "usage" => "conxian-cli <limits|usage> --tenant <id> [--key-file <path>]\n\n  Show a tenant's published limits / metered usage. Requires a managed key.",
        "keys" => "conxian-cli keys <rotate|revoke> --tenant <id> [--key-file <path>]\n\n  Rotate or revoke a tenant key. Requires a managed key.",
        "quote" | "subscribe" | "verify" | "balance" => "conxian-cli <quote|subscribe|verify|balance> [--plan <plan>] [--tenant <id>]\n\n  Billing via the Lightning subscription rail. `quote` needs no account.",
        "config" => "conxian-cli config <get|set|show> [gateway|tenant|key-file] [value]\n\n  Manage the persistent CLI profile (~/.config/conxian/config.json).",
        "version" => "conxian-cli version\n\n  Print the CLI version.",
        "help" => "conxian-cli help [<subcommand>]\n\n  Show the overview, or per-subcommand help.",
        _ => {
            println!("No help for '{topic}'.");
            println!();
            print_usage();
            return;
        }
    };
    println!("{text}");
    println!("\nRun `conxian-cli --help` for the full command list.");
}

fn print_usage() {
    println!("Conxian Sovereign Settlement Suite - Unified Installer & CLI (v{CLI_VERSION})");
    println!("Usage: conxian-cli <subcommand> [options]");
    println!("Global flags: --json (machine-readable output), --help, --version\n");
    println!("Deployment:");
    println!("  init      Write .env.production + docker-compose.yml templates");
    println!("  doctor    Preflight checks (syntax; --probe adds RPC reachability)");
    println!("  start     Print startup instructions (does not launch a process)");
    println!("  uninstall Remove the generated templates (--force skips the prompt)\n");
    println!("Diagnostics:");
    println!("  version   Print the CLI version\n");
    println!("Account & provisioning:");
    println!("  onboard   Provision a tenant (POST /onboard) and verify the minted key");
    println!("  deploy    Deploy a provisioned tenant (POST /tenants/{{id}}/deploy)");
    println!("  status    Probe the gateway, or the site's multi-service /api/status\n");
    println!("Self-management (requires a managed key):");
    println!("  limits    Show a tenant's published limits");
    println!("  usage     Show a tenant's metered usage");
    println!("  keys      rotate | revoke a tenant key\n");
    println!("Billing (the Lightning subscription rail):");
    println!("  quote     Price a plan (no account needed)");
    println!("  subscribe Create a Lightning invoice for a plan");
    println!("  verify    Verify a settled invoice and mint the key");
    println!("  balance   Show a tenant's credit balance\n");
    println!("Configuration:");
    println!("  config    get | set | show the persistent CLI profile\n");
    println!("Common flags:");
    println!("  --gateway <url>   Gateway base URL (default: $GATEWAY_URL, profile, or :8080)");
    println!("  --tenant <id>     Tenant id");
    println!("  --key-file <path> File holding the managed key (0600)");
    println!("  --tier <tier>     sandbox | starter | indie | business | enterprise");
    println!("  --plan <plan>     Billing plan (quote/subscribe/verify)");
    println!("  --json            Emit machine-readable JSON output");
    println!("  Onboarding token is read from $ONBOARDING_TOKEN only (never a flag).");
}

/// Parses `--flag value` pairs into a map. A flag whose next token is another
/// flag (or absent) is stored with an empty value, so bare boolean flags like
/// `--probe` and `--json` are detected via `contains_key`. Unknown flags are
/// ignored so the command stays forward-compatible.
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
    // rustls 0.23 cannot auto-select a CryptoProvider when more than one provider
    // feature is compiled into the process (the workspace enables both
    // `aws-lc-rs` via reqwest and `ring` via async-wsocket). Without an explicit
    // default, every HTTPS call panics. Install one up front.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    // Global flags handled before dispatch so every command supports them.
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("conxian-cli {CLI_VERSION}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        // Per-subcommand help (`doctor --help`) shows that command's flags;
        // bare `--help` shows the overview.
        match args.get(1).map(String::as_str) {
            Some(topic) if !topic.starts_with('-') => print_command_help(topic),
            _ => print_usage(),
        }
        return Ok(());
    }
    let mode = if args.iter().any(|a| a == "--json") {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    match args[1].as_str() {
        "uninstall" => {
            // Reverse of `init`: remove the generated files. Refuses to touch
            // anything it did not create (a path it cannot confirm is a
            // generated template is left in place), and never touches the
            // gateway data volume or a database.
            let flags = parse_flags(&args[2..]);
            let force = flags.contains_key("force");
            let targets = [
                (PathBuf::from(".env.production"), "API_TOKEN="),
                (PathBuf::from("docker-compose.yml"), "conxian-gateway:"),
            ];
            let mut removed = Vec::new();
            let mut skipped = Vec::new();
            for (path, marker) in targets {
                match fs::read_to_string(&path) {
                    Ok(text) if text.contains(marker) => {
                        if force || confirm_removal(&path) {
                            fs::remove_file(&path)
                                .with_context(|| format!("Failed to remove {}", path.display()))?;
                            removed.push(path.display().to_string());
                        } else {
                            skipped.push(format!("{} (declined)", path.display()));
                        }
                    }
                    Ok(_) => skipped.push(format!("{} (not a generated template)", path.display())),
                    Err(_) => skipped.push(format!("{} (absent)", path.display())),
                }
            }
            if mode == OutputMode::Json {
                let value = serde_json::json!({
                    "command": "uninstall",
                    "removed": removed,
                    "skipped": skipped,
                    "note": "only generated template files are removed; the gateway data volume and database are untouched",
                });
                emit_data(mode, &value);
            } else {
                for r in &removed {
                    println!("  [REMOVED] {r}");
                }
                for s in &skipped {
                    println!("  [SKIPPED] {s}");
                }
                println!("\nGenerated template files only. The gateway data volume and any database are NOT removed.");
                println!(
                    "To remove them: `docker compose down -v` and drop the database manually."
                );
            }
        }
        "version" => {
            if mode == OutputMode::Json {
                emit_data(
                    mode,
                    &serde_json::json!({ "command": "version", "version": CLI_VERSION }),
                );
            } else {
                println!("conxian-cli {CLI_VERSION}");
            }
        }
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
            let flags = parse_flags(&args[2..]);
            let probe = flags.contains_key("probe");
            let config = InstallerConfig::default();
            let report = run_doctor_checks(&config);

            // Optional connectivity probe: distinguishes a reachable endpoint
            // from a syntactically-valid one. Never reads the response body.
            let (btc_probe, stacks_probe) = if probe {
                (
                    Some(probe_rpc_reachable(&config.bitcoin_rpc_url).await),
                    Some(probe_rpc_reachable(&config.stacks_rpc_url).await),
                )
            } else {
                (None, None)
            };

            let all_ok = doctor_configuration_ok(&report, btc_probe, stacks_probe);

            if mode == OutputMode::Json {
                let value = serde_json::json!({
                    "command": "doctor",
                    "probed": probe,
                    "secret_validation": "not_inspected",
                    "tier_valid": report.tier_valid,
                    "bitcoin_rpc_syntax_ok": report.btc_rpc_reachable,
                    "bitcoin_rpc_reachable": btc_probe,
                    "stacks_rpc_syntax_ok": report.stacks_rpc_reachable,
                    "stacks_rpc_reachable": stacks_probe,
                    "ok": all_ok,
                    "note": "trust-tier and RPC syntax/optional reachability only; secret configuration is not inspected and deployment readiness is not established",
                });
                emit_data(mode, &value);
            } else {
                println!("Running Conxian Preflight Diagnostic Checks...");
                println!("  - Secret configuration: NOT INSPECTED (secret values and validity are not logged)");
                println!(
                    "  - Trust Tier Verification (T1/T2): {}",
                    if report.tier_valid { "PASS" } else { "FAIL" }
                );
                println!(
                    "  - Bitcoin RPC URL syntax{}: {}",
                    if probe {
                        " + reachability"
                    } else {
                        " (connectivity not tested)"
                    },
                    describe_rpc(report.btc_rpc_reachable, btc_probe)
                );
                println!(
                    "  - Stacks RPC URL syntax{}: {}",
                    if probe {
                        " + reachability"
                    } else {
                        " (connectivity not tested)"
                    },
                    describe_rpc(report.stacks_rpc_reachable, stacks_probe)
                );
                if all_ok {
                    println!(
                        "\nConfiguration checks passed; deployment readiness is not established."
                    );
                } else {
                    println!("\nSystem Diagnostics Warnings Detected. Review above parameters.");
                }
            }

            if !all_ok {
                anyhow::bail!("Installer configuration is incomplete or an endpoint was unreachable; runtime readiness was not tested");
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
        "deploy" => {
            run_deploy(&args[2..], mode).await?;
        }
        "limits" => {
            run_tenant_get(&args[2..], mode, "limits").await?;
        }
        "usage" => {
            run_tenant_get(&args[2..], mode, "usage").await?;
        }
        "keys" => {
            run_keys(&args[2..], mode).await?;
        }
        "quote" => {
            run_billing(&args[2..], mode, "quote").await?;
        }
        "subscribe" => {
            run_billing(&args[2..], mode, "subscribe").await?;
        }
        "verify" => {
            run_billing(&args[2..], mode, "verify").await?;
        }
        "balance" => {
            run_billing(&args[2..], mode, "balance").await?;
        }
        "config" => {
            run_config(&args[2..], mode)?;
        }
        "help" | "--help" | "-h" => match args.get(2).map(String::as_str) {
            Some(topic) => print_command_help(topic),
            None => print_usage(),
        },
        other => {
            // An unknown subcommand must fail (non-zero), not print usage and
            // exit 0 — matching docker/kubectl/gh behaviour.
            print_usage();
            anyhow::bail!("unknown subcommand: {other}");
        }
    }

    Ok(())
}

/// Reads the managed key from `--key-file`, or the profile's key file, or
/// `GATEWAY_MANAGED_API_KEY`. The key is never taken from a flag.
fn resolve_managed_key(flags: &std::collections::HashMap<String, String>) -> Result<String> {
    if let Some(path) = flags.get("key-file") {
        let content =
            fs::read_to_string(path).with_context(|| format!("Cannot read key file {path}"))?;
        return extract_env_value(&content, "GATEWAY_MANAGED_API_KEY")
            .or_else(|| Some(content.trim().to_string()).filter(|s| !s.is_empty()))
            .context("Key file did not contain a managed key");
    }
    if let Some(path) = load_profile().key_file {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(key) = extract_env_value(&content, "GATEWAY_MANAGED_API_KEY") {
                return Ok(key);
            }
        }
    }
    env::var("GATEWAY_MANAGED_API_KEY").context(
        "No managed key: pass --key-file, set the profile key_file, or set GATEWAY_MANAGED_API_KEY",
    )
}

/// Extracts `KEY=value` from a dotenv-style file, stripping surrounding quotes.
fn extract_env_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&format!("{key}=")) {
            return Some(rest.trim().trim_matches('\'').trim_matches('"').to_string());
        }
    }
    None
}

fn resolve_tenant(flags: &std::collections::HashMap<String, String>) -> Result<String> {
    flags
        .get("tenant")
        .cloned()
        .or_else(|| env::var("GATEWAY_TENANT_ID").ok())
        .or_else(|| load_profile().tenant_id)
        .context("No tenant id: pass --tenant or set GATEWAY_TENANT_ID")
}

/// `deploy` — POST /tenants/{id}/deploy (operator-token gated).
async fn run_deploy(args: &[String], mode: OutputMode) -> Result<()> {
    let flags = parse_flags(args);
    let gateway = resolve_gateway(&flags);
    let tenant = resolve_tenant(&flags)?;
    validate_tenant_id(&tenant)?;
    let token = env::var("ONBOARDING_TOKEN")
        .context("ONBOARDING_TOKEN must be set (the CLI never takes it as a flag)")?;
    let (status, text) = api_request(
        &gateway,
        "POST",
        &format!("/api/v1/tenants/{tenant}/deploy"),
        Some(&token),
        Some(serde_json::json!({})),
    )
    .await?;
    anyhow::ensure!(status == 200, "Deploy returned HTTP {status}");
    emit(mode, "success", &format!("Deploy accepted for {tenant}"));
    emit_data(
        mode,
        &serde_json::json!({ "tenant_id": tenant, "status": status, "response": text }),
    );
    Ok(())
}

/// `limits` / `usage` — GET /tenants/{id}/{limits|usage} (managed-key gated).
async fn run_tenant_get(args: &[String], mode: OutputMode, what: &str) -> Result<()> {
    let flags = parse_flags(args);
    let gateway = resolve_gateway(&flags);
    let tenant = resolve_tenant(&flags)?;
    validate_tenant_id(&tenant)?;
    let key = resolve_managed_key(&flags)?;
    let (status, text) = api_request(
        &gateway,
        "GET",
        &format!("/api/v1/tenants/{tenant}/{what}"),
        Some(&key),
        None,
    )
    .await?;
    anyhow::ensure!(
        status == 200,
        "{what} returned HTTP {status} (key rejected?)"
    );
    emit(mode, "success", &format!("{what} for {tenant}: {text}"));
    emit_data(
        mode,
        &serde_json::json!({ "tenant_id": tenant, "result": text }),
    );
    Ok(())
}

/// `keys rotate|revoke` — POST/DELETE /tenants/{id}/keys[/{key_id}].
async fn run_keys(args: &[String], mode: OutputMode) -> Result<()> {
    let action = args.first().map(String::as_str).unwrap_or("");
    let flags = parse_flags(args);
    let gateway = resolve_gateway(&flags);
    let tenant = resolve_tenant(&flags)?;
    validate_tenant_id(&tenant)?;
    let key = resolve_managed_key(&flags)?;
    match action {
        "rotate" => {
            let (status, text) = api_request(
                &gateway,
                "POST",
                &format!("/api/v1/tenants/{tenant}/keys"),
                Some(&key),
                Some(serde_json::json!({})),
            )
            .await?;
            anyhow::ensure!(status == 200, "Key rotation returned HTTP {status}");
            emit(mode, "success", &format!("Rotated key for {tenant}"));
            emit_data(
                mode,
                &serde_json::json!({ "tenant_id": tenant, "result": text }),
            );
        }
        "revoke" => {
            let key_id = flags
                .get("key-id")
                .context("--key-id is required for revoke")?;
            let (status, text) = api_request(
                &gateway,
                "DELETE",
                &format!("/api/v1/tenants/{tenant}/keys/{key_id}"),
                Some(&key),
                None,
            )
            .await?;
            anyhow::ensure!(status == 200, "Key revocation returned HTTP {status}");
            emit(
                mode,
                "success",
                &format!("Revoked key {key_id} for {tenant}"),
            );
            emit_data(
                mode,
                &serde_json::json!({ "tenant_id": tenant, "key_id": key_id, "result": text }),
            );
        }
        _ => anyhow::bail!("keys expects 'rotate' or 'revoke'"),
    }
    Ok(())
}

/// `quote|subscribe|verify|balance` — the gateway's Lightning billing rail.
async fn run_billing(args: &[String], mode: OutputMode, what: &str) -> Result<()> {
    let flags = parse_flags(args);
    let gateway = resolve_gateway(&flags);
    match what {
        "quote" | "subscribe" => {
            let plan = flags.get("plan").context("--plan is required")?;
            let (status, text) = api_request(
                &gateway,
                "POST",
                &format!("/api/v1/billing/{what}"),
                None,
                Some(serde_json::json!({ "plan": plan })),
            )
            .await?;
            anyhow::ensure!(status == 200, "{what} returned HTTP {status}");
            emit(mode, "success", &format!("{what} {plan}: {text}"));
            emit_data(mode, &serde_json::json!({ "plan": plan, "result": text }));
        }
        "verify" => {
            let plan = flags.get("plan").context("--plan is required")?;
            let tenant = resolve_tenant(&flags)?;
            let invoice_id = flags
                .get("invoice-id")
                .context("--invoice-id is required")?;
            let (status, text) = api_request(
                &gateway, "POST", "/api/v1/billing/verify", None,
                Some(serde_json::json!({ "plan": plan, "tenant_id": tenant, "invoice_id": invoice_id })),
            ).await?;
            // 402 means "not settled yet" — a normal, non-fatal state.
            if status == 402 {
                emit(mode, "info", "Invoice not settled yet");
                emit_data(
                    mode,
                    &serde_json::json!({ "settled": false, "detail": text }),
                );
                return Ok(());
            }
            anyhow::ensure!(status == 200, "verify returned HTTP {status}");
            emit(mode, "success", "Invoice settled; managed key minted");
            emit_data(
                mode,
                &serde_json::json!({ "settled": true, "result": text }),
            );
        }
        "balance" => {
            let tenant = resolve_tenant(&flags)?;
            let (status, text) = api_request(
                &gateway,
                "GET",
                &format!("/api/v1/billing/balance?tenant_id={tenant}"),
                None,
                None,
            )
            .await?;
            anyhow::ensure!(status == 200, "balance returned HTTP {status}");
            emit(mode, "success", &format!("balance for {tenant}: {text}"));
            emit_data(
                mode,
                &serde_json::json!({ "tenant_id": tenant, "result": text }),
            );
        }
        _ => anyhow::bail!("unknown billing subcommand {what}"),
    }
    Ok(())
}

/// `config get|set|show` — the persistent CLI profile.
fn run_config(args: &[String], mode: OutputMode) -> Result<()> {
    let action = args.first().map(String::as_str).unwrap_or("show");
    let flags = parse_flags(args);
    let mut profile = load_profile();
    match action {
        "set" => {
            if let Some(v) = flags.get("gateway") {
                profile.gateway_url = Some(v.clone());
            }
            if let Some(v) = flags.get("tenant") {
                profile.tenant_id = Some(v.clone());
            }
            if let Some(v) = flags.get("key-file") {
                profile.key_file = Some(v.clone());
            }
            save_profile(&profile)?;
            emit(mode, "success", "Profile saved");
            emit_data(
                mode,
                &serde_json::json!({ "gateway_url": profile.gateway_url, "tenant_id": profile.tenant_id, "key_file": profile.key_file }),
            );
        }
        "get" => {
            let key = args.get(1).map(String::as_str).unwrap_or("");
            let value = match key {
                "gateway" => profile.gateway_url,
                "tenant" => profile.tenant_id,
                "key-file" => profile.key_file,
                _ => anyhow::bail!("config get expects gateway|tenant|key-file"),
            };
            if let Some(v) = value {
                println!("{v}");
            }
        }
        _ => {
            emit_data(
                mode,
                &serde_json::json!({ "gateway_url": profile.gateway_url, "tenant_id": profile.tenant_id, "key_file": profile.key_file }),
            );
            if mode == OutputMode::Human {
                println!(
                    "gateway: {}",
                    profile.gateway_url.unwrap_or_else(|| "(unset)".into())
                );
                println!(
                    "tenant:  {}",
                    profile.tenant_id.unwrap_or_else(|| "(unset)".into())
                );
                println!(
                    "key-file:{}",
                    profile.key_file.unwrap_or_else(|| "(unset)".into())
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, routing::get, routing::post, Router};

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
    fn doctor_configuration_check_does_not_expose_or_require_secret_state() {
        let report = run_doctor_checks(&InstallerConfig::default());
        assert!(!report.secret_valid);
        assert!(doctor_configuration_ok(&report, None, None));
        assert!(!doctor_configuration_ok(&report, Some(false), None));
    }

    #[test]
    fn describe_rpc_labels_syntax_and_reachability() {
        assert_eq!(describe_rpc(false, None), "FAIL (invalid URL)");
        assert_eq!(describe_rpc(true, None), "PASS (valid URL)");
        assert_eq!(describe_rpc(true, Some(true)), "PASS (reachable)");
        assert_eq!(describe_rpc(true, Some(false)), "FAIL (unreachable)");
    }

    #[test]
    fn parse_flags_handles_bare_boolean_flags() {
        // A bare flag followed by another flag must not swallow it as a value.
        let args: Vec<String> = ["--json", "--probe"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let flags = parse_flags(&args);
        assert!(flags.contains_key("json"));
        assert!(flags.contains_key("probe"));
        assert_eq!(flags["probe"], "");

        // A flag with a value still captures it.
        let args: Vec<String> = ["--tier", "business", "--probe"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let flags = parse_flags(&args);
        assert_eq!(flags["tier"], "business");
        assert!(flags.contains_key("probe"));
    }

    #[tokio::test]
    async fn probe_rpc_reachable_detects_listening_and_closed_ports() {
        // A listening server (any HTTP status) is reachable.
        let app = Router::new().route("/", post(|| async { StatusCode::UNAUTHORIZED }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        assert!(probe_rpc_reachable(&format!("http://{addr}")).await);

        // A closed port is not reachable.
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_addr = closed.local_addr().unwrap();
        drop(closed);
        assert!(!probe_rpc_reachable(&format!("http://{closed_addr}")).await);

        // A non-HTTP scheme is rejected without a probe.
        assert!(!probe_rpc_reachable("ftp://example.invalid").await);
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
