use api::a2p::A2pRouter;
use api::fiat::FiatRouter;
use api::lightning::{
    LightningAdapter, LightningBackend, LightningBackendError, LightningSettlementRequest,
    LightningSettlementResponse,
};
use api::{configure_routes, new_lightning_adapter, new_settlement_log, AppState};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use compliance::zkc::{ATTESTATION_SIGNING_DOMAIN, TEE_DEVICE_ID_PREFIX};
use compliance::{CoreVerifier, IdentityManager, UniversalVerifier, ZkcVerifier};
use conxian_core::{
    Attestation, AttestationRequest, BitVmAttestation, ConxianJobCard, GatewayState,
    JobCardSettlementRequest, SharedState, WorkIntent,
};
use hmac::KeyInit;
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use secp256k1::{Message, Secp256k1, SecretKey};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::sleep;
use tower::ServiceExt;

const TEST_TOKEN: &str = "test-token";
const TEST_FIAT_SECRET: &str = "fake";
const TEST_SETTLEMENT_SECRET: &str = "simulated";
const TEST_X402_PROOF: &str = "proof-test-123";

type HmacSha256 = Hmac<Sha256>;

fn setup_app(state: SharedState) -> axum::Router {
    setup_app_with_lightning(state, new_lightning_adapter())
}

fn setup_app_with_lightning(state: SharedState, lightning: Arc<LightningAdapter>) -> axum::Router {
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
        "sentinel_infobip".to_string(),
        "test-infobip".to_string(),
        "test-hmac".to_string(),
    ));
    let identity = Arc::new(IdentityManager::new());
    let compliance = Arc::new(ZkcVerifier::new());
    let alex = Arc::new(engine::stacks::alex::SimulatedAlexClient);
    let mut multi_chain: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>> =
        std::collections::HashMap::new();
    multi_chain.insert(
        "liquid".to_string(),
        Arc::new(engine::LiquidAdapter::new(
            Arc::new(engine::BitcoinRpcClient::new("http://localhost:18843", "", "").unwrap()),
            "simulated".to_string(),
        )),
    );
    multi_chain.insert(
        "babylon".to_string(),
        Arc::new(engine::BabylonAdapter::new("simulated".to_string())),
    );
    multi_chain.insert(
        "bitvm".to_string(),
        Arc::new(engine::BitVmAdapter::new("simulated".to_string())),
    );

    let verifier = Arc::new(UniversalVerifier::new(
        compliance.clone() as Arc<dyn CoreVerifier>,
        multi_chain.clone(),
    ));

    struct SimulatedOfflineQueue {
        replay_claims: Mutex<HashSet<String>>,
    }

    impl conxian_core::OfflineQueue for SimulatedOfflineQueue {
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
    let offline_queue = Arc::new(SimulatedOfflineQueue {
        replay_claims: Mutex::new(HashSet::new()),
    });

    let app_state = AppState {
        shared: state,
        fiat,
        a2p,
        identity,
        compliance,
        verifier,
        alex,
        multi_chain,
        lightning,
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
    let pubkey = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);

    let mut hasher = Sha256::new();
    hasher.update(ATTESTATION_SIGNING_DOMAIN);
    hasher.update(payload_hash.as_bytes());
    hasher.update(device_id.as_bytes());
    let msg = Message::from_digest(hasher.finalize().into());

    let sig = secp.sign_ecdsa(&msg, &secret_key);

    let att = Attestation {
        device_id: device_id.to_string(),
        signature: hex::encode(sig.serialize_compact()),
        payload: payload_hash.to_string(),
        public_key: hex::encode(pubkey.serialize()),
    };

    serde_json::to_string(&AttestationRequest::Ecdsa(att)).unwrap()
}

fn sample_job_card_request() -> JobCardSettlementRequest {
    JobCardSettlementRequest {
        job_card: ConxianJobCard {
            context: "https://conxian.org/ns/job-card/v2".to_string(),
            r#type: "PaymentJob".to_string(),
            work_intent: WorkIntent {
                sender_address: "alice".to_string(),
                receiver_address: "bob".to_string(),
                amount_sbtc: 1000,
                town_name: Some("Cape Town".to_string()),
                country_code: Some("ZA".to_string()),
            },
        },
        bitvm_attestation: BitVmAttestation {
            prover_id: "prover-1".to_string(),
            commitment_hash: "0x123".to_string(),
            state_root: "0xabc".to_string(),
            proof_hash: "0xdef".to_string(),
            verifier_address: "ST123".to_string(),
        },
    }
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
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["status"], "ok");
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
                .header("x-402-payment", TEST_X402_PROOF)
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
                .header("x-402-payment", TEST_X402_PROOF)
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
        "provider": "ramp",
        "amount": 100.0,
        "currency": "USD",
        "wallet_address": "SP123"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/fiat/session")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
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

    let payload = json!({
        "provider": "ramp",
        "status": "completed",
        "tx_id": "tx-123"
    });

    let raw_payload = serde_json::to_string(&payload).unwrap();
    let mut mac = HmacSha256::new_from_slice(TEST_FIAT_SECRET.as_bytes()).unwrap();
    mac.update(raw_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let webhook_payload = json!({
        "provider": "ramp",
        "event_type": "ORDER_CREATED",
        "reference_id": "ref123",
        "amount": 100.0,
        "status": "SUCCESS",
        "signature": signature,
        "raw_payload": raw_payload
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/fiat/webhook")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&webhook_payload).unwrap()))
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

    let payload = json!({
        "provider": "ramp",
        "status": "completed",
        "tx_id": "tx-123"
    });

    let raw_payload = serde_json::to_string(&payload).unwrap();
    let mut mac = HmacSha256::new_from_slice(TEST_FIAT_SECRET.as_bytes()).unwrap();
    mac.update(raw_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let webhook_payload = json!({
        "provider": "ramp",
        "event_type": "ORDER_CREATED",
        "reference_id": "ref123",
        "amount": 100.0,
        "status": "SUCCESS",
        "signature": signature,
        "raw_payload": raw_payload
    });

    // First call
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/fiat/webhook")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&webhook_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Duplicate call
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/fiat/webhook")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&webhook_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
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
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_sync_erp_ledger_odata() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "value": [
            { "DocNum": "1001", "DocTotal": 50.0, "DocCur": "USD" }
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/erp/sync")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_settle_job_card_bitvm2() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = sample_job_card_request();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ingress_iso20022_authorized() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let xml_payload = r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.07">
    <FIToFICstmrCdtTrf>
        <Grphdr><MsgId>MSG-001</MsgId></Grphdr>
    </FIToFICstmrCdtTrf>
</Document>"#;

    let payload_hash = sha256_hex(xml_payload.as_bytes());
    let device_id = format!("{}test-device", TEE_DEVICE_ID_PREFIX);
    let attestation_header = make_attestation_header(&device_id, &payload_hash);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let trust_metadata = json!({
        "system": "IBC",
        "trust_tier": "T2",
        "policy": {
            "policy_id": "P-123",
            "policy_version": "1.0",
            "allowed_systems": []
        },
        "evidence": {
            "source": "TEE",
            "reference": "REF-456"
        },
        "freshness": {
            "observed_at_epoch_secs": now,
            "max_age_secs": 3600
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/iso20022")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("x-conxian-attestation", attestation_header)
                .header(
                    "x-conxian-trust-metadata",
                    serde_json::to_string(&trust_metadata).unwrap(),
                )
                .header("Content-Type", "application/xml")
                .body(Body::from(xml_payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ingress_iso20022_missing_trust_metadata_blocked() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/iso20022")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .body(Body::from("<xml></xml>"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_ingress_iso20022_denied_combo_blocked() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let trust_metadata = json!({
        "system": "HYPERLANE",
        "trust_tier": "T4",
        "policy": {
            "policy_id": "P-123",
            "policy_version": "1.0",
            "allowed_systems": []
        },
        "evidence": {
            "source": "TEE",
            "reference": "REF-456"
        },
        "freshness": {
            "observed_at_epoch_secs": now,
            "max_age_secs": 3600
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/ingress/iso20022")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header(
                    "x-conxian-trust-metadata",
                    serde_json::to_string(&trust_metadata).unwrap(),
                )
                .body(Body::from("<xml></xml>"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
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
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn test_x402_middleware_rejects_malformed_payload() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/state")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "{malformed}")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_x402_middleware_typed_payload() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let x402_payload = json!({
        "amount_satoshi": 1000,
        "asset": "sBTC",
        "challenge": "challenge-123",
        "expiry": 4_744_000_000u64,
        "proof_ref": "proof-ref-123"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/state")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header(
                    "x-402-payment",
                    serde_json::to_string(&x402_payload).unwrap(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

struct SingleOutcomeBackend {
    outcome: Result<LightningSettlementResponse, LightningBackendError>,
}

#[async_trait]
impl LightningBackend for SingleOutcomeBackend {
    async fn settle_payment(
        &self,
        _request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        self.outcome.clone()
    }
}

struct SlowSuccessBackend {
    delay: Duration,
}

#[async_trait]
impl LightningBackend for SlowSuccessBackend {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        sleep(self.delay).await;
        Ok(LightningSettlementResponse {
            settled_amount: request.amount,
            proof: "proof-123".to_string(),
            preimage: "preimage-123".to_string(),
        })
    }
}

#[tokio::test]
async fn test_x402_backend_unavailable_propagates_service_unavailable() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let adapter = Arc::new(
        LightningAdapter::new(Arc::new(SingleOutcomeBackend {
            outcome: Err(LightningBackendError::Unavailable),
        }))
        .with_retry_policy(0, Duration::from_millis(10)),
    );
    let app = setup_app_with_lightning(state, adapter);

    let x402_payload = json!({
        "amount_satoshi": 5000,
        "asset": "sBTC",
        "challenge": "challenge-unavailable",
        "expiry": 4_744_000_000u64,
        "proof_ref": "proof-unavailable"
    });

    let payload = sample_job_card_request();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header(
                    "x-402-payment",
                    serde_json::to_string(&x402_payload).unwrap(),
                )
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_x402_partial_failure_returns_bad_gateway() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let adapter = Arc::new(
        LightningAdapter::new(Arc::new(SingleOutcomeBackend {
            outcome: Err(LightningBackendError::PartialFailure {
                detail: "node committed but receipt write failed".to_string(),
            }),
        }))
        .with_retry_policy(0, Duration::from_millis(10)),
    );
    let app = setup_app_with_lightning(state, adapter);

    let x402_payload = json!({
        "amount_satoshi": 5000,
        "asset": "sBTC",
        "challenge": "challenge-partial-failure",
        "expiry": 4_744_000_000u64,
        "proof_ref": "proof-partial"
    });

    let payload = sample_job_card_request();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header(
                    "x-402-payment",
                    serde_json::to_string(&x402_payload).unwrap(),
                )
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn test_x402_backend_timeout_returns_gateway_timeout() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let adapter = Arc::new(
        LightningAdapter::new(Arc::new(SlowSuccessBackend {
            delay: Duration::from_millis(100),
        }))
        .with_retry_policy(0, Duration::from_millis(1)),
    );
    let app = setup_app_with_lightning(state, adapter);

    let x402_payload = json!({
        "amount_satoshi": 5000,
        "asset": "sBTC",
        "challenge": "challenge-timeout",
        "expiry": 4_744_000_000u64,
        "proof_ref": "proof-timeout"
    });

    let payload = sample_job_card_request();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settle")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header(
                    "x-402-payment",
                    serde_json::to_string(&x402_payload).unwrap(),
                )
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
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

#[tokio::test]
async fn test_admin_release_approval_request() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "release_id": "v1.9.2",
        "artifact_hash": "sha256:abc...",
        "environment": "production",
        "requester": "jules"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/v1/releases/request-approval")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["action_id"].as_str().unwrap().starts_with("req-"));
    assert_eq!(body["status"], "pending");
}

#[tokio::test]
async fn test_admin_release_decision_submission() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "release_id": "v1.9.2",
        "decision": "approved",
        "approver": "sab-admin",
        "reason": "Verified security gates"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/v1/releases/decision")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["action_id"].as_str().unwrap().starts_with("dec-"));
    assert_eq!(body["status"], "approved");
}

#[tokio::test]
async fn test_admin_governance_decision_submission() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "proposal_id": "gov-123",
        "decision": "approved",
        "voter": "dao-member-1",
        "signature": "0x..."
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/v1/governance/decision")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["action_id"].as_str().unwrap().starts_with("gov-"));
    assert_eq!(body["status"], "approved");
}

#[tokio::test]
async fn test_list_supported_chains() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/chains/list")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["supported_chains"].is_array());
}

#[tokio::test]
async fn test_get_liquid_chain_height() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/chains/liquid/height")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Since the RPC is fake/localhost, it might fail, but let's check it's routed
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn test_verify_state_proof_babylon() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "type": "finality_gadget",
        "evidence": "0x..."
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/chains/babylon/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment", "proof-test")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["chain"], "babylon");
    assert_eq!(body["verified"], true);
}

#[tokio::test]
async fn test_verify_state_proof_bitvm() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "root_hash": "0xabc..."
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/chains/bitvm/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment", "proof-test")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["chain"], "bitvm");
    assert_eq!(body["verified"], true);
}

#[tokio::test]
async fn test_resolve_identity_with_invalid_bip322_signature() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "identifier": "bc1q9v6v5p29n8lyat3tcmz2x7a9k9p0p2v6v5p29n",
        "provider": "web3bio",
        "signature": "dGVzdA=="
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/identity/resolve")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment", "proof-test")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_map_dlc_bond_to_usi_in_compliance() {
    let verifier = ZkcVerifier::new();
    let bond = conxian_core::DlcBond {
        bond_id: "dlc-123".to_string(),
        amount_btc: 1,
        interest_rate: 5.0,
        maturity_date: 1234567890,
        sovereign_alignment: true,
    };

    let usi = verifier.map_dlc_bond_to_usi(&bond);
    assert_eq!(usi.source, conxian_core::SettlementSource::DlcBond);
    assert_eq!(usi.transaction_id, "dlc-123");
    assert_eq!(usi.amount_minor, 100_000_000);
}

#[tokio::test]
async fn test_musig2_aggregation_in_compliance() {
    use conxian_core::musig2::MuSig2Orchestrator;
    let verifier = ZkcVerifier::new();
    let pubkeys = vec!["pk1-12345678".to_string(), "pk2-87654321".to_string()];

    let agg_key = verifier.aggregate_pubkeys(&pubkeys).unwrap();
    assert_eq!(agg_key.participant_pubkeys.len(), 2);

    let message_hash = [0u8; 32];
    let partial_sigs = vec![
        conxian_core::musig2::MuSig2PartialSignature {
            participant_pubkey: "pk1-12345678".to_string(),
            partial_signature: "sig1".to_string(),
            nonce: "nonce1".to_string(),
        },
        conxian_core::musig2::MuSig2PartialSignature {
            participant_pubkey: "pk2-87654321".to_string(),
            partial_signature: "sig2".to_string(),
            nonce: "nonce2".to_string(),
        },
    ];

    let final_sig = verifier
        .aggregate_signatures(&agg_key, &partial_sigs, &message_hash)
        .unwrap();
    assert!(final_sig.starts_with("final-sig-"));
}

#[tokio::test]
async fn test_verify_attestation_bitvm_rejection() {
    let state = Arc::new(std::sync::RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "type": "BitVm",
        "data": {"prover_id": "p1", "commitment_hash": "c1", "state_root": "r1", "proof_hash": "", "witness_hash": ""}
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment", "proof-test")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["status"], "action_required");
    assert_eq!(body["error"], "action_required");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("JobCard context"));
}
