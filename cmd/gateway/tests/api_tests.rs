use api::a2p::A2pRouter;
use api::fiat::FiatRouter;
use api::{configure_routes, new_settlement_log, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
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
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, RwLock},
};
use tower::ServiceExt;

const TEST_TOKEN: &str = "test-token";
const TEST_FIAT_SECRET: &str = "test-fiat-secret";
const TEST_SETTLEMENT_SECRET: &str = "test-settlement-secret";
const TEST_X402_PROOF: &str = "proof-test-123";

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

    struct MockOfflineQueue {
        replay_claims: Mutex<HashSet<String>>,
    }

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

        fn claim_replay_key(
            &self,
            replay_key: &str,
            _ttl_seconds: u64,
        ) -> conxian_core::ConxianResult<bool> {
            let mut claims = self.replay_claims.lock().unwrap();
            Ok(claims.insert(replay_key.to_string()))
        }
    }
    let offline_queue = Arc::new(MockOfflineQueue {
        replay_claims: Mutex::new(HashSet::new()),
    });

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
    make_attestation_header(&format!("{}test-123", TEE_DEVICE_ID_PREFIX), payload_hash)
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
async fn test_fiat_session_authorized() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "wallet_address": "SP123...",
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

    let raw_payload_content = r#"{"reference":"ref123","status":"SUCCESS"}"#;

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
async fn test_fiat_webhook_duplicate_returns_conflict() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let raw_payload_content = r#"{"reference":"ref-dup-1","status":"SUCCESS"}"#;

    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_FIAT_SECRET.as_bytes()).unwrap();
    mac.update(raw_payload_content.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let payload = json!({
        "provider": "ramp",
        "event_type": "ORDER_CREATED",
        "reference_id": "ref-dup-1",
        "amount": 100.0,
        "status": "SUCCESS",
        "signature": signature,
        "raw_payload": raw_payload_content
    });

    let body = serde_json::to_string(&payload).unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/fiat/webhook")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-ramp-signature", &signature)
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(first.status(), StatusCode::OK);

    let duplicate = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/fiat/webhook")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-ramp-signature", &signature)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    let duplicate_body = duplicate.into_body().collect().await.unwrap().to_bytes();
    let duplicate_json: serde_json::Value = serde_json::from_slice(&duplicate_body).unwrap();
    assert_eq!(duplicate_json["code"], "WEBHOOK_REPLAY_DETECTED");
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

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/iso20022")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/xml")
                .header("x-iso20022-signature", signature)
                .header("x-tee-attestation", tee_attestation)
                .header("x-402-payment", TEST_X402_PROOF)
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

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/erp/sync")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment", TEST_X402_PROOF)
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
            amount_sbtc: 1000.5,
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

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment", TEST_X402_PROOF)
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
                // Missing x-402-payment header
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

    let x402_payload = json!({
        "amount_satoshi": 5000,
        "asset": "sBTC",
        "challenge": "challenge-xyz",
        "expiry": 1744000000u64,
        "proof_ref": "tx-abc-123"
    });
    let x402_header_val = serde_json::to_string(&x402_payload).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment", x402_header_val)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Pass x402 filter
    assert_ne!(response.status(), StatusCode::PAYMENT_REQUIRED);
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
