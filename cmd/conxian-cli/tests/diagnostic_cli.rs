use std::process::Command;

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("conxian-cli-{}-{id}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn init(&self) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
            .arg("init")
            .current_dir(&self.0)
            .env_clear()
            .output()
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn init_templates_match_startup_inputs_without_embedding_secrets_in_compose() {
    let dir = Scratch::new();
    assert!(dir.init().status.success());
    let env = std::fs::read_to_string(dir.0.join(".env.production")).unwrap();
    let compose = std::fs::read_to_string(dir.0.join("docker-compose.yml")).unwrap();
    for key in [
        "API_TOKEN",
        "FIAT_WEBHOOK_SECRET",
        "SETTLEMENT_INGRESS_SECRET",
        "HMAC_SECRET",
        "OFFLINE_QUEUE_SECRET",
    ] {
        assert!(env.contains(&format!("{key}=")));
        assert!(!compose.contains(key));
    }
    assert!(env.contains("API_PORT=3000"));
    assert!(env.contains("CONXIAN_NETWORK=mainnet"));
    assert!(!env.contains("GATEWAY_PORT="));
    assert!(!env.contains("DATABASE_URL="));
    assert!(env.contains("GATEWAY_STATE_PATH='/usr/app/gateway_state.json'"));
    assert!(env.contains("GATEWAY_PERSISTENCE_MODE='exclusive-local-writer'"));
    for mode in [
        "RAMP_MODE",
        "ALCHEMY_PAY_MODE",
        "INFOBIP_MODE",
        "STITCH_MODE",
        "OZOW_MODE",
        "BANXA_MODE",
        "PAPSS_MODE",
    ] {
        assert!(env.contains(&format!("{mode}='disabled'")));
    }
    assert!(compose.contains("${GATEWAY_IMAGE:?"));
    assert!(compose.contains("working_dir: /usr/app"));
    assert!(compose.contains("gateway_data:/usr/app"));
    assert!(compose.contains("127.0.0.1:${API_PORT:?Set API_PORT}:${API_PORT:?Set API_PORT}"));
    assert!(compose.contains("- .env.production"));
    assert!(String::from_utf8_lossy(&dir.init().stderr).contains("Refusing to replace"));
    assert_eq!(
        std::fs::read_to_string(dir.0.join(".env.production")).unwrap(),
        env
    );
    assert_eq!(
        std::fs::read_to_string(dir.0.join("docker-compose.yml")).unwrap(),
        compose
    );
}

#[test]
fn init_preserves_each_existing_file_without_creating_the_other() {
    for (existing, other) in [
        (".env.production", "docker-compose.yml"),
        ("docker-compose.yml", ".env.production"),
    ] {
        let dir = Scratch::new();
        std::fs::write(dir.0.join(existing), "private-existing-content").unwrap();
        let output = dir.init();
        assert!(!output.status.success());
        assert!(!dir.0.join(other).exists());
        assert_eq!(
            std::fs::read_to_string(dir.0.join(existing)).unwrap(),
            "private-existing-content"
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-existing-content"));
    }
}

#[cfg(unix)]
#[test]
fn init_protects_permissions_and_preserves_symlink_targets() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fresh = Scratch::new();
    assert!(fresh.init().status.success());
    for name in [".env.production", "docker-compose.yml"] {
        assert_eq!(
            std::fs::metadata(fresh.0.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
    for name in [".env.production", "docker-compose.yml"] {
        let dir = Scratch::new();
        let target = dir.0.join("target");
        std::fs::write(&target, "private-target").unwrap();
        symlink(&target, dir.0.join(name)).unwrap();
        assert!(!dir.init().status.success());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "private-target");
    }
    let dangling = Scratch::new();
    symlink(
        dangling.0.join("missing"),
        dangling.0.join(".env.production"),
    )
    .unwrap();
    assert!(!dangling.init().status.success());
    assert!(!dangling.0.join("missing").exists());
}

#[test]
fn default_doctor_reports_configuration_scope_and_exits_unsuccessfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .arg("doctor")
        .env_clear()
        .output()
        .unwrap();
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("connectivity not tested"));
    assert!(text.contains("Secret validation: not displayed"));
    assert!(!text.contains("Default installer API token configured"));
    assert!(!text.contains("FAIL (sentinel or empty)"));
    assert!(!text.contains("Ready for deployment"));
}

#[test]
fn invalid_status_base_fails_without_echoing_the_input() {
    let output = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .arg("status")
        .env_clear()
        .env("GATEWAY_URL", "file:///private-synthetic-input")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.contains("private-synthetic-input"));
}

#[test]
fn version_reports_the_cli_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .arg("version")
        .env_clear()
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("conxian-cli"));
}

#[test]
fn uninstall_removes_generated_templates_with_force() {
    let dir = Scratch::new();
    assert!(dir.init().status.success());
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["uninstall", "--force"])
        .current_dir(&dir.0)
        .env_clear()
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!dir.0.join(".env.production").exists());
    assert!(!dir.0.join("docker-compose.yml").exists());
}

#[test]
fn uninstall_preserves_files_it_did_not_generate() {
    let dir = Scratch::new();
    // A user-authored compose file must never be deleted.
    std::fs::write(
        dir.0.join("docker-compose.yml"),
        "services:\n  mine:\n    image: nginx\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["uninstall", "--force"])
        .current_dir(&dir.0)
        .env_clear()
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        dir.0.join("docker-compose.yml").exists(),
        "user file was deleted"
    );
}

#[test]
fn unknown_subcommand_exits_nonzero() {
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .arg("definitely-not-a-command")
        .env_clear()
        .output()
        .unwrap();
    assert!(!out.status.success(), "unknown subcommand must fail");
}

#[test]
fn subcommand_help_shows_command_specific_flags() {
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["doctor", "--help"])
        .env_clear()
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("--probe"), "doctor help must mention --probe");
    assert!(
        !text.contains("onboard"),
        "doctor help must not list other commands"
    );
}

/// A scratch dir holding a generated install, plus helpers to overwrite the env
/// file with concrete (non-placeholder) values.
struct Install(Scratch);

impl Install {
    fn new() -> Self {
        let dir = Scratch::new();
        assert!(dir.init().status.success());
        Self(dir)
    }

    /// Writes a complete, non-placeholder env file so preflight can pass. The
    /// image is digest-pinned; RPC URLs are syntactically valid.
    fn fill(&self) {
        let body = "\
GATEWAY_IMAGE=ghcr.io/conxian/gateway@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
API_TOKEN='a-real-strong-token'
FIAT_WEBHOOK_SECRET='fiat-value'
SETTLEMENT_INGRESS_SECRET='settlement-value'
HMAC_SECRET='hmac-value'
OFFLINE_QUEUE_SECRET='offline-value'
BITCOIN_RPC_URL='http://127.0.0.1:8332'
STACKS_RPC_URL='https://api.mainnet.hiro.so'
API_PORT=3000
CONXIAN_NETWORK=mainnet
";
        std::fs::write(self.0 .0.join(".env.production"), body).unwrap();
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
            .args(args)
            .current_dir(&self.0 .0)
            .env_clear()
            .output()
            .unwrap()
    }
}

#[test]
fn doctor_validates_the_install_file_and_passes_once_placeholders_are_replaced() {
    let install = Install::new();

    // The freshly generated template is all placeholders: doctor must fail and
    // name the problems by key, without printing any value.
    let template = install.run(&["doctor", "--json"]);
    assert!(!template.status.success());
    let out = String::from_utf8_lossy(&template.stdout);
    assert!(out.contains("\"checked_install\":true"));
    assert!(out.contains("API_TOKEN"));
    assert!(!out.contains("sentinel_"), "values must not leak");

    // After the client fills the file, the same install passes its own preflight.
    // This is the regression the check-only CLI could never satisfy: it validated
    // a hard-coded default token instead of the file it had just written.
    install.fill();
    let ready = install.run(&["doctor", "--json"]);
    assert!(
        ready.status.success(),
        "a filled install must pass preflight: {}",
        String::from_utf8_lossy(&ready.stdout)
    );
    let out = String::from_utf8_lossy(&ready.stdout);
    assert!(out.contains("\"ok\":true"));
    assert!(out.contains("\"install_problems\":[]"));
}

#[test]
fn doctor_reports_problems_by_key_without_echoing_values() {
    let install = Install::new();
    install.fill();
    // Replace one secret with a placeholder and one RPC with an invalid URL.
    let path = install.0 .0.join(".env.production");
    let body = std::fs::read_to_string(&path)
        .unwrap()
        .replace(
            "HMAC_SECRET='hmac-value'",
            "HMAC_SECRET='sentinel_HMAC_SECRET'",
        )
        .replace(
            "STACKS_RPC_URL='https://api.mainnet.hiro.so'",
            "STACKS_RPC_URL='not a url'",
        );
    std::fs::write(&path, body).unwrap();

    let out = install.run(&["doctor"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("HMAC_SECRET"),
        "must name the placeholder key"
    );
    assert!(text.contains("STACKS_RPC_URL"), "must name the bad URL key");
    assert!(
        !text.contains("sentinel_HMAC_SECRET") && !text.contains("not a url"),
        "must never echo values"
    );
}

#[test]
fn start_refuses_without_an_install_and_lists_missing_keys() {
    let dir = Scratch::new();
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .arg("start")
        .current_dir(&dir.0)
        .env_clear()
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("init"));
}

#[test]
fn start_refuses_a_template_with_placeholder_secrets_naming_keys_not_values() {
    let install = Install::new();
    let out = install.run(&["start", "--dry-run"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("API_TOKEN"));
    assert!(text.contains("GATEWAY_IMAGE"));
    assert!(
        !text.contains("CONXIAN_PROD_AUTH_TOKEN_REQUIRED") && !text.contains("sentinel_"),
        "must not echo the placeholder values"
    );
}

#[test]
fn start_dry_run_prints_the_exact_compose_command_without_launching() {
    let install = Install::new();
    install.fill();
    let out = install.run(&["start", "--dry-run", "--json"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("\"launched\":false"));
    assert!(text.contains("docker compose --env-file .env.production"));
    assert!(text.contains(" up -d"));
    // Dry-run must not have created any container.
    assert!(!text.contains("\"launched\":true"));
}

#[test]
fn start_requires_a_digest_pinned_image() {
    let install = Install::new();
    install.fill();
    let path = install.0 .0.join(".env.production");
    let body = std::fs::read_to_string(&path)
        .unwrap()
        .replace("ghcr.io/conxian/gateway@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "ghcr.io/conxian/gateway:latest");
    std::fs::write(&path, body).unwrap();
    let out = install.run(&["start", "--dry-run"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("digest"));
}

#[test]
fn logs_requires_a_compose_file_and_rejects_a_bad_tail_count() {
    // No compose file -> must refuse and point at init.
    let empty = Scratch::new();
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .arg("logs")
        .current_dir(&empty.0)
        .env_clear()
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("init"));

    // With a compose file, a non-numeric -n is rejected before docker runs.
    let install = Install::new();
    let bad = install.run(&["logs", "-n", "not-a-number"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("line count"));
}

#[test]
fn completion_emits_scripts_for_supported_shells_and_rejects_others() {
    for shell in ["bash", "zsh", "fish"] {
        let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
            .args(["completion", shell])
            .env_clear()
            .output()
            .unwrap();
        assert!(out.status.success(), "{shell} completion must succeed");
        assert!(String::from_utf8_lossy(&out.stdout).contains("conxian-cli"));
    }
    let bad = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["completion", "powershell"])
        .env_clear()
        .output()
        .unwrap();
    assert!(!bad.status.success());
}
