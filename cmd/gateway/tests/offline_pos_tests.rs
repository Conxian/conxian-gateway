use axum::http::StatusCode;
use axum_test::TestServer;
use conxian_api::a2p::A2pRouter;
use conxian_api::fiat::FiatRouter;
use conxian_api::{configure_routes, new_lightning_adapter, new_settlement_log, AppState};
use conxian_compliance::zkc::{ATTESTATION_SIGNING_DOMAIN, TEE_DEVICE_ID_PREFIX};
use conxian_compliance::{CoreVerifier, IdentityManager, UniversalVerifier, ZkcVerifier};
use conxian_core::{GatewayState, SharedState};
use conxian_engine::stacks::alex::SimulatedAlexClient;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_TOKEN: &str = "test-token";

fn setup_app(
    state: SharedState,
    offline_queue: Arc<dyn conxian_core::OfflineQueue>,
) -> axum::Router {
    let fiat = Arc::new(FiatRouter::new(
        "simulated".into(),
        "simulated".into(),
        "simulated".into(),
        "simulated".into(),
        "simulated".into(),
        "simulated".into(),
        "simulated".into(),
    ));
    let a2p = Arc::new(A2pRouter::new(
        "simulated".into(),
        "simulated".into(),
        "simulated".into(),
    ));
    let identity = Arc::new(IdentityManager::new());
    let compliance = Arc::new(ZkcVerifier::new());
    let alex = Arc::new(SimulatedAlexClient);
    let multi_chain: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>> =
        std::collections::HashMap::new();

    let verifier = Arc::new(UniversalVerifier::new(
        compliance.clone() as Arc<dyn CoreVerifier>,
        multi_chain.clone(),
    ));

    let app_state = AppState {
        coordinator: None,
        shared: state,
        persistence: None,
        bitcoin_core_shadow_observer: None,
        fiat,
        a2p,
        identity,
        compliance,
        verifier,
        alex,
        multi_chain,
        lightning: new_lightning_adapter(),
        fiat_webhook_secret: "secret".into(),
        settlement_ingress_secret: "secret".into(),
        settlement_log: new_settlement_log(),
        offline_queue,
    };

    configure_routes(
        app_state,
        TEST_TOKEN.to_string(),
        std::time::Instant::now(),
        None,
    )
}

#[tokio::test]
async fn test_offline_pos_blackout_reconciliation() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock moved backwards")
        .as_nanos();
    let db_path = format!("offline_queue_{}.db", ts);
    let key = [0u8; 32];
    let offline_queue =
        Arc::new(conxian_core::persistence::EncryptedOfflineQueue::new(&db_path, key).unwrap());

    let app = setup_app(state.clone(), offline_queue);
    let server = TestServer::new(app);

    let device_id = format!("{TEE_DEVICE_ID_PREFIX}simulated-device-1");
    let passkey_payload = "simulated-payload";

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    let public_key_hex = hex::encode(public_key.serialize());

    let mut hasher = Sha256::new();
    hasher.update(ATTESTATION_SIGNING_DOMAIN);
    hasher.update(passkey_payload.as_bytes());
    hasher.update(device_id.as_bytes());

    let digest = hasher.finalize();
    let message = Message::from_digest_slice(&digest).unwrap();
    let signature = secp.sign_ecdsa(&message, &secret_key);
    let signature_hex = hex::encode(signature.serialize_compact());

    let count = 10;
    for i in 0..count {
        let tx_hash = format!("tx-offline-{}", i);
        let payload = json!({
            "receipt_id": format!("rec-{}", i),
            "tx_hash": tx_hash,
            "amount_sbtc": 100000,
            "timestamp": 123456789,
            "device_id": &device_id,
            "tee_signature": "sim-sig",
            "passkey_attestation": {
                "type": "Ecdsa",
                "data": {
                    "device_id": &device_id,
                    "signature": signature_hex,
                    "payload": passkey_payload,
                    "public_key": public_key_hex
                }
            },
            "status": "PENDING"
        });

        server
            .post("/api/v1/pos/offline")
            .add_header("Authorization", format!("Bearer {}", TEST_TOKEN))
            .json(&payload)
            .await
            .assert_status(StatusCode::OK);
    }

    let sync_response = server
        .post("/api/v1/pos/sync")
        .add_header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .await;

    sync_response.assert_status(StatusCode::OK);
    let sync_result = sync_response.json::<serde_json::Value>();
    assert_eq!(sync_result["synced_count"].as_u64(), Some(count as u64));
    assert_eq!(sync_result["status"].as_str(), Some("success"));

    let _ = std::fs::remove_file(db_path);
}
