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
    assert!(env.contains("API_PORT=8080"));
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
    assert!(text.contains("not a strength test"));
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
