use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conxian_api::lightning::{
    LightningAdapter, LightningBackend, LightningBackendError, LightningSettlementRequest,
    LightningSettlementResponse,
};
use conxian_api::{configure_routes, AppState};
use conxian_compliance::{IdentityManager, ZkcVerifier};
use conxian_engine::{
    MBridgeAdapter, MBridgeAttestationPayload,
};
use http_body_util::BodyExt;
use secp256k1::{Keypair, Message, Secp256k1};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

const TEST_TOKEN: &str = "test_bearer_token";

fn make_attestation_header(device_id: &str, payload_hash: &str) -> String {
    use secp256k1::SecretKey;

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let pubkey = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);

    let mut hasher = Sha256::new();
    hasher.update(conxian_compliance::zkc::ATTESTATION_SIGNING_DOMAIN);
    hasher.update(payload_hash.as_bytes());
    hasher.update(device_id.as_bytes());
    let msg = Message::from_digest(hasher.finalize().into());

    let sig = secp.sign_ecdsa(&msg, &secret_key);

    let att = conxian_core::Attestation {
        device_id: device_id.to_string(),
        signature: hex::encode(sig.serialize_compact()),
        payload: payload_hash.to_string(),
        public_key: hex::encode(pubkey.serialize()),
    };

    serde_json::to_string(&conxian_core::AttestationRequest::Ecdsa(att)).unwrap()
}

fn trust_metadata_json() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    json!({
        "system": "IBC",
        "trust_tier": "T1",
        "policy": {
            "policy_id": "CON-791",
            "policy_version": "2026-06-01",
            "allowed_systems": []
        },
        "evidence": {
            "source": "simulated",
            "reference": "ref-1"
        },
        "freshness": {
            "observed_at_epoch_secs": now,
            "max_age_secs": 3600
        }
    })
    .to_string()
}

struct DummyBackend;

#[async_trait]
impl LightningBackend for DummyBackend {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        Ok(LightningSettlementResponse {
            settled_amount: request.amount,
            preimage: "preimage-123".to_string(),
            proof: "proof-123".to_string(),
        })
    }
}

struct SimulatedOfflineQueue {
    replay_claims: Mutex<HashSet<String>>,
}

impl conxian_core::OfflineQueue for SimulatedOfflineQueue {
    fn enqueue(&self, receipt: &conxian_core::OfflineReceipt) -> conxian_core::ConxianResult<()> {
        let mut claims = self.replay_claims.lock().unwrap();
        if claims.contains(&receipt.receipt_id) {
            return Err(conxian_core::ConxianError::Compliance(
                "Replay claim error".to_string(),
            ));
        }
        claims.insert(receipt.receipt_id.clone());
        Ok(())
    }

    fn dequeue_pending(&self) -> conxian_core::ConxianResult<Vec<conxian_core::OfflineReceipt>> {
        Ok(vec![])
    }

    fn mark_broadcasted(&self, _receipt_id: &str) -> conxian_core::ConxianResult<()> {
        Ok(())
    }

    fn claim_replay_key(
        &self,
        _replay_key: &str,
        _ttl_seconds: u64,
    ) -> conxian_core::ConxianResult<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn test_mbridge_dlt_attestation_verification_engine() {
    let secp = Secp256k1::new();
    let mut rng = secp256k1::rand::thread_rng();

    let keypair1 = Keypair::new(&secp, &mut rng);
    let (pubkey1, _) = keypair1.x_only_public_key();

    let keypair2 = Keypair::new(&secp, &mut rng);
    let (pubkey2, _) = keypair2.x_only_public_key();

    let mbridge_id = "mb-tx-2026-cbt";
    let from_cbdc = "e-CNY";
    let to_cbdc = "e-AED";
    let amount = 5000000;
    let currency = "AED";
    let sender_bic = "BKCHCNBJXXX";
    let receiver_bic = "FADBAEADXXX";
    let timestamp = 1750000000;

    let payload_hash = MBridgeAdapter::compute_payload_hash(
        mbridge_id,
        from_cbdc,
        to_cbdc,
        amount,
        currency,
        sender_bic,
        receiver_bic,
        timestamp,
    );

    let msg_hash = Sha256::digest(payload_hash.as_bytes());
    let message = Message::from_digest(msg_hash.into());

    let sig1 = secp.sign_schnorr(&message, &keypair1);
    let sig2 = secp.sign_schnorr(&message, &keypair2);

    let payload = MBridgeAttestationPayload {
        mbridge_id: mbridge_id.into(),
        from_cbdc: from_cbdc.into(),
        to_cbdc: to_cbdc.into(),
        amount,
        currency: currency.into(),
        sender_bic: sender_bic.into(),
        receiver_bic: receiver_bic.into(),
        proof_hash: payload_hash,
        timestamp,
        validator_attestations: vec![
            (hex::encode(pubkey1.serialize()), hex::encode(sig1.as_ref())),
            (hex::encode(pubkey2.serialize()), hex::encode(sig2.as_ref())),
        ],
        quorum_threshold: 2,
    };

    let result = MBridgeAdapter::verify_mbridge_dlt_attestation(&payload).expect("Attestation failed");
    assert!(result.is_valid);
    assert_eq!(result.verified_validators, 2);
    assert_eq!(result.quorum_threshold, 2);
}

#[tokio::test]
async fn test_ingress_mbridge_api_route() {
    let compliance = Arc::new(ZkcVerifier::new());
    let identity = Arc::new(IdentityManager::new());
    let state_shared: conxian_core::SharedState =
        Arc::new(RwLock::new(conxian_core::GatewayState::default()));
    let fiat = Arc::new(conxian_api::fiat::FiatRouter::new(
        "ramp-key".to_string(),
        "investec-id".to_string(),
        "investec-secret".to_string(),
        "alchemy-id".to_string(),
        "alchemy-secret".to_string(),
        "banxa-key".to_string(),
        "banxa-secret".to_string(),
    ));
    let a2p = Arc::new(conxian_api::a2p::A2pRouter::new(
        "sentinel_infobip".to_string(),
        "test-infobip".to_string(),
        "test-hmac".to_string(),
    ));
    let alex = Arc::new(conxian_engine::stacks::alex::SimulatedAlexClient);
    let multi_chain = std::collections::HashMap::new();
    let offline_queue = Arc::new(SimulatedOfflineQueue {
        replay_claims: Mutex::new(HashSet::new()),
    });
    let adapter = LightningAdapter::new(Arc::new(DummyBackend));

    let state = AppState {
        coordinator: None,
        shared: state_shared,
        persistence: None,
        bitcoin_core_shadow_observer: None,
        fiat,
        a2p,
        identity,
        compliance: compliance.clone(),
        verifier: Arc::new(conxian_compliance::UniversalVerifier::new(
            compliance as Arc<dyn conxian_compliance::CoreVerifier>,
            multi_chain.clone(),
        )),
        alex_preparer: Arc::new(
            conxian_engine::stacks::alex::AlexPreparationService::disabled(alex.clone()),
        ),
        alex,
        multi_chain,
        lightning: Arc::new(adapter),
        fiat_webhook_secret: "fake".to_string(),
        settlement_ingress_secret: "simulated".to_string(),
        settlement_log: conxian_api::new_settlement_log(),
        offline_queue,
    };

    let app = configure_routes(state, TEST_TOKEN.to_string(), Instant::now(), None);

    let payload = json!({
        "mbridge_id": "MBR-2026-TEST",
        "from_cbdc": "e-CNY",
        "to_cbdc": "e-AED",
        "amount": 10000,
        "currency": "AED",
        "sender": "BKCHCNBJXXX",
        "receiver": "FADBAEADXXX",
        "timestamp": 1750000000u64
    });

    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    let mut hasher = sha2::Sha256::new();
    hasher.update(&payload_bytes);
    let payload_hash = hex::encode(hasher.finalize());
    let device_id = format!(
        "{}test-device",
        conxian_compliance::zkc::TEE_DEVICE_ID_PREFIX
    );
    let att_header = make_attestation_header(&device_id, &payload_hash);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/ingress/mbridge")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-conxian-attestation", att_header)
        .header("x-conxian-trust-metadata", trust_metadata_json())
        .header("Content-Type", "application/json")
        .body(Body::from(payload_bytes))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    if status != 200 {
        println!("RESPONSE ERROR: {}", String::from_utf8_lossy(&body_bytes));
    }
    assert_eq!(status, StatusCode::OK);
}
