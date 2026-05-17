use api::a2p::A2pRouter;
use api::fiat::FiatRouter;
use api::{configure_routes, new_settlement_log, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::prelude::*;
use compliance::zkc::{ATTESTATION_SIGNING_DOMAIN, TEE_DEVICE_ID_PREFIX};
use compliance::{IdentityManager, ZkcVerifier};
use conxian_core::{
    Attestation, AttestationRequest, BitVmAttestation, ConxianJobCard, GatewayState, SharedState,
    WorkIntent,
};
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use secp256k1::{Message, Secp256k1, SecretKey};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};
use tower::ServiceExt;

const TEST_TOKEN: &str = "test-token-that-is-at-least-32-characters-long-for-prod";
const TEST_FIAT_SECRET: &str = "test-fiat-secret";
const TEST_SETTLEMENT_SECRET: &str = "test-settlement-secret";

// Institutional x402 header generation for tests
fn make_x402_header() -> (String, String) {
    let payment_required = json!({
        "accepts": [
            {
                "amount": "1000",
                "asset": "sBTC",
                "maxTimeoutSeconds": 600
            }
        ],
        "challenge": "test-challenge"
    });

    let payment_signature = json!({
        "payload": {
            "authorization": {
                "nonce": "test-challenge",
                "validBefore": "2000000000"
            },
            "transaction": "0xdeadbeef"
        },
        "signature": "test-proof-signature"
    });

    (
        BASE64_STANDARD.encode(serde_json::to_vec(&payment_required).unwrap()),
        BASE64_STANDARD.encode(serde_json::to_vec(&payment_signature).unwrap()),
    )
}

fn setup_app(state: SharedState) -> axum::Router {
    let fiat = Arc::new(FiatRouter::new(
        "ramp-key".to_string(),
        "investec-id".to_string(),
        "investec-secret".to_string(),
        "alchemy-id".to_string(),
        "alchemy-secret".to_string(),
        "banxa-key".to_string(),
        "banxa-secret".to_string(),
    ));
    let a2p = Arc::new(A2pRouter::new(
        "test-infobip".to_string(),
        "test-infobip".to_string(),
        "test-hmac".to_string(),
    ));
    let identity = Arc::new(IdentityManager::new());
    let compliance = Arc::new(ZkcVerifier::new());
    let alex = Arc::new(engine::stacks::alex::SimulatedAlexClient);

    struct MockOfflineQueue;
    impl conxian_core::OfflineQueue for MockOfflineQueue {
        fn enqueue(&self, _r: &conxian_core::OfflineReceipt) -> conxian_core::ConxianResult<()> {
            Ok(())
        }
        fn dequeue_pending(
            &self,
        ) -> conxian_core::ConxianResult<Vec<conxian_core::OfflineReceipt>> {
            Ok(vec![])
        }
        fn mark_broadcasted(&self, _id: &str) -> conxian_core::ConxianResult<()> {
            Ok(())
        }
    }
    let offline_queue = Arc::new(MockOfflineQueue);

    let app_state = AppState {
        shared: state,
        fiat,
        a2p,
        identity,
        compliance,
        alex,
        fiat_webhook_secret: TEST_FIAT_SECRET.to_string(),
        settlement_ingress_secret: TEST_SETTLEMENT_SECRET.to_string(),
        settlement_log: new_settlement_log(),
        offline_queue,
    };

    configure_routes(app_state, TEST_TOKEN.to_string())
}

fn make_attestation_header(device_id: &str, payload_hash: &str) -> String {
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);

    let mut hasher = Sha256::new();
    hasher.update(ATTESTATION_SIGNING_DOMAIN);
    hasher.update(device_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(payload_hash.as_bytes());
    let digest = hasher.finalize();

    let message = Message::from_digest_slice(&digest).unwrap();
    let signature = secp.sign_ecdsa(&message, &secret_key);
    let signature_der = signature.serialize_der();

    let attestation = AttestationRequest::Ecdsa(Attestation {
        device_id: device_id.to_string(),
        signature: hex::encode(signature_der),
        payload: payload_hash.to_string(),
        public_key: hex::encode(public_key.serialize()),
    });

    serde_json::to_string(&attestation).unwrap()
}

fn make_tee_attestation_header(payload_hash: &str) -> String {
    make_attestation_header(
        &format!("{}test-simulated-device", TEE_DEVICE_ID_PREFIX),
        payload_hash,
    )
}

#[tokio::test]
async fn test_health_check() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
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
}

#[tokio::test]
async fn test_get_metrics_authorized() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/metrics")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_state_authorized() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
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
}

#[tokio::test]
async fn test_fiat_session_authorized() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "wallet_address": "bc1qtest",
        "amount": 100.0,
        "currency": "USD",
        "provider": "ramp"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/fiat/session")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_fiat_webhook_authorized() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let raw_payload_content = "{\"orderId\":\"123\"}";
    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_FIAT_SECRET.as_bytes()).unwrap();
    mac.update(raw_payload_content.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let payload = json!({
        "provider": "ramp",
        "event_type": "ORDER_CREATED",
        "reference_id": "ref123",
        "amount": 100.0,
        "status": "SUCCESS",
        "signature": signature,
        "raw_payload": raw_payload_content
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/fiat/webhook")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-ramp-signature", signature)
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_send_otp_authorized() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "phone_number": "+27123456789",
        "channel": "sms"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/a2p/otp")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        matches!(
            response.status(),
            StatusCode::OK | StatusCode::INTERNAL_SERVER_ERROR
        ),
        "unexpected status for OTP route: {}",
        response.status()
    );
}

#[tokio::test]
async fn test_ingress_iso20022_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    {
        let mut s = state.write().unwrap();
        s.stacks.burn_block_height = Some(55);
    }
    let app = setup_app(state);

    let xml_payload = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>TX-123</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="sBTC">0.5</IntrBkSttlmAmt><Dbtr><Nm>SENDER</Nm></Dbtr><Cdtr><Nm>RECEIVER</Nm></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;

    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_SETTLEMENT_SECRET.as_bytes()).unwrap();
    mac.update(xml_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let raw_payload_hash = hex::encode(Sha256::digest(xml_payload.as_bytes()));
    let tee_attestation = make_tee_attestation_header(&raw_payload_hash);
    let (x402_req, x402_sig) = make_x402_header();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/iso20022")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/xml")
                .header("x-iso20022-signature", signature)
                .header("x-tee-attestation", tee_attestation)
                .header("x-402-payment-required", x402_req)
                .header("x-402-payment-signature", x402_sig)
                .body(Body::from(xml_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_sync_erp_ledger_odata() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "value": [
            {
                "ID": "ERP-001",
                "Amount": "1000.50",
                "Currency": "USD",
                "Sender": "SAP_PROD",
                "Receiver": "CONXIAN_MAIN"
            }
        ]
    });
    let raw_payload = serde_json::to_string(&payload).unwrap();
    let (x402_req, x402_sig) = make_x402_header();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/erp/sync")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment-required", x402_req)
                .header("x-402-payment-signature", x402_sig)
                .body(Body::from(raw_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_settle_job_card_bitvm2() {
    use serde_json::Value;

    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let job_card = ConxianJobCard {
        context: "https://schema.conxian.io/jobcard/v2".to_string(),
        r#type: "ConxianJobCard".to_string(),
        work_intent: WorkIntent {
            sender_address: "SENDER".to_string(),
            receiver_address: "RECEIVER".to_string(),
            amount_satoshi: 100_050_000,
            town_name: None,
            country_code: None,
        },
    };

    let expected_job_hash = compliance::zkc::ZkcVerifier::compute_job_hash(&job_card).unwrap();
    let state_root = format!("job_hash={}", expected_job_hash);
    let commitment_hash = hex::encode(Sha256::digest(state_root.as_bytes()));

    let bitvm_attestation = BitVmAttestation {
        prover_id: "prover-1".to_string(),
        commitment_hash,
        state_root,
        proof_hash: "proof-1".to_string(),
        verifier_address: "verifier-1".to_string(),
    };

    let payload = json!({
        "job_card": job_card,
        "bitvm_attestation": bitvm_attestation
    });

    let (x402_req, x402_sig) = make_x402_header();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment-required", x402_req)
                .header("x-402-payment-signature", x402_sig)
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["verified"], true);
}

#[tokio::test]
async fn test_x402_middleware_rejection() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                // Missing x-402-payment headers
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn test_x402_middleware_typed_payload() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let (x402_req, x402_sig) = make_x402_header();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment-required", x402_req)
                .header("x-402-payment-signature", x402_sig)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Pass x402 filter
    assert_ne!(response.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn test_auth_middleware_weak_token_rejection() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let fiat = Arc::new(FiatRouter::new(
        "ramp".to_string(),
        "id".to_string(),
        "sec".to_string(),
        "apid".to_string(),
        "apsec".to_string(),
        "bk".to_string(),
        "bs".to_string(),
    ));
    let a2p = Arc::new(A2pRouter::new("a".into(), "b".into(), "c".into()));
    let identity = Arc::new(IdentityManager::new());
    let compliance = Arc::new(ZkcVerifier::new());
    let alex = Arc::new(engine::stacks::alex::SimulatedAlexClient);

    struct MockOfflineQueue;
    impl conxian_core::OfflineQueue for MockOfflineQueue {
        fn enqueue(&self, _r: &conxian_core::OfflineReceipt) -> conxian_core::ConxianResult<()> {
            Ok(())
        }
        fn dequeue_pending(
            &self,
        ) -> conxian_core::ConxianResult<Vec<conxian_core::OfflineReceipt>> {
            Ok(vec![])
        }
        fn mark_broadcasted(&self, _id: &str) -> conxian_core::ConxianResult<()> {
            Ok(())
        }
    }

    let app_state = AppState {
        shared: state,
        fiat,
        a2p,
        identity,
        compliance,
        alex,
        fiat_webhook_secret: "secret".to_string(),
        settlement_ingress_secret: "secret".to_string(),
        settlement_log: new_settlement_log(),
        offline_queue: Arc::new(MockOfflineQueue),
    };

    // Use a weak token
    let app = configure_routes(app_state, "too-short-token".to_string());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/metrics")
                .header("Authorization", "Bearer too-short-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 500 INTERNAL_SERVER_ERROR as per auth_middleware logic for insecure tokens
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_handoff_sequence() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    // 1. Get initial status
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/handoff/status")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["current_state"], "BootstrapActive");

    // 2. Update to AuditInProgress
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/handoff/update")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from("\"SabAuditInProgress\""))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 3. Verify destination change
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/handoff/status")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["current_state"], "SabAuditInProgress");
    // Should now point to treasury instead of bootstrap
    assert_ne!(body["treasury_destination"], body["bootstrap_wallet"]);
}
