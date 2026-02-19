use api::configure_routes;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conxian_core::{GatewayState, SharedState};
use serde_json::Value;
use std::sync::{Arc, RwLock};
use tower::ServiceExt; // for `oneshot` and `ready`

const TEST_TOKEN: &str = "test-token";

#[tokio::test]
async fn test_health_check() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = configure_routes(state, TEST_TOKEN.to_string());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_state_unauthorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = configure_routes(state, TEST_TOKEN.to_string());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_state_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    {
        let mut s = state.write().unwrap();
        s.bitcoin.height = 12345;
        s.bitcoin.status = "testing".to_string();
    }

    let app = configure_routes(state, TEST_TOKEN.to_string());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/state")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["bitcoin"]["height"], 12345);
    assert_eq!(json["bitcoin"]["status"], "testing");
}
