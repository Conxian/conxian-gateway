use api::a2p::A2pRouter;
use api::fiat::FiatRouter;
use api::{configure_routes, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use compliance::{IdentityManager, ZkcVerifier};
use conxian_core::{GatewayState, SharedState};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::sync::{Arc, RwLock};
use tokio::sync::RwLock as TokioRwLock;
use tower::ServiceExt;

const TEST_TOKEN: &str = "test-token";
const TEST_FIAT_SECRET: &str = "test-fiat-secret";
const TEST_SETTLEMENT_SECRET: &str = "test-settlement-secret";

fn setup_app(state: SharedState) -> axum::Router {
    let app_state = AppState {
        shared: state,
        fiat: Arc::new(FiatRouter::new(
            "test-key".to_string(),
            "id".to_string(),
            "secret".to_string(),
            "ap-id".to_string(),
            "ap-secret".to_string(),
            "banxa-key".to_string(),
            "banxa-secret".to_string(),
        )),
        a2p: Arc::new(A2pRouter::new(
            "key".to_string(),
            "url".to_string(),
            "secret".to_string(),
        )),
        identity: Arc::new(IdentityManager::new()),
        compliance: Arc::new(ZkcVerifier::new()),
        fiat_webhook_secret: TEST_FIAT_SECRET.to_string(),
        settlement_ingress_secret: TEST_SETTLEMENT_SECRET.to_string(),
        settlement_log: Arc::new(TokioRwLock::new(std::collections::VecDeque::new())),
    };
    configure_routes(app_state, TEST_TOKEN.to_string())
}

#[tokio::test]
async fn test_health_check() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "healthy");
}

#[tokio::test]
async fn test_get_state_unauthorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

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

    let app = setup_app(state);

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

#[tokio::test]
async fn test_verify_attestation_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&serde_json::json!({
                    "type": "Ecdsa", "data": { "device_id": "conxius-123",
                    "signature": "30440220263f69528d22384a32c2a07c3f3e1a8e9b6a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0220263f69528d22384a32c2a07c3f3e1a8e9b6a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
                    "payload": "payload",
                    "public_key": "0250863ad64a87ad8a2bf2bb8ae16617bc25e101c70628d01f0599a4f7bb4d602f" }
                })).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_verify_schnorr_attestation_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&serde_json::json!({
                    "type": "Schnorr",
                    "data": {
                        "device_id": "conxius-schnorr-123",
                        "signature": "64646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464",
                        "payload": "payload",
                        "x_only_public_key": "3232323232323232323232323232323232323232323232323232323232323232"
                    }
                })).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    assert!(body_str.contains("gateway_total_requests"));
    assert!(body_str.contains("bitcoin_block_height"));
}

#[tokio::test]
async fn test_version_endpoint() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, conxian_core::VERSION.as_bytes());
}

#[tokio::test]
async fn test_erp_sync_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/erp/sync")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "d": {
                            "results": [
                                { "OrderID": 1, "Status": "Paid" },
                                { "OrderID": 2, "Status": "Pending" }
                            ]
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["synced_records"], 2);
}

#[tokio::test]
async fn test_settle_job_card_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "job_card": {
                            "@context": "https://conxian.com/contexts/job-card/v2.0",
                            "@type": "ConxianJobCard",
                            "work_intent": {
                                "sender_address": "ST123",
                                "receiver_address": "ST456",
                                "amount_sbtc": 0.1,
                                "town_name": "Joburg",
                                "country_code": "ZA"
                            }
                        },
                        "bitvm_proof": {
                            "prover_id": "prover-1",
                            "commitment_hash": "MOCK_COMMITMENT",
                            "state_root": "PROTOTYPE_ROOT"
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_iso_payment_v8_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/iso20022/payment")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "@context": "https://conxian.com/contexts/job-card/v2.0",
                        "@type": "ConxianJobCard",
                        "work_intent": {
                            "sender_address": "ST12345678",
                            "receiver_address": "ST87654321",
                            "amount_sbtc": 0.05,
                            "town_name": "Johannesburg",
                            "country_code": "ZA"
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["schema"], "pacs.008.001.08");
    assert!(json["xml"].as_str().unwrap().contains("pacs.008.001.08"));
}

#[tokio::test]
async fn test_ingress_iso20022_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let xml_payload = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>TX-123</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="sBTC">0.5</IntrBkSttlmAmt><DbtrAcct><Id><Othr><Id>SENDER-AC-1</Id></Othr></Id></DbtrAcct><CdtrAcct><Id><Othr><Id>RECEIVER-AC-1</Id></Othr></Id></CdtrAcct></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;

    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_SETTLEMENT_SECRET.as_bytes()).unwrap();
    mac.update(xml_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/iso20022")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/xml")
                .header("x-iso20022-signature", signature)
                .body(Body::from(xml_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["payload"]["transaction_id"], "TX-123");
    assert_eq!(json["payload"]["amount_minor"], 5);
    assert_eq!(json["payload"]["amount_scale"], 1);
    assert_eq!(json["payload"]["sender"], "SENDER-AC-1");
    assert_eq!(json["payload"]["receiver"], "RECEIVER-AC-1");
}

#[tokio::test]
async fn test_ingress_papss_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let inner_payload = serde_json::json!({
        "transaction_id": "PAPSS-456",
        "amount": "1000.00",
        "sender_bic": "BANK-ZA-1",
        "receiver_bic": "BANK-NG-1",
        "currency": "USD"
    });

    let raw_payload = serde_json::to_string(&inner_payload).unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_SETTLEMENT_SECRET.as_bytes()).unwrap();
    mac.update(raw_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/papss")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-papss-signature", signature)
                .body(Body::from(raw_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["payload"]["transaction_id"], "PAPSS-456");
    assert_eq!(json["payload"]["amount_minor"], 100000);
    assert_eq!(json["payload"]["amount_scale"], 2);
}

#[tokio::test]
async fn test_ingress_brics_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let inner_payload = serde_json::json!({
        "brics_tx_id": "BRICS-789",
        "amount": "50",
        "origin_bank": "RUB-BANK",
        "target_bank": "CNY-BANK",
        "currency": "GOLD"
    });

    let raw_payload = serde_json::to_string(&inner_payload).unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_SETTLEMENT_SECRET.as_bytes()).unwrap();
    mac.update(raw_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/brics")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-brics-signature", signature)
                .body(Body::from(raw_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["payload"]["transaction_id"], "BRICS-789");
    assert_eq!(json["payload"]["currency"], "GOLD");
}
