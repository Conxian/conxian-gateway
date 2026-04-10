use api::a2p::A2pRouter;
use api::fiat::FiatRouter;
use api::{configure_routes, new_settlement_log, AppState};
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use compliance::zkc::ATTESTATION_SIGNING_DOMAIN;
use compliance::{IdentityManager, ZkcVerifier};
use conxian_core::{Attestation, AttestationRequest, GatewayState, SharedState};
use hmac::{Hmac, Mac};
use secp256k1::{Message, Secp256k1, SecretKey};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};
use tower::ServiceExt;

const TEST_TOKEN: &str = "test-token";
const TEST_FIAT_SECRET: &str = "test-fiat-secret";
const TEST_SETTLEMENT_SECRET: &str = "test-settlement-secret";

fn make_tee_attestation_header(raw_payload_hash: &str) -> String {
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);

    let device_id = "conxius-tee-test";

    let mut hasher = Sha256::new();
    hasher.update(ATTESTATION_SIGNING_DOMAIN);
    hasher.update(device_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(raw_payload_hash.as_bytes());
    let digest = hasher.finalize();

    let message = Message::from_digest_slice(&digest).unwrap();
    let signature = secp.sign_ecdsa(&message, &secret_key);
    let signature_der = signature.serialize_der();

    let attestation = Attestation {
        device_id: device_id.to_string(),
        signature: hex::encode(signature_der),
        payload: raw_payload_hash.to_string(),
        public_key: hex::encode(public_key.serialize()),
    };

    let request = AttestationRequest::Ecdsa(attestation);
    serde_json::to_string(&request).unwrap()
}

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
        alex: Arc::new(engine::stacks::alex::SimulatedAlexClient),
        fiat_webhook_secret: TEST_FIAT_SECRET.to_string(),
        settlement_ingress_secret: TEST_SETTLEMENT_SECRET.to_string(),
        settlement_log: new_settlement_log(),
        offline_queue: api::new_offline_queue(),
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
}

#[tokio::test]
async fn test_auth_middleware_rejection() {
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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_middleware_acceptance() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/metrics")
                .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ingress_iso20022_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    {
        let mut s = state.write().unwrap();
        s.stacks.burn_block_height = Some(55);
    }
    let app = setup_app(state);

    let xml_payload = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>TX-123</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="sBTC">0.5</IntrBkSttlmAmt><DbtrAcct><Id><Othr><Id>SENDER-AC-1</Id></Othr></Id></DbtrAcct><CdtrAcct><Id><Othr><Id>RECEIVER-AC-1</Id></Othr></Id></CdtrAcct></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;

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
                .body(Body::from(xml_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ingress_iso20022_rejects_tampered_tee_device_id() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    {
        let mut s = state.write().unwrap();
        s.stacks.burn_block_height = Some(55);
    }
    let app = setup_app(state);

    let xml_payload = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>TX-123</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="sBTC">0.5</IntrBkSttlmAmt><DbtrAcct><Id><Othr><Id>SENDER-AC-1</Id></Othr></Id></DbtrAcct><CdtrAcct><Id><Othr><Id>RECEIVER-AC-1</Id></Othr></Id></CdtrAcct></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;

    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_SETTLEMENT_SECRET.as_bytes()).unwrap();
    mac.update(xml_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let raw_payload_hash = hex::encode(Sha256::digest(xml_payload.as_bytes()));
    let tee_attestation = make_tee_attestation_header(&raw_payload_hash);

    let mut attestation_req: AttestationRequest = serde_json::from_str(&tee_attestation).unwrap();
    match &mut attestation_req {
        AttestationRequest::Ecdsa(a) => a.device_id = "conxius-tee-tampered".to_string(),
        _ => panic!("expected Ecdsa attestation in test"),
    }
    let tee_attestation = serde_json::to_string(&attestation_req).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/iso20022")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/xml")
                .header("x-iso20022-signature", signature)
                .header("x-tee-attestation", tee_attestation)
                .body(Body::from(xml_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_ingress_papss_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let json_payload = json!({
        "transaction_id": "papss-456",
        "amount": "12.34",
        "currency": "USD",
        "sender_bic": "SEND123",
        "receiver_bic": "RECV456"
    });
    let raw_payload = serde_json::to_string(&json_payload).unwrap();

    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_SETTLEMENT_SECRET.as_bytes()).unwrap();
    mac.update(raw_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let raw_payload_hash = hex::encode(Sha256::digest(raw_payload.as_bytes()));
    let tee_attestation = make_tee_attestation_header(&raw_payload_hash);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/papss")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-papss-signature", signature)
                .header("x-tee-attestation", tee_attestation)
                .body(Body::from(raw_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ingress_brics_authorized() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let json_payload = json!({
        "brics_tx_id": "brics-789",
        "amount": "100.00",
        "currency": "XAU",
        "origin_bank": "GOLD1",
        "target_bank": "GOLD2"
    });
    let raw_payload = serde_json::to_string(&json_payload).unwrap();

    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_SETTLEMENT_SECRET.as_bytes()).unwrap();
    mac.update(raw_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let raw_payload_hash = hex::encode(Sha256::digest(raw_payload.as_bytes()));
    let tee_attestation = make_tee_attestation_header(&raw_payload_hash);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/brics")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-brics-signature", signature)
                .header("x-tee-attestation", tee_attestation)
                .body(Body::from(raw_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
