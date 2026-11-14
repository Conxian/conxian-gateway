//! End-to-end tests for the CLI device lifecycle verbs.
//!
//! These drive the real compiled binary (`CARGO_BIN_EXE_conxian-cli`), the same
//! way `diagnostic_cli.rs` does. They cover the real sign → verify → plan path
//! with real BIP-340 crypto, a tampered manifest being refused, and an unknown
//! action kind being refused at parse time.

use std::process::Command;

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("conxian-device-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SK: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn manifest_json() -> String {
    r#"{
        "schemaVersion": 1,
        "device": "dev-1",
        "issuedAt": "2026-11-14T00:00:00Z",
        "actions": [
            { "kind": "migrateDatabase", "target": "control-plane" },
            { "kind": "restartService", "service": "gateway" }
        ]
    }"#
    .to_string()
}

fn sign(s: &Scratch, manifest: &std::path::Path, signed: &std::path::Path) {
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args([
            "device",
            "sign",
            "--manifest",
            manifest.to_str().unwrap(),
            "--key",
            SK,
            "--out",
            signed.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "sign failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = s;
}

fn pin_signer(signed: &std::path::Path, signer: &std::path::Path) {
    let envelope: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(signed).unwrap()).unwrap();
    std::fs::write(signer, envelope["signer"].as_str().unwrap()).unwrap();
}

/// `device sign` writes a signed manifest; `device verify` accepts it.
#[test]
fn sign_then_verify_roundtrip() {
    let s = Scratch::new();
    let manifest = s.path("desired-state.json");
    std::fs::write(&manifest, manifest_json()).unwrap();
    let signed = s.path("signed.json");
    let signer = s.path("device-signer.hex");

    sign(&s, &manifest, &signed);
    pin_signer(&signed, &signer);

    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["device", "verify", "--file", signed.to_str().unwrap()])
        .env("CONXIAN_DEVICE_SIGNER", &signer)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("VERIFIED"));
}

/// A manifest tampered after signing must be refused.
#[test]
fn tampered_manifest_is_refused() {
    let s = Scratch::new();
    let manifest = s.path("desired-state.json");
    std::fs::write(&manifest, manifest_json()).unwrap();
    let signed = s.path("signed.json");
    let signer = s.path("device-signer.hex");

    sign(&s, &manifest, &signed);
    let mut envelope: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&signed).unwrap()).unwrap();
    std::fs::write(&signer, envelope["signer"].as_str().unwrap()).unwrap();
    envelope["manifest"]["actions"][0]["target"] = serde_json::json!("attacker");
    std::fs::write(&signed, serde_json::to_string(&envelope).unwrap()).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["device", "verify", "--file", signed.to_str().unwrap()])
        .env("CONXIAN_DEVICE_SIGNER", &signer)
        .output()
        .unwrap();
    assert!(!out.status.success(), "tampered manifest must not verify");
    assert!(String::from_utf8_lossy(&out.stderr).contains("signature"));
}

/// An unknown action kind must be refused at parse time (fail-closed).
#[test]
fn unknown_action_kind_is_refused() {
    let s = Scratch::new();
    let manifest = s.path("desired-state.json");
    std::fs::write(
        &manifest,
        r#"{ "schemaVersion": 1, "device": "d", "issuedAt": "2026-11-14T00:00:00Z",
             "actions": [ { "kind": "runShell", "cmd": "rm -rf /" } ] }"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args([
            "device",
            "sign",
            "--manifest",
            manifest.to_str().unwrap(),
            "--key",
            SK,
            "--out",
            s.path("o.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "unknown action kind must be refused");
}

/// `device plan` on a converged device reports nothing to do.
#[test]
fn plan_reports_noop_when_converged() {
    let s = Scratch::new();
    let manifest = s.path("desired-state.json");
    std::fs::write(
        &manifest,
        r#"{ "schemaVersion": 1, "device": "d", "issuedAt": "2026-11-14T00:00:00Z", "actions": [] }"#,
    )
    .unwrap();
    let signed = s.path("signed.json");
    let signer = s.path("device-signer.hex");
    sign(&s, &manifest, &signed);
    pin_signer(&signed, &signer);

    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["device", "plan", "--file", signed.to_str().unwrap()])
        .env("CONXIAN_DEVICE_SIGNER", &signer)
        .env("XDG_CONFIG_HOME", &s.0)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "plan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("converged"));
}
