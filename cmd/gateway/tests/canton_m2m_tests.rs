//! Integration tests for Canton Network and Machine Economy endpoints (G-C1 through G-C6).
//!
//! Covers: CBTC verification, machine identity resolution, M2M settlement,
//! Canton state translation, CCIP routing, and machine RWA revenue verification.
//!
//! Test strategy: real handler logic with simulated/mocked backends (Lightning,
//! compliance verifier). No live network calls.

use api::lightning::{
    LightningAdapter, LightningBackend, LightningBackendError, LightningSettlementRequest,
    LightningSettlementResponse,
};
use api::{configure_routes, AppState};
use async_trait::async_trait;
use axum::{body::Body, http::Request};
use compliance::ZkcVerifier;
use http_body_util::BodyExt;
use serde_json::json;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

// ── Inline test backend (mirrors the pattern in lightning.rs tests) ────────

struct SimulatedOutcome {
    result: Result<LightningSettlementResponse, LightningBackendError>,
}

impl Clone for SimulatedOutcome {
    fn clone(&self) -> Self {
        SimulatedOutcome {
            result: self.result.clone(),
        }
    }
}

struct SequenceBackend {
    outcomes: Mutex<Vec<SimulatedOutcome>>,
    calls: Mutex<u32>,
}

impl SequenceBackend {
    fn new(outcomes: Vec<SimulatedOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes),
            calls: Mutex::new(0),
        }
    }
    /// Build a backend that returns the same outcome N times (for double-execution
    /// scenarios where x402 middleware + handler both call execute_payment).
    fn repeating(
        outcome: Result<LightningSettlementResponse, LightningBackendError>,
        count: usize,
    ) -> Self {
        Self::new(vec![SimulatedOutcome { result: outcome }; count])
    }
}

#[async_trait]
impl LightningBackend for SequenceBackend {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        let outcome = {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            let outcomes = self.outcomes.lock().unwrap();
            if outcomes.is_empty() {
                return Err(LightningBackendError::Unavailable);
            }
            // Cycle through outcomes if we have fewer queued than calls made
            let idx = (*calls - 1) as usize % outcomes.len();
            outcomes[idx].result.clone()
        };

        match outcome {
            Err(e) => Err(e),
            Ok(response) => {
                let preimage = request
                    .proof_refs
                    .iter()
                    .find(|v| v.starts_with("preimage-") || v.contains("preimage"))
                    .cloned()
                    .unwrap_or_else(|| response.preimage.clone());

                let proof = request
                    .proof_refs
                    .iter()
                    .find(|v| v.starts_with("proof-") || v.contains("proof"))
                    .cloned()
                    .or_else(|| request.proof_refs.first().cloned())
                    .unwrap_or_else(|| response.proof.clone());

                Ok(LightningSettlementResponse {
                    settled_amount: request.amount,
                    preimage,
                    proof,
                })
            }
        }
    }
}

fn ok_response() -> SimulatedOutcome {
    SimulatedOutcome {
        result: Ok(LightningSettlementResponse {
            settled_amount: 1000,
            preimage: "test-preimage-123".to_string(),
            proof: "test-proof".to_string(),
        }),
    }
}

fn make_test_state(lightning: Arc<LightningAdapter>) -> AppState {
    let state: conxian_core::SharedState =
        Arc::new(RwLock::new(conxian_core::GatewayState::default()));
    let fiat = Arc::new(api::fiat::FiatRouter::new(
        "ramp-key".to_string(),
        "investec-id".to_string(),
        "investec-secret".to_string(),
        "alchemy-id".to_string(),
        "alchemy-secret".to_string(),
        "banxa-key".to_string(),
        "banxa-secret".to_string(),
    ));
    let a2p = Arc::new(api::a2p::A2pRouter::new(
        "sentinel_infobip".to_string(),
        "test-infobip".to_string(),
        "test-hmac".to_string(),
    ));
    let identity = Arc::new(compliance::IdentityManager::new());
    let compliance = Arc::new(ZkcVerifier::new());
    let alex = Arc::new(engine::stacks::alex::SimulatedAlexClient);
    let multi_chain = std::collections::HashMap::new();
    let offline_queue = Arc::new(SimulatedOfflineQueue {
        replay_claims: Mutex::new(HashSet::new()),
    });
    AppState {
        coordinator: None,
        shared: state,
        fiat,
        a2p,
        identity,
        compliance: compliance.clone(),
        verifier: Arc::new(compliance::UniversalVerifier::new(
            compliance as Arc<dyn compliance::CoreVerifier>,
            multi_chain.clone(),
        )),
        alex,
        multi_chain,
        lightning,
        fiat_webhook_secret: "fake".to_string(),
        settlement_ingress_secret: "simulated".to_string(),
        settlement_log: api::new_settlement_log(),
        offline_queue,
    }
}

fn test_app() -> axum::Router {
    let backend = SequenceBackend::repeating(ok_response().result, 2);
    let adapter = LightningAdapter::new(Arc::new(backend));
    let state = make_test_state(Arc::new(adapter));
    configure_routes(
        state,
        "test-token".to_string(),
        std::time::Instant::now(),
        None,
    )
}

fn authed_request(uri: &str, method: axum::http::Method, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("Authorization", "Bearer test-token")
        .header("x-402-payment", "proof-test")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

fn post_request(uri: &str, body: serde_json::Value) -> Request<Body> {
    authed_request(uri, axum::http::Method::POST, body)
}

// ── G-C1: CBTC Non-Custodial Verification ─────────────────────────────────

#[tokio::test]
async fn test_cbtc_verify_valid_attestation() {
    let app = test_app();
    let payload = json!({
        "attestation": {
            "canton_domain": "global",
            "contract_id": "ContractId:00234abcd5678",
            "amount_sats": 1_000_000,
            "bitcoin_utxos": ["txid1234567890abcdef:0"],
            "frost_attestation": "sig_frost_threshold",
            "attested_at_height": 850_000,
            "quorum": {
                "signers_present": 3,
                "signers_total": 5,
                "aggregate_key": "pk_frost_agg"
            }
        }
    });

    let response = app
        .oneshot(post_request("/api/v1/canton/cbtc/verify", payload))
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    eprintln!(
        "CBTC STATUS: {:?} BODY: {:?}",
        status,
        String::from_utf8_lossy(&body_bytes)
    );

    assert_eq!(status, axum::http::StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["verified"], true);
    assert_eq!(body["contract_id"], "ContractId:00234abcd5678");
    assert_eq!(body["amount_sats"], 1_000_000);
    assert_eq!(body["utxos_verified"], 1);
    assert!(body["quorum_ratio"].is_number());
}

#[tokio::test]
async fn test_cbtc_verify_missing_contract_id() {
    let app = test_app();
    let payload = json!({
        "attestation": {
            "canton_domain": "global",
            "contract_id": "",  // empty — should fail
            "amount_sats": 1_000_000,
            "bitcoin_utxos": ["txid1234567890abcdef:0"],
            "attested_at_height": 850_000
        }
    });

    let response = app
        .oneshot(post_request("/api/v1/canton/cbtc/verify", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK); // handler still returns 200
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    // Handler returns verified=false but 200 — check the verified field
    assert_eq!(body["verified"], false);
    // contract_id_format check should be false
    let checks: Vec<serde_json::Value> = serde_json::from_value(body["checks"].clone()).unwrap();
    let contract_check = checks
        .iter()
        .find(|c| c["check"] == "contract_id_format")
        .unwrap();
    assert_eq!(contract_check["passed"], false);
}

#[tokio::test]
async fn test_cbtc_verify_zero_amount() {
    let app = test_app();
    let payload = json!({
        "attestation": {
            "canton_domain": "global",
            "contract_id": "ContractId:00234abcd5678",
            "amount_sats": 0,  // zero — exceeds supply cap
            "bitcoin_utxos": [],
            "attested_at_height": 850_000
        }
    });

    let response = app
        .oneshot(post_request("/api/v1/canton/cbtc/verify", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["verified"], false);
    let checks: Vec<serde_json::Value> = serde_json::from_value(body["checks"].clone()).unwrap();
    let amount_check = checks
        .iter()
        .find(|c| c["check"] == "amount_valid")
        .unwrap();
    assert_eq!(amount_check["passed"], false);
}

#[tokio::test]
async fn test_cbtc_verify_oversized_amount() {
    let app = test_app();
    let payload = json!({
        "attestation": {
            "canton_domain": "global",
            "contract_id": "ContractId:00234abcd5678",
            "amount_sats": 3_000_000_000_000_000_u64, // exceeds 21M BTC cap
            "bitcoin_utxos": [],
            "attested_at_height": 850_000
        }
    });

    let response = app
        .oneshot(post_request("/api/v1/canton/cbtc/verify", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["verified"], false);
}

#[tokio::test]
async fn test_cbtc_verify_quorum_pass_without_metadata() {
    // Quorum absence should pass with a caveat detail, not hard-fail
    let app = test_app();
    let payload = json!({
        "attestation": {
            "canton_domain": "global",
            "contract_id": "ContractId:00234abcd5678",
            "amount_sats": 1_000_000,
            "bitcoin_utxos": ["txid1234567890abcdef:0"],
            "attested_at_height": 850_000
            // no quorum field
        }
    });

    let response = app
        .oneshot(post_request("/api/v1/canton/cbtc/verify", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let checks: Vec<serde_json::Value> = serde_json::from_value(body["checks"].clone()).unwrap();
    let quorum_check = checks
        .iter()
        .find(|c| c["check"] == "quorum_valid")
        .unwrap();
    assert_eq!(quorum_check["passed"], true); // passes with caveat
    assert!(quorum_check["detail"]
        .as_str()
        .unwrap()
        .contains("No quorum metadata"));
}

// ── G-C2: Machine Identity Resolution ─────────────────────────────────────

#[tokio::test]
async fn test_machine_identity_resolve_peaq() {
    let app = test_app();
    let payload = json!({
        "identifier": "0x742d35Cc6634C0532925a3b844Bc9e7595f1b1E4",
        "provider": "peaq",
        "machine_type_hint": "SENSOR"
    });

    let response = app
        .oneshot(post_request("/api/v1/identity/resolve/machine", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["provider"], "peaq");
    assert_eq!(
        body["identity"]["peaq_did"],
        "did:peaq:0x742d35Cc6634C0532925a3b844Bc9e7595f1b1E4"
    );
    assert_eq!(body["verified"], false); // no signature provided
}

#[tokio::test]
async fn test_machine_identity_resolve_dimo() {
    let app = test_app();
    let payload = json!({
        "identifier": "vehicle:abc123",
        "provider": "dimo",
        "machine_type_hint": "ELECTRIC_VEHICLE"
    });

    let response = app
        .oneshot(post_request("/api/v1/identity/resolve/machine", payload))
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    eprintln!(
        "DIMO STATUS: {:?} BODY: {:?}",
        status,
        String::from_utf8_lossy(&body_bytes)
    );
    assert_eq!(status, axum::http::StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["provider"], "dimo");
    assert_eq!(body["identity"]["dimo_vehicle_id"], "vehicle:abc123");
}

#[tokio::test]
async fn test_machine_identity_resolve_device_key() {
    let app = test_app();
    let payload = json!({
        "identifier": "xonly_pubkey_abc123",
        "provider": "device_key",
        "machine_type_hint": "ROBOT"
    });

    let response = app
        .oneshot(post_request("/api/v1/identity/resolve/machine", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["provider"], "device_key");
    assert_eq!(body["identity"]["device_key"], "xonly_pubkey_abc123");
}

#[tokio::test]
async fn test_machine_identity_resolve_unknown_provider() {
    let app = test_app();
    let payload = json!({
        "identifier": "some_id",
        "provider": "unknown_depin"
    });

    let response = app
        .oneshot(post_request("/api/v1/identity/resolve/machine", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Unsupported machine identity provider"));
}

// ── G-C3: M2M Settlement ─────────────────────────────────────────────────

#[tokio::test]
async fn test_m2m_settle_lightning_success() {
    let app = test_app();
    let payload = json!({
        "source_machine": {
            "device_key": "src_key_abc123",
            "machine_type": "SENSOR"
        },
        "target_machine": {
            "device_key": "tgt_key_def456",
            "machine_type": "COMPUTE_NODE"
        },
        "service_type": "DATA",
        "settlement_rail": "LIGHTNING",
        "amount_minor": 1000,
        "amount_scale": 8,
        "currency": "BTC",
        "payment_request": "lnbc1pnsp...",
        "timestamp": 2_000_000_000i64  // 2033-05-18, passes expiry validation
    });

    let response = app
        .oneshot(post_request("/api/v1/m2m/settle", payload))
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    eprintln!(
        "M2M-LN STATUS: {:?} BODY: {:?}",
        status,
        String::from_utf8_lossy(&body_bytes)
    );
    assert_eq!(status, axum::http::StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["status"], "SETTLED");
    assert_eq!(body["settlement_rail"], "LIGHTNING");
    assert!(body["settlement_proof"].is_string());
    assert!(body["settlement_id"]
        .as_str()
        .unwrap()
        .starts_with("m2m-ln-"));
}

#[tokio::test]
async fn test_m2m_settle_missing_source_device_key() {
    let app = test_app();
    let payload = json!({
        "source_machine": {
            "device_key": "",  // empty
            "machine_type": "SENSOR"
        },
        "target_machine": {
            "device_key": "tgt_key_def456",
            "machine_type": "COMPUTE_NODE"
        },
        "service_type": "DATA",
        "settlement_rail": "LIGHTNING",
        "amount_minor": 1000,
        "amount_scale": 8,
        "currency": "BTC",
        "payment_request": "lnbc1pnsp...",
        "timestamp": 1_000_000
    });

    let response = app
        .oneshot(post_request("/api/v1/m2m/settle", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("device keys"));
}

#[tokio::test]
async fn test_m2m_settle_zero_amount() {
    let app = test_app();
    let payload = json!({
        "source_machine": { "device_key": "src_key", "machine_type": "SENSOR" },
        "target_machine": { "device_key": "tgt_key", "machine_type": "SENSOR" },
        "service_type": "DATA",
        "settlement_rail": "LIGHTNING",
        "amount_minor": 0,  // zero
        "amount_scale": 8,
        "currency": "BTC",
        "payment_request": "lnbc1pnsp...",
        "timestamp": 1_000_000
    });

    let response = app
        .oneshot(post_request("/api/v1/m2m/settle", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("amount_minor"));
}

#[tokio::test]
async fn test_m2m_settle_invalid_scale() {
    let app = test_app();
    let payload = json!({
        "source_machine": { "device_key": "src_key", "machine_type": "SENSOR" },
        "target_machine": { "device_key": "tgt_key", "machine_type": "SENSOR" },
        "service_type": "DATA",
        "settlement_rail": "LIGHTNING",
        "amount_minor": 1000,
        "amount_scale": 100, // exceeds MAX_DECIMALS=38
        "currency": "BTC",
        "payment_request": "lnbc1pnsp...",
        "timestamp": 1_000_000
    });

    let response = app
        .oneshot(post_request("/api/v1/m2m/settle", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("amount_scale"));
}

#[tokio::test]
async fn test_m2m_settle_peaq_rail_not_implemented() {
    let app = test_app();
    let payload = json!({
        "source_machine": { "device_key": "src_key", "machine_type": "SENSOR" },
        "target_machine": { "device_key": "tgt_key", "machine_type": "SENSOR" },
        "service_type": "DATA",
        "settlement_rail": "PEAQ",  // not yet implemented
        "amount_minor": 1000,
        "amount_scale": 18,
        "currency": "PEAQ",
        "timestamp": 1_000_000
    });

    let response = app
        .oneshot(post_request("/api/v1/m2m/settle", payload))
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    eprintln!(
        "M2M STATUS: {:?} BODY: {:?}",
        status,
        String::from_utf8_lossy(&body_bytes)
    );

    assert_eq!(status, axum::http::StatusCode::NOT_IMPLEMENTED);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["message"].as_str().unwrap().contains("Peaq"));
}

// ── G-C4: Canton State Translation ────────────────────────────────────────

#[tokio::test]
async fn test_canton_translate_asset_transfer() {
    let app = test_app();
    let payload = json!({
        "domain": {
            "domain_name": "global",
            "synchronizer_endpoint": "https://canton.example.com",
            "public_observer": true
        },
        "daml_contract_id": "ContractId:00567abcd8901",
        "template_name": "AssetTransfer",
        "target_ledger": "bitcoin"
    });

    let response = app
        .oneshot(post_request("/api/v1/canton/state/translate", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["source_ledger"], "canton");
    assert_eq!(body["target_ledger"], "bitcoin");
    assert_eq!(body["translation_complete"], true);
    assert!(body["unmapped_fields"].as_array().unwrap().is_empty());
    assert_eq!(body["contract_ref"]["ledger"], "canton");
}

#[tokio::test]
async fn test_canton_translate_unknown_template() {
    let app = test_app();
    let payload = json!({
        "domain": { "domain_name": "global" },
        "daml_contract_id": "ContractId:00567abcd8901",
        "template_name": "UnknownTemplate",
        "target_ledger": "stacks"
    });

    let response = app
        .oneshot(post_request("/api/v1/canton/state/translate", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["translation_complete"], false);
    assert!(!body["unmapped_fields"].as_array().unwrap().is_empty());
    assert!(body["unmapped_fields"][0]
        .as_str()
        .unwrap()
        .contains("UnknownTemplate"));
}

#[tokio::test]
async fn test_canton_translate_missing_domain() {
    let app = test_app();
    let payload = json!({
        "domain": { "domain_name": "" },
        "daml_contract_id": "ContractId:00567abcd8901",
        "target_ledger": "bitcoin"
    });

    let response = app
        .oneshot(post_request("/api/v1/canton/state/translate", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("domain"));
}

// ── G-C5: CCIP Compliance Routing ────────────────────────────────────────

#[tokio::test]
async fn test_ccip_route_canton_to_ethereum_low_risk() {
    let app = test_app();
    let payload = json!({
        "message": {
            "source_chain": "canton",
            "destination_chain": "ethereum",
            "message_id": "msg-123",
            "payload": "0xdeadbeef",
            "requires_screening": false
        },
        "elevated_scrutiny": false
    });

    let response = app
        .oneshot(post_request("/api/v1/ccip/route", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["approved"], true);
    assert_eq!(body["risk_level"], "LOW");
}

#[tokio::test]
async fn test_ccip_route_spfs_high_risk() {
    let app = test_app();
    let payload = json!({
        "message": {
            "source_chain": "spfs",
            "destination_chain": "ethereum",
            "message_id": "msg-456",
            "payload": "0xcafebabe"
        },
        "elevated_scrutiny": true  // triggers escalate_risk: High → Critical → blocked
    });

    let response = app
        .oneshot(post_request("/api/v1/ccip/route", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    // SPFS with elevated scrutiny → CRITICAL (blocked); escalated from HIGH by escalate_risk()
    assert_eq!(body["approved"], false);
    assert_eq!(body["risk_level"], "CRITICAL");
    assert!(body["rejection_reason"].is_string());
}

#[tokio::test]
async fn test_ccip_route_mbridge_medium_risk() {
    let app = test_app();
    let payload = json!({
        "message": {
            "source_chain": "mbridge",
            "destination_chain": "canton",
            "message_id": "msg-789",
            "payload": "0xbabe"
        }
    });

    let response = app
        .oneshot(post_request("/api/v1/ccip/route", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["risk_level"], "MEDIUM");
    assert_eq!(body["approved"], true); // Medium is approved, not escalated to High
}

#[tokio::test]
async fn test_ccip_route_elevated_scrutiny_escalates_low_to_medium() {
    let app = test_app();
    let payload = json!({
        "message": {
            "source_chain": "canton",
            "destination_chain": "ethereum",
            "message_id": "msg-esc",
            "payload": "0x"
        },
        "elevated_scrutiny": true  // escalation
    });

    let response = app
        .oneshot(post_request("/api/v1/ccip/route", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["risk_level"], "MEDIUM"); // Low escalated to Medium
}

#[tokio::test]
async fn test_ccip_route_missing_message_id() {
    let app = test_app();
    let payload = json!({
        "message": {
            "source_chain": "canton",
            "destination_chain": "ethereum",
            "message_id": "",  // empty
            "payload": "0x"
        }
    });

    let response = app
        .oneshot(post_request("/api/v1/ccip/route", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_ccip_route_unknown_chain_defaults_to_medium() {
    let app = test_app();
    let payload = json!({
        "message": {
            "source_chain": "unknown_chain_xyz",
            "destination_chain": "another_unknown",
            "message_id": "msg-abc",
            "payload": "0x"
        }
    });

    let response = app
        .oneshot(post_request("/api/v1/ccip/route", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["risk_level"], "MEDIUM"); // Unknown → Medium
}

// ── G-C6: Machine RWA Revenue Verification ────────────────────────────────

#[tokio::test]
async fn test_machine_rwa_verify_valid_revenue() {
    let app = test_app();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload = json!({
        "revenue": {
            "machine_identity": {
                "device_key": "machine_key_abc",
                "machine_type": "SENSOR",
                "peaq_did": "did:peaq:0x123"
            },
            "period_start": now - 86400,
            "period_end": now,
            "total_revenue_minor": 5000,
            "currency": "BTC",
            "revenue_sources": [
                {
                    "service_type": "DATA",
                    "amount_minor": 3000,
                    "event_count": 100
                },
                {
                    "service_type": "COMPUTE",
                    "amount_minor": 2000,
                    "event_count": 50
                }
            ]
        },
        "verify_signature": false
    });

    let response = app
        .oneshot(post_request("/api/v1/rwa/machine/verify-revenue", payload))
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["verified"], true);
    assert_eq!(body["verified_revenue_minor"], 5000);
    assert_eq!(body["sources_verified"], 2);
    assert_eq!(body["holder_distribution_bps"], 9000); // 90%
}

#[tokio::test]
async fn test_machine_rwa_verify_revenue_mismatch() {
    let app = test_app();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload = json!({
        "revenue": {
            "machine_identity": { "device_key": "machine_key", "machine_type": "SENSOR" },
            "period_start": now - 86400,
            "period_end": now,
            "total_revenue_minor": 5000,
            "currency": "BTC",
            "revenue_sources": [
                // sum is 3000, but total_revenue_minor is 5000 — mismatch
                { "service_type": "DATA", "amount_minor": 1500, "event_count": 10 },
                { "service_type": "COMPUTE", "amount_minor": 1500, "event_count": 5 }
            ]
        },
        "verify_signature": false
    });

    let response = app
        .oneshot(post_request("/api/v1/rwa/machine/verify-revenue", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["verified"], false);
    assert_eq!(body["verified_revenue_minor"], 0); // zeroed on failure
}

#[tokio::test]
async fn test_machine_rwa_verify_zero_event_count_fails_sources_ok() {
    // sources_ok requires event_count > 0 AND amount_minor > 0.
    // Zero event_count makes sources_ok=false, which propagates to all_passed=false.
    let app = test_app();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload = json!({
        "revenue": {
            "machine_identity": { "device_key": "machine_key", "machine_type": "SENSOR" },
            "period_start": now - 86400,
            "period_end": now,
            "total_revenue_minor": 3000,
            "currency": "BTC",
            "revenue_sources": [
                { "service_type": "DATA", "amount_minor": 3000, "event_count": 0 }  // zero events
            ]
        },
        "verify_signature": false
    });

    let response = app
        .oneshot(post_request("/api/v1/rwa/machine/verify-revenue", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    // sources_ok = false (event_count=0), included in all_passed → verified=false
    assert_eq!(body["verified"], false);
}

#[tokio::test]
async fn test_machine_rwa_verify_future_period_fails() {
    let app = test_app();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload = json!({
        "revenue": {
            "machine_identity": { "device_key": "machine_key", "machine_type": "SENSOR" },
            "period_start": now + 86400,  // future
            "period_end": now + 172800,
            "total_revenue_minor": 5000,
            "currency": "BTC",
            "revenue_sources": []
        },
        "verify_signature": false
    });

    let response = app
        .oneshot(post_request("/api/v1/rwa/machine/verify-revenue", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["verified"], false);
    let checks: Vec<serde_json::Value> = serde_json::from_value(body["checks"].clone()).unwrap();
    let period_check = checks
        .iter()
        .find(|c| c["check"] == "revenue_period_valid")
        .unwrap();
    assert_eq!(period_check["passed"], false);
}

#[tokio::test]
async fn test_machine_rwa_verify_empty_sources_passes() {
    // Empty revenue_sources is allowed (sum_ok short-circuits)
    let app = test_app();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload = json!({
        "revenue": {
            "machine_identity": { "device_key": "machine_key", "machine_type": "SENSOR" },
            "period_start": now - 86400,
            "period_end": now,
            "total_revenue_minor": 0,
            "currency": "BTC",
            "revenue_sources": []  // empty — allowed
        },
        "verify_signature": false
    });

    let response = app
        .oneshot(post_request("/api/v1/rwa/machine/verify-revenue", payload))
        .await
        .unwrap();

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["verified"], true); // no sources → no source failure
}

// ── Auth guard tests ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_new_endpoints_require_auth() {
    let backend = SequenceBackend::new(vec![SimulatedOutcome {
        result: Ok(LightningSettlementResponse {
            settled_amount: 1000,
            preimage: "pre".to_string(),
            proof: "prf".to_string(),
        }),
    }]);
    let adapter = LightningAdapter::new(Arc::new(backend));
    let state = make_test_state(Arc::new(adapter));
    let app = configure_routes(
        state,
        "test-token".to_string(),
        std::time::Instant::now(),
        None,
    );

    let test_cases = [
        ("/api/v1/canton/cbtc/verify", json!({"attestation": {}})),
        (
            "/api/v1/identity/resolve/machine",
            json!({"identifier": "x", "provider": "peaq"}),
        ),
        (
            "/api/v1/m2m/settle",
            json!({"source_machine": {"device_key": "a", "machine_type": "SENSOR"}, "target_machine": {"device_key": "b", "machine_type": "SENSOR"}, "service_type": "DATA", "settlement_rail": "LIGHTNING", "amount_minor": 1, "amount_scale": 8, "currency": "BTC", "payment_request": "x", "timestamp": 1}),
        ),
        (
            "/api/v1/canton/state/translate",
            json!({"domain": {"domain_name": "g"}, "daml_contract_id": "c", "target_ledger": "b"}),
        ),
        (
            "/api/v1/ccip/route",
            json!({"message": {"source_chain": "c", "destination_chain": "e", "message_id": "m", "payload": "0x"}}),
        ),
        (
            "/api/v1/rwa/machine/verify-revenue",
            json!({"revenue": {"machine_identity": {"device_key": "k", "machine_type": "SENSOR"}, "period_start": 1, "period_end": 2, "total_revenue_minor": 0, "currency": "BTC", "revenue_sources": []}}),
        ),
    ];

    for (uri, payload) in test_cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNAUTHORIZED,
            "auth required for {uri}"
        );
    }
}

// ── OfflineQueue helper (required by make_test_state) ─────────────────────

struct SimulatedOfflineQueue {
    replay_claims: Mutex<HashSet<String>>,
}

impl conxian_core::OfflineQueue for SimulatedOfflineQueue {
    fn enqueue(&self, _r: &conxian_core::OfflineReceipt) -> conxian_core::ConxianResult<()> {
        Ok(())
    }

    fn dequeue_pending(&self) -> conxian_core::ConxianResult<Vec<conxian_core::OfflineReceipt>> {
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
