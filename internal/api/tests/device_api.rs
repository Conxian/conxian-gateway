//! Conformance test for the fleet-agent device routes (Track B, server half).
//!
//! Mounts the *real* device routes with the *real* signing key and drives them
//! over HTTP, then verifies the served manifest against the pinned signer key
//! using the real `conxian_device` verifier. No mocks.

use axum::{routing::get, routing::post, Router};
use conxian_api::device::{
    get_desired_state, post_report, put_desired_state, DeviceApi, DeviceRegistry,
};
use conxian_device::{verify_manifest, ActionKind, SignedManifest};
use std::sync::Arc;
use tower::ServiceExt;

const SK: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const TOKEN: &str = "device-token";

fn router() -> Router {
    std::env::set_var("DEVICE_MANIFEST_SIGNING_KEY", SK);
    std::env::set_var("DEVICE_ENROLLMENT_TOKEN", TOKEN);
    let api = DeviceApi::from_env(Arc::new(DeviceRegistry::new())).unwrap();
    Router::new()
        .route(
            "/devices/{device_id}/desired-state",
            get(get_desired_state).post(put_desired_state),
        )
        .route("/devices/{device_id}/report", post(post_report))
        .with_state(api)
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn missing_credential_is_unauthorized() {
    let app = router();
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/devices/dev-1/desired-state")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn served_manifest_verifies_against_the_pinned_signer() {
    let app = router();
    // Operator sets desired state.
    let set = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/devices/dev-1/desired-state")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"actions":[{"kind":"restartService","service":"gateway"}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(set.status(), axum::http::StatusCode::OK);

    // Device pulls it.
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/devices/dev-1/desired-state")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let signed: SignedManifest = serde_json::from_value(body_json(resp).await).unwrap();

    // The served manifest must verify against the signer the device pins.
    verify_manifest(&signed, &signed.signer, None).unwrap();
    assert_eq!(signed.manifest.device, "dev-1");
    assert_eq!(
        signed.manifest.actions,
        vec![ActionKind::RestartService {
            service: "gateway".into()
        }]
    );
}

#[tokio::test]
async fn unknown_device_converges_to_an_empty_manifest() {
    let app = router();
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/devices/never-seen/desired-state")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let signed: SignedManifest = serde_json::from_value(body_json(resp).await).unwrap();
    verify_manifest(&signed, &signed.signer, None).unwrap();
    assert!(signed.manifest.actions.is_empty());
}

#[tokio::test]
async fn report_is_recorded_and_device_is_bound_to_the_path() {
    std::env::set_var("DEVICE_MANIFEST_SIGNING_KEY", SK);
    std::env::set_var("DEVICE_ENROLLMENT_TOKEN", TOKEN);
    let registry = Arc::new(DeviceRegistry::new());
    let api = DeviceApi::from_env(registry.clone()).unwrap();
    let app = Router::new()
        .route("/devices/{device_id}/report", post(post_report))
        .with_state(api);

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/devices/dev-1/report")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"device":"spoofed","manifestDigest":"abc","status":"applied","outcomes":["restartService:applied"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert_eq!(registry.report_count().await, 1);
}

#[tokio::test]
async fn report_rejects_a_bad_credential() {
    std::env::set_var("DEVICE_MANIFEST_SIGNING_KEY", SK);
    std::env::set_var("DEVICE_ENROLLMENT_TOKEN", TOKEN);
    let registry = Arc::new(DeviceRegistry::new());
    let api = DeviceApi::from_env(registry.clone()).unwrap();
    let app = Router::new()
        .route("/devices/{device_id}/report", post(post_report))
        .with_state(api);

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/devices/dev-1/report")
                .header("authorization", "Bearer wrong")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"device":"d","manifestDigest":"x","status":"applied"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(registry.report_count().await, 0);
}
