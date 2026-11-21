//! End-to-end test of the full pull loop: the compiled CLI binary against a
//! real HTTP server serving a real signed manifest, then reporting back.
//!
//! This is the Track B proof: the device pulls, verifies, plans, applies with
//! consent, and the server receives the report.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::{routing::get, routing::post, Router};

const SK: &str = "1111111111111111111111111111111111111111111111111111111111111111";

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("cxn-pull-{}-{id}", std::process::id()));
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

fn sign_manifest_via_cli(s: &Scratch) -> String {
    let manifest = s.path("desired-state.json");
    // An empty action list: the plan is a no-op, so pull reports "noop" without
    // needing docker or a database. That exercises fetch → verify → plan → report.
    std::fs::write(
        &manifest,
        r#"{ "schemaVersion": 1, "device": "dev-1", "issuedAt": "2026-11-14T00:00:00Z", "actions": [] }"#,
    )
    .unwrap();
    let signed = s.path("signed.json");
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
    assert!(out.status.success());
    std::fs::read_to_string(&signed).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn pull_verifies_plans_and_reports_against_a_real_server() {
    let s = Scratch::new();
    let signed_json = sign_manifest_via_cli(&s);
    let envelope: serde_json::Value = serde_json::from_str(&signed_json).unwrap();
    let signer_hex = envelope["signer"].as_str().unwrap().to_string();

    let reports = Arc::new(AtomicUsize::new(0));
    let r = reports.clone();
    let body = signed_json.clone();
    let app = Router::new()
        .route(
            "/api/v1/devices/{id}/desired-state",
            get(move || {
                let b = body.clone();
                async move { ([(axum::http::header::CONTENT_TYPE, "application/json")], b) }
            }),
        )
        .route(
            "/api/v1/devices/{id}/report",
            post(move || {
                let r = r.clone();
                async move {
                    r.fetch_add(1, Ordering::SeqCst);
                    axum::http::StatusCode::OK
                }
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Pin the signer and point the CLI at the server.
    let signer_file = s.path("device-signer.hex");
    std::fs::write(&signer_file, signer_hex).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["device", "pull", "--device", "dev-1", "--yes"])
        .env("GATEWAY_URL", format!("http://{addr}"))
        .env("CONXIAN_DEVICE_ID", "dev-1")
        .env("CONXIAN_ENROLLMENT_TOKEN", "test-token")
        .env("CONXIAN_DEVICE_SIGNER", &signer_file)
        .env("XDG_CONFIG_HOME", &s.0)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "pull failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("verified"), "stdout: {stdout}");
    assert_eq!(
        reports.load(Ordering::SeqCst),
        1,
        "the server must receive exactly one report"
    );

    server.abort();
}

/// A pull whose manifest fails verification must not apply or report success.
#[tokio::test(flavor = "multi_thread")]
async fn pull_refuses_an_unverifiable_manifest() {
    let s = Scratch::new();
    let mut envelope: serde_json::Value = serde_json::from_str(&sign_manifest_via_cli(&s)).unwrap();
    // Tamper the manifest after signing.
    envelope["manifest"]["device"] = serde_json::json!("tampered");
    let body = serde_json::to_string(&envelope).unwrap();

    let app = Router::new().route(
        "/api/v1/devices/{id}/desired-state",
        get(move || {
            let b = body.clone();
            async move { ([(axum::http::header::CONTENT_TYPE, "application/json")], b) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let signer_file = s.path("device-signer.hex");
    std::fs::write(&signer_file, envelope["signer"].as_str().unwrap()).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .args(["device", "pull", "--device", "dev-1", "--yes"])
        .env("GATEWAY_URL", format!("http://{addr}"))
        .env("CONXIAN_DEVICE_ID", "dev-1")
        .env("CONXIAN_ENROLLMENT_TOKEN", "test-token")
        .env("CONXIAN_DEVICE_SIGNER", &signer_file)
        .env("XDG_CONFIG_HOME", &s.0)
        .output()
        .unwrap();

    assert!(
        !out.status.success(),
        "an unverifiable manifest must fail the pull"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("signature"));

    server.abort();
}
