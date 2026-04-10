use api::{configure_routes, new_settlement_log, AppState};
use axum::http::StatusCode;
use axum_test::TestServer;
use compliance::{IdentityManager, ZkcVerifier};
use conxian_core::{GatewayState, SharedState};
use engine::stacks::alex::SimulatedAlexClient;
use serde_json::json;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn test_offline_pos_blackout_reconciliation() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let fiat_router = Arc::new(api::fiat::FiatRouter::new(
        "mock".into(),
        "mock".into(),
        "mock".into(),
        "mock".into(),
        "mock".into(),
        "mock".into(),
        "mock".into(),
    ));
    let a2p_router = Arc::new(api::a2p::A2pRouter::new(
        "mock".into(),
        "mock".into(),
        "mock".into(),
    ));
    let identity_manager = Arc::new(IdentityManager::new());
    let zkc_verifier = Arc::new(ZkcVerifier::new());
    let alex_client = Arc::new(SimulatedAlexClient);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = format!("offline_queue_{}.db", ts);
    let key = [0u8; 32];
    let offline_queue =
        Arc::new(conxian_core::persistence::EncryptedOfflineQueue::new(&db_path, key).unwrap());

    let app_state = AppState {
        shared: state.clone(),
        fiat: fiat_router,
        a2p: a2p_router,
        identity: identity_manager,
        compliance: zkc_verifier,
        alex: alex_client,
        fiat_webhook_secret: "secret".into(),
        settlement_ingress_secret: "secret".into(),
        settlement_log: new_settlement_log(),
        offline_queue,
    };

    let api_token = "test-token";
    let app = configure_routes(app_state, api_token.to_string());
    let server = TestServer::new(app).unwrap();

    let count = 10;
    for i in 0..count {
        let tx_hash = format!("tx-offline-{}", i);
        let payload = json!({
            "tx_hash": tx_hash,
            "amount_sbtc": 0.001,
            "device_id": "conxius-mock-device-1",
            "passkey_attestation": {
                "type": "Ecdsa",
                "data": {
                    "device_id": "conxius-mock-device-1",
                    "signature": "mock-sig",
                    "payload": "mock-payload",
                    "public_key": "mock-key"
                }
            }
        });

        server
            .post("/api/v1/pos/offline")
            .add_header("Authorization", format!("Bearer {}", api_token))
            .json(&payload)
            .await
            .assert_status(StatusCode::OK);
    }

    let sync_response = server
        .post("/api/v1/pos/sync")
        .add_header("Authorization", format!("Bearer {}", api_token))
        .await;

    sync_response.assert_status(StatusCode::OK);
    let sync_result = sync_response.json::<serde_json::Value>();
    assert_eq!(sync_result["synced_count"].as_u64(), Some(count as u64));
    assert_eq!(sync_result["status"].as_str(), Some("success"));

    let _ = std::fs::remove_file(db_path);
}
