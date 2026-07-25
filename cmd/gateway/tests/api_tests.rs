use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conxian_api::a2p::A2pRouter;
use conxian_api::fiat::FiatRouter;
use conxian_api::lightning::{
    LightningAdapter, LightningBackend, LightningBackendError, LightningSettlementRequest,
    LightningSettlementResponse,
};
use conxian_api::{configure_routes, new_lightning_adapter, new_settlement_log, AppState};
use conxian_compliance::zkc::{ATTESTATION_SIGNING_DOMAIN, TEE_DEVICE_ID_PREFIX};
use conxian_compliance::{CoreVerifier, IdentityManager, UniversalVerifier, ZkcVerifier};
use conxian_core::{
    Attestation, AttestationRequest, BitVmAttestation, ConxianJobCard, FeeBumpStrategy,
    GatewayState, JobCardSettlementRequest, MempoolTxStatus, Persistence, PersistentState,
    SharedState, TrackedMempoolTx, VersionedPersistentState, WorkIntent,
};
use hmac::KeyInit;
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
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

struct StaticPersistence {
    state: PersistentState,
}

impl Persistence for StaticPersistence {
    fn load_versioned(&self) -> conxian_core::ConxianResult<VersionedPersistentState> {
        Ok(VersionedPersistentState {
            revision: 0,
            state: self.state.clone(),
        })
    }

    fn compare_and_swap(
        &self,
        expected_revision: u64,
        _new_state: &PersistentState,
    ) -> conxian_core::ConxianResult<VersionedPersistentState> {
        Err(conxian_core::ConxianError::PersistenceConflict {
            expected: expected_revision,
            actual: 0,
        })
    }
}

struct FailingPersistence;

impl Persistence for FailingPersistence {
    fn load_versioned(&self) -> conxian_core::ConxianResult<VersionedPersistentState> {
        Err(conxian_core::ConxianError::Internal(
            "backend-error-must-not-appear".to_string(),
        ))
    }

    fn compare_and_swap(
        &self,
        _expected_revision: u64,
        _new_state: &PersistentState,
    ) -> conxian_core::ConxianResult<VersionedPersistentState> {
        Err(conxian_core::ConxianError::Internal(
            "backend-error-must-not-appear".to_string(),
        ))
    }
}

fn setup_app(state: SharedState) -> axum::Router {
    setup_app_with_lightning_and_persistence(state, new_lightning_adapter(), None)
}

fn setup_app_with_lightning(state: SharedState, lightning: Arc<LightningAdapter>) -> axum::Router {
    setup_app_with_lightning_and_persistence(state, lightning, None)
}

fn setup_app_with_persistence(
    state: SharedState,
    persistence: Arc<dyn Persistence>,
) -> axum::Router {
    setup_app_with_lightning_and_persistence(state, new_lightning_adapter(), Some(persistence))
}

fn setup_app_with_lightning_and_persistence(
    state: SharedState,
    lightning: Arc<LightningAdapter>,
    persistence: Option<Arc<dyn Persistence>>,
) -> axum::Router {
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
    let alex = Arc::new(conxian_engine::stacks::alex::SimulatedAlexClient);
    let mut multi_chain: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>> =
        std::collections::HashMap::new();
    multi_chain.insert(
        "liquid".to_string(),
        Arc::new(conxian_engine::LiquidAdapter::new(
            Arc::new(
                conxian_engine::BitcoinRpcClient::new("http://localhost:18843", "", "").unwrap(),
            ),
            "simulated".to_string(),
        )),
    );
    multi_chain.insert(
        "babylon".to_string(),
        Arc::new(conxian_engine::BabylonAdapter::new("simulated".to_string())),
    );
    multi_chain.insert(
        "bitvm".to_string(),
        Arc::new(conxian_engine::BitVmAdapter::new("simulated".to_string())),
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
        coordinator: None,
        shared: state,
        persistence,
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

    configure_routes(
        app_state,
        TEST_TOKEN.to_string(),
        std::time::Instant::now(),
        None,
    )
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
    assert_eq!(body, serde_json::json!({ "status": "ok" }));
}

#[tokio::test]
async fn test_alex_quote_marks_simulated_source_explicitly() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/alex/quote?token_x=sBTC&token_y=STX&amount=1")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", TEST_X402_PROOF)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["quote"], "100");
    assert_eq!(body["source"], "fixture");
    assert_eq!(body["status"], "FIXTURE");
    assert_eq!(body["endpoint"], "simulated");
}

#[tokio::test]
async fn test_alex_swap_never_prepares_simulated_payload() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);
    let payload = json!({
        "token_x": "sBTC",
        "token_y": "STX",
        "factor": 100000000,
        "amount": 100,
        "min_dy": 90
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/alex/swap")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", TEST_X402_PROOF)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("simulated client cannot produce"));
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
async fn test_mempool_telemetry_requires_authentication() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/bitcoin/mempool/telemetry")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_mempool_telemetry_authorized_and_scoped() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let persistence = Arc::new(StaticPersistence {
        state: PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![TrackedMempoolTx {
                txid: "tracked-txid-must-not-appear".to_string(),
                first_seen_at: 10,
                last_evaluated_at: Some(100),
                last_bump_at: Some(120),
                bump_attempts: 2,
                current_fee_rate_sat_vb: 10,
                target_fee_rate_sat_vb: Some(14),
                replaceable: true,
                cpfp_eligible: true,
                status: MempoolTxStatus::BumpBroadcasted,
                last_bump_strategy: Some(FeeBumpStrategy::Rbf),
                last_error: Some("must-not-appear".to_string()),
                replacement_txid: Some("replacement-must-not-appear".to_string()),
                lease_owner: None,
                lease_id: None,
                lease_expires_at: None,
                record_generation: 0,
            }],
        },
    });
    let app = setup_app_with_persistence(state, persistence);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/bitcoin/mempool/telemetry")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", TEST_X402_PROOF)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body["schema_version"], 2);
    assert_eq!(body["scope"], "gateway_tracked_transactions");
    assert_eq!(body["network_mempool_observation"], "not_configured");
    assert_eq!(body["availability"], "available");
    assert_eq!(body["tracked_transaction_count"], 1);
    assert_eq!(body["status_counts"]["bump_broadcasted"], 1);
    assert_eq!(body["replaceable_tracked_total"], 1);
    assert_eq!(body["cpfp_capable_tracked_total"], 1);
    assert_eq!(body["bump_attempts_current_total"], 2);
    assert_eq!(body["last_bump_strategy_counts"]["rbf"], 1);
    assert_eq!(body["last_updated_at"], 120);
    assert!(!body.to_string().contains("tracked-txid-must-not-appear"));
    assert!(!body.to_string().contains("must-not-appear"));
}

#[tokio::test]
async fn test_mempool_telemetry_missing_persistence_returns_stable_503() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/bitcoin/mempool/telemetry")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", TEST_X402_PROOF)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["error"], "tracked_mempool_state_not_configured");
}

#[tokio::test]
async fn test_mempool_telemetry_failing_persistence_returns_stable_503() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app_with_persistence(state, Arc::new(FailingPersistence));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/bitcoin/mempool/telemetry")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", TEST_X402_PROOF)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["error"], "tracked_mempool_state_unavailable");
    assert!(!body.contains("backend-error-must-not-appear"));
}

#[tokio::test]
async fn test_prometheus_metrics_persisted_telemetry_is_bounded_and_scoped() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let persistence = Arc::new(StaticPersistence {
        state: PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![
                TrackedMempoolTx {
                    txid: "metrics-txid-must-not-appear".to_string(),
                    status: MempoolTxStatus::Pending,
                    last_bump_strategy: Some(FeeBumpStrategy::Rbf),
                    last_error: Some("metrics-error-must-not-appear".to_string()),
                    replacement_txid: Some("replacement-must-not-appear".to_string()),
                    ..TrackedMempoolTx::default()
                },
                TrackedMempoolTx {
                    txid: "metrics-txid-two-must-not-appear".to_string(),
                    status: MempoolTxStatus::Stuck,
                    last_bump_strategy: Some(FeeBumpStrategy::Cpfp),
                    ..TrackedMempoolTx::default()
                },
            ],
        },
    });
    let app = setup_app_with_persistence(state, persistence);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; version=0.0.4")
    );
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert!(body.contains("conxian_gateway_tracked_mempool_state_available 1"));
    assert!(body.contains("conxian_gateway_tracked_mempool_transactions 2"));
    assert!(
        body.contains("conxian_gateway_tracked_mempool_transactions_status{status=\"PENDING\"} 1")
    );
    assert!(
        body.contains("conxian_gateway_tracked_mempool_transactions_status{status=\"STUCK\"} 1")
    );
    assert!(body.contains(
        "conxian_gateway_tracked_mempool_last_bump_strategy_records{strategy=\"RBF\"} 1"
    ));
    assert!(body.contains(
        "conxian_gateway_tracked_mempool_last_bump_strategy_records{strategy=\"CPFP\"} 1"
    ));
    assert!(!body.contains("metrics-txid-must-not-appear"));
    assert!(!body.contains("metrics-txid-two-must-not-appear"));
    assert!(!body.contains("metrics-error-must-not-appear"));
    assert!(!body.contains("replacement-must-not-appear"));
}

#[tokio::test]
async fn test_prometheus_metrics_unavailable_omits_aggregate_samples() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app_with_persistence(state, Arc::new(FailingPersistence));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert!(body.contains("conxian_gateway_tracked_mempool_state_available 0"));
    assert!(!body
        .lines()
        .any(|line| line.starts_with("conxian_gateway_tracked_mempool_transactions ")));
    assert!(!body
        .lines()
        .any(|line| { line.starts_with("conxian_gateway_tracked_mempool_transactions_status{") }));
    assert!(!body.lines().any(|line| {
        line.starts_with("conxian_gateway_tracked_mempool_last_bump_strategy_records{")
    }));
    assert!(!body.contains("backend-error-must-not-appear"));
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
async fn test_settle_job_card_bitvm2_fails_closed_without_verifier() {
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

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["chain"], "bitvm");
    assert_eq!(body["status"], "unsupported");
    assert_eq!(body["code"], "verifier_unavailable");
    assert_eq!(body["authoritative"], false);
    assert!(body.get("txid").is_none());
    assert!(!body.to_string().contains("success"));
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
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
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
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
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
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
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
async fn test_verify_state_proof_bitvm_returns_typed_unavailable() {
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

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["chain"], "bitvm");
    assert_eq!(body["status"], "unsupported");
    assert_eq!(body["code"], "verifier_unavailable");
    assert_eq!(body["authoritative"], false);
    assert!(body.get("verified").is_none());
}

#[tokio::test]
async fn test_verify_state_proof_bitvm_rejects_legacy_payload_shapes() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payloads = [
        json!({"root_hash": "0xabc123"}),
        json!({
            "proof": {"a": "looks-like-a-proof", "b": "arbitrary"},
            "verified": true
        }),
        json!({}),
    ];

    for payload in payloads {
        let response = app
            .clone()
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

        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body["chain"], "bitvm");
        assert_eq!(body["status"], "unsupported");
        assert_eq!(body["code"], "verifier_unavailable");
        assert_eq!(body["authoritative"], false);
        assert!(!body["verified"].as_bool().unwrap_or(false));
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/chains/bitvm/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("Content-Type", "application/json")
                .header("x-402-payment", "proof-test")
                .body(Body::from("{malformed-json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&body_bytes);
    assert!(!body.contains("verified"));
}

#[tokio::test]
async fn test_verify_state_proof_liquid_rejects_arbitrary_metadata() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);
    let payload = json!({
        "verified": true,
        "claim": "accepted",
        "proof": "caller-supplied"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/chains/liquid/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", TEST_X402_PROOF)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["chain"], "liquid");
    assert_eq!(body["verified"], false);
}

#[tokio::test]
async fn test_verify_state_proof_liquid_rejects_empty_metadata() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/chains/liquid/verify")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", TEST_X402_PROOF)
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["chain"], "liquid");
    assert_eq!(body["verified"], false);
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

// ============================================================
// G-24: Fiat Webhook HMAC Verification Tests
// ============================================================

#[tokio::test]
async fn test_fiat_webhook_rejects_invalid_hmac_signature() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "provider": "investec",
        "status": "completed",
        "tx_id": "tx-invalid-sig"
    });

    let raw_payload = serde_json::to_string(&payload).unwrap();
    // Use wrong secret to create invalid HMAC
    let wrong_secret = b"wrong-secret-key-for-testing";
    let mut mac = HmacSha256::new_from_slice(wrong_secret).unwrap();
    mac.update(raw_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let webhook_payload = json!({
        "provider": "investec",
        "event_type": "ORDER_CREATED",
        "reference_id": "ref-bad-sig",
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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["error"], "invalid_signature");
}

#[tokio::test]
async fn test_fiat_webhook_rejects_missing_signature() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let payload = json!({
        "provider": "ramp",
        "status": "completed",
        "tx_id": "tx-no-sig"
    });

    let raw_payload = serde_json::to_string(&payload).unwrap();
    let webhook_payload = json!({
        "provider": "ramp",
        "event_type": "ORDER_CREATED",
        "reference_id": "ref-no-sig",
        "amount": 50.0,
        "status": "SUCCESS",
        "signature": "",
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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_fiat_webhook_rejects_tampered_payload() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    // Sign a legitimate payload
    let original_payload = json!({
        "provider": "ramp",
        "status": "completed",
        "tx_id": "tx-original"
    });
    let raw_original = serde_json::to_string(&original_payload).unwrap();
    let mut mac = HmacSha256::new_from_slice(TEST_FIAT_SECRET.as_bytes()).unwrap();
    mac.update(raw_original.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    // Tamper: send a different raw_payload but keep the original signature
    let tampered_payload = json!({
        "provider": "ramp",
        "status": "completed",
        "tx_id": "tx-tampered"
    });
    let raw_tampered = serde_json::to_string(&tampered_payload).unwrap();

    let webhook_payload = json!({
        "provider": "ramp",
        "event_type": "ORDER_CREATED",
        "reference_id": "ref-tampered",
        "amount": 100.0,
        "status": "SUCCESS",
        "signature": signature,
        "raw_payload": raw_tampered
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

    // Should reject because signature doesn't match the tampered payload
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ============================================================
// G-25: DLC Bond Creation Test
// ============================================================

#[tokio::test]
async fn test_create_dlc_bond() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let bond_payload = json!({
        "bond_id": "dlc-test-001",
        "amount_btc": 500000,
        "interest_rate": 0.05,
        "maturity_date": 1750000000u64,
        "sovereign_alignment": true
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/dlc/bond")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&bond_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["bond_id"].as_str().is_some());
}

#[tokio::test]
async fn test_create_dlc_bond_rejects_missing_bond_id() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let bond_payload = json!({
        "bond_id": "",
        "amount_btc": 500000,
        "interest_rate": 0.05,
        "maturity_date": 1750000000u64,
        "sovereign_alignment": true
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/dlc/bond")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&bond_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ============================================================
// G-26: MuSig2 Key Aggregation Test
// ============================================================

#[tokio::test]
async fn test_musig2_aggregate_keys() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let pubkeys = json!({
        "pubkeys": [
            "02aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            "03aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/musig2/aggregate-keys")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&pubkeys).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["aggregated_pubkey"].as_str().is_some());
}

// ============================================================
// G-18: Prometheus Metrics Endpoint Test
// ============================================================

#[tokio::test]
async fn test_prometheus_metrics_endpoint() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    // Pre-seed some metrics to verify they appear
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests = 42;
        s.metrics.health_requests = 10;
        s.metrics.verification_success = 5;
        s.metrics.treasury_balance_btc = 1_050_000_000;
        s.bitcoin.height = 850_000;
        s.stacks.height = 175_000;
    }
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    // Verify Prometheus text format
    assert!(body.contains("# HELP conxian_requests_total"));
    assert!(body.contains("# TYPE conxian_requests_total counter"));
    assert!(body.contains("conxian_requests_total 42"));
    assert!(body.contains("conxian_health_requests_total 10"));
    assert!(body.contains("conxian_verification_success_total 5"));
    assert!(body.contains("conxian_treasury_balance_btc 1050000000"));
    assert!(body.contains("conxian_bitcoin_height 850000"));
    assert!(body.contains("conxian_stacks_height 175000"));
    assert!(body.contains("conxian_syi_index"));
}

// ============================================================
// G-B2: Multi-currency FX Metrics Test
// ============================================================

#[tokio::test]
async fn test_prometheus_metrics_includes_fx_rates() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    {
        let mut s = state.write().unwrap();
        s.metrics.fx_rmb_usd = 0.14;
        s.metrics.fx_rub_usd = 0.011;
        s.metrics.fx_inr_usd = 0.012;
        s.metrics.fx_aed_usd = 0.272;
    }
    let app = setup_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert!(body.contains("conxian_fx_rmb_usd 0.14"));
    assert!(body.contains("conxian_fx_rub_usd 0.011"));
    assert!(body.contains("conxian_fx_inr_usd 0.012"));
    assert!(body.contains("conxian_fx_aed_usd 0.272"));
}

// ============================================================
// G-B5: PAPSS Ingress Test
// ============================================================

#[tokio::test]
async fn test_ingress_papss_success() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let papss_payload = json!({
        "PAPSS_MsgId": "PAPSS-AFRICA-001",
        "PAPSS_Amount": 250000,
        "PAPSS_Sender": "NGBK001",
        "PAPSS_Receiver": "GHBK002",
        "PAPSS_Currency": "NGN",
        "PAPSS_TxRef": "REF-PAPSS-001"
    });

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let trust_metadata = json!({
        "system": "IBC",
        "trust_tier": "T1",
        "policy": {
            "policy_id": "CON-791",
            "policy_version": "2026-06-01",
            "allowed_systems": []
        },
        "evidence": {
            "source": "unit-test"
        },
        "freshness": {
            "observed_at_epoch_secs": now,
            "max_age_secs": 3600
        }
    });

    let payload_bytes = serde_json::to_vec(&papss_payload).unwrap();
    let payload_hash = sha256_hex(&payload_bytes);
    let device_id = format!("{}_{}", TEE_DEVICE_ID_PREFIX, "papss-device");

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[0xef; 32]).unwrap();
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);

    let mut hasher = Sha256::new();
    hasher.update(ATTESTATION_SIGNING_DOMAIN);
    hasher.update(payload_hash.as_bytes());
    hasher.update(device_id.as_bytes());
    let msg = Message::from_digest(hasher.finalize().into());
    let sig = secp.sign_ecdsa(&msg, &secret_key);

    let attestation_request = conxian_core::AttestationRequest::Ecdsa(conxian_core::Attestation {
        device_id,
        signature: hex::encode(sig.serialize_compact()),
        payload: payload_hash.clone(),
        public_key: hex::encode(public_key.serialize()),
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settlement/papss")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header(
                    "x-conxian-attestation",
                    serde_json::to_string(&attestation_request).unwrap(),
                )
                .header(
                    "x-conxian-trust-metadata",
                    serde_json::to_string(&trust_metadata).unwrap(),
                )
                .header("Content-Type", "application/json")
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(
        body["envelope"]["payload"]["transaction_id"],
        "PAPSS-AFRICA-001"
    );
    assert_eq!(body["envelope"]["payload"]["source"], "PAPSS");
}

// ============================================================
// G-B4: Sanctions Blocking Test (SPFS)
// ============================================================

#[tokio::test]
async fn test_ingress_spfs_blocked_by_sanctions() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let spfs_payload = json!({
        "spfs_msg_id": "SPFS-BLOCK-001",
        "amount": 1000000,
        "currency": "RUB"
    });

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let trust_metadata = json!({
        "system": "IBC",
        "trust_tier": "T1",
        "policy": {
            "policy_id": "CON-791",
            "policy_version": "2026-06-01",
            "allowed_systems": []
        },
        "evidence": {
            "source": "unit-test"
        },
        "freshness": {
            "observed_at_epoch_secs": now,
            "max_age_secs": 3600
        }
    });

    let payload_bytes = serde_json::to_vec(&spfs_payload).unwrap();
    let payload_hash = sha256_hex(&payload_bytes);
    let device_id = format!("{}_{}", TEE_DEVICE_ID_PREFIX, "spfs-device");

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[0xaa; 32]).unwrap();
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);

    let mut hasher = Sha256::new();
    hasher.update(ATTESTATION_SIGNING_DOMAIN);
    hasher.update(payload_hash.as_bytes());
    hasher.update(device_id.as_bytes());
    let msg = Message::from_digest(hasher.finalize().into());
    let sig = secp.sign_ecdsa(&msg, &secret_key);

    let attestation_request = conxian_core::AttestationRequest::Ecdsa(conxian_core::Attestation {
        device_id,
        signature: hex::encode(sig.serialize_compact()),
        payload: payload_hash.clone(),
        public_key: hex::encode(public_key.serialize()),
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settlement/spfs")
                .method("POST")
                .header("Authorization", format!("Bearer {}", TEST_TOKEN))
                .header("x-402-payment", "proof-test")
                .header(
                    "x-conxian-attestation",
                    serde_json::to_string(&attestation_request).unwrap(),
                )
                .header(
                    "x-conxian-trust-metadata",
                    serde_json::to_string(&trust_metadata).unwrap(),
                )
                .header("Content-Type", "application/json")
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be FORBIDDEN due to Critical sanctions risk
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("Critical"));
}

#[tokio::test]
async fn test_admin_routes_unauthorized_rejection() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let app = setup_app(state);

    let endpoints = vec![
        ("/admin/v1/releases/request-approval", "POST"),
        ("/admin/v1/releases/decision", "POST"),
        ("/admin/v1/governance/decision", "POST"),
    ];

    for (uri, method) in endpoints {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .method(method)
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Endpoint {} should require authentication",
            uri
        );
    }
}
