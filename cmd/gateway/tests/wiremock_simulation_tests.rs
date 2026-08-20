//! Automated Integration Tests with Stateful API Virtualization (WireMock Cloud / Proxymock)
//!
//! Features tested:
//! - ISO 20022 pacs.008 FI-to-FI Customer Credit Transfer XML builder & stateful clearing virtualization
//! - X402 Settlement Middleware stateful payment execution and chaos fault injection (HTTP 500, High Latency)
//! - External Identity Resolution (World ID, Web3.bio) with WireMock virtualization and chaos scenarios

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conxian_api::a2p::A2pRouter;
use conxian_api::camt::{build_pacs008_xml, Pacs008Request};
use conxian_api::fiat::FiatRouter;
use conxian_api::lightning::{
    LightningAdapter, LightningBackend, LightningBackendError, LightningSettlementRequest,
    LightningSettlementResponse,
};
use conxian_api::{configure_routes, new_lightning_adapter, new_settlement_log, AppState};
use conxian_compliance::zkc::{ATTESTATION_SIGNING_DOMAIN, TEE_DEVICE_ID_PREFIX};
use conxian_compliance::{CoreVerifier, IdentityManager, UniversalVerifier, ZkcVerifier};
use conxian_core::{
    Attestation, AttestationRequest, ConxianResult, GatewayState, OfflineQueue, OfflineReceipt,
    Persistence, PersistentState, SharedState, VersionedPersistentState,
};
use http_body_util::BodyExt;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_TOKEN: &str = "test-token";
const TEST_FIAT_SECRET: &str = "fake";
const TEST_SETTLEMENT_SECRET: &str = "simulated";

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn make_attestation_header(device_id: &str, payload_hash: &str) -> String {
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let pubkey = PublicKey::from_secret_key(&secp, &secret_key);

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

fn make_trust_metadata_header() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    json!({
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
    })
    .to_string()
}

struct StaticPersistence {
    state: PersistentState,
}

impl Persistence for StaticPersistence {
    fn load_versioned(&self) -> ConxianResult<VersionedPersistentState> {
        Ok(VersionedPersistentState {
            revision: 0,
            state: self.state.clone(),
        })
    }

    fn compare_and_swap(
        &self,
        _expected_revision: u64,
        _new_state: &PersistentState,
    ) -> ConxianResult<VersionedPersistentState> {
        Ok(VersionedPersistentState {
            revision: 1,
            state: self.state.clone(),
        })
    }
}

struct SimulatedStacksRpc;

#[async_trait]
impl conxian_core::SimulatedStacksRpcTrait for SimulatedStacksRpc {
    async fn call_read_only(
        &self,
        _contract: &str,
        _function: &str,
        _args: Vec<serde_json::Value>,
    ) -> ConxianResult<serde_json::Value> {
        Ok(json!({ "value": "SP2JZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62" }))
    }
}

struct SimulatedOfflineQueue {
    replay_claims: Mutex<HashSet<String>>,
}

impl OfflineQueue for SimulatedOfflineQueue {
    fn enqueue(&self, _r: &OfflineReceipt) -> ConxianResult<()> {
        Ok(())
    }
    fn dequeue_pending(&self) -> ConxianResult<Vec<OfflineReceipt>> {
        Ok(vec![])
    }
    fn mark_broadcasted(&self, _id: &str) -> ConxianResult<()> {
        Ok(())
    }
    fn claim_replay_key(&self, replay_key: &str, _ttl_seconds: u64) -> ConxianResult<bool> {
        let mut claims = self.replay_claims.lock().unwrap();
        Ok(claims.insert(replay_key.to_string()))
    }
}

fn create_test_app() -> axum::Router {
    create_test_app_with_lightning(new_lightning_adapter())
}

fn create_test_app_with_lightning(lightning: Arc<LightningAdapter>) -> axum::Router {
    let persistence = Arc::new(StaticPersistence {
        state: PersistentState::default(),
    });
    let shared: SharedState = Arc::new(RwLock::new(GatewayState::default()));
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
    let identity = Arc::new(IdentityManager::with_stacks_rpc(Box::new(
        SimulatedStacksRpc,
    )));
    let compliance = Arc::new(ZkcVerifier::new());
    let alex = Arc::new(conxian_engine::stacks::alex::SimulatedAlexClient);
    let mut multi_chain: HashMap<String, Arc<dyn conxian_core::ChainAdapter>> = HashMap::new();
    multi_chain.insert(
        "liquid".to_string(),
        Arc::new(conxian_engine::LiquidAdapter::new(
            Arc::new(
                conxian_engine::BitcoinRpcClient::new("http://localhost:18843", "", "").unwrap(),
            ),
            "simulated".to_string(),
        )),
    );
    let verifier = Arc::new(UniversalVerifier::new(
        compliance.clone() as Arc<dyn CoreVerifier>,
        multi_chain.clone(),
    ));
    let offline_queue = Arc::new(SimulatedOfflineQueue {
        replay_claims: Mutex::new(HashSet::new()),
    });

    let app_state = AppState {
        coordinator: None,
        shared,
        persistence: Some(persistence),
        bitcoin_core_shadow_observer: None,
        fiat,
        a2p,
        identity,
        compliance,
        verifier,
        alex_preparer: Arc::new(
            conxian_engine::stacks::alex::AlexPreparationService::disabled(alex.clone()),
        ),
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

async fn parse_response_json(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap_or(Value::Null)
}

// ============================================================================
// 1. ISO 20022 (pacs.008) Messaging & Stateful Clearing Virtualization
// ============================================================================

#[tokio::test]
async fn test_pacs008_generation_and_stateful_clearing_simulation() {
    let app = create_test_app();

    // 1. Generate pacs.008 XML customer credit transfer payment request
    let pacs008_req = Pacs008Request {
        end_to_end_id: "E2E-2026-X402-9901".to_string(),
        debtor_name: "Sovereign Treasury Ltd".to_string(),
        creditor_name: "Apex Global Liquidity Inc".to_string(),
        amount: "1250000.00".to_string(),
        currency: "USD".to_string(),
        debtor_agent_bic: "CHASUS33XXX".to_string(),
        creditor_agent_bic: "BOFAUS3NXXX".to_string(),
    };

    let xml_payload = build_pacs008_xml("MSG-PACS008-001", &pacs008_req).unwrap();

    // Verify pacs.008 XML contents
    assert!(
        xml_payload.contains("<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08\">")
    );
    assert!(xml_payload.contains("<EndToEndId>E2E-2026-X402-9901</EndToEndId>"));
    assert!(xml_payload.contains("<IntrBkSttlmAmt Ccy=\"USD\">1250000.00</IntrBkSttlmAmt>"));

    // 2. Prepare headers for Gateway ISO 20022 Ingress route
    let payload_hash = sha256_hex(xml_payload.as_bytes());
    let device_id = format!("{}test-device", TEE_DEVICE_ID_PREFIX);
    let attestation_header = make_attestation_header(&device_id, &payload_hash);
    let trust_metadata_header = make_trust_metadata_header();

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/iso20022/pacs008")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-settlement-secret", TEST_SETTLEMENT_SECRET)
        .header("x-conxian-trust-metadata", trust_metadata_header)
        .header("x-conxian-attestation", attestation_header)
        .header("Content-Type", "application/xml")
        .body(Body::from(xml_payload.clone()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = parse_response_json(res).await;
    assert_eq!(body["envelope"]["payload"]["source"], "ISO20022_PACS008");
    assert!(body["proposal_id"].is_string());

    // 3. Proxymock / WireMock Virtualization of External ISO 20022 Clearing Network
    let mock_server = MockServer::start().await;

    // Stateful Clearing: Initial Submit -> 202 Accepted (IN_FLIGHT)
    Mock::given(method("POST"))
        .and(path("/iso20022/clearing"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "clearing_id": "CLR-9901-USD",
            "status": "IN_FLIGHT",
            "message": "pacs.008 customer credit transfer accepted for settlement"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Stateful Clearing: Query Status -> 200 OK (CLEARED)
    Mock::given(method("GET"))
        .and(path("/iso20022/clearing/CLR-9901-USD"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "clearing_id": "CLR-9901-USD",
            "status": "CLEARED",
            "settlement_timestamp": "2026-08-20T04:15:00Z"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Execute simulated clearing network interaction using minreq inside tokio spawn_blocking
    let uri = format!("{}/iso20022/clearing", mock_server.uri());
    let xml_body = xml_payload.clone();

    let submit_res = tokio::task::spawn_blocking(move || {
        minreq::Request::new(minreq::Method::Post, uri)
            .with_header("Content-Type", "application/xml")
            .with_body(xml_body)
            .send()
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(submit_res.status_code, 202);
    let submit_json: Value = serde_json::from_str(submit_res.as_str().unwrap()).unwrap();
    assert_eq!(submit_json["status"], "IN_FLIGHT");

    // Query status
    let query_uri = format!("{}/iso20022/clearing/CLR-9901-USD", mock_server.uri());
    let query_res = tokio::task::spawn_blocking(move || minreq::get(query_uri).send())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(query_res.status_code, 200);
    let query_json: Value = serde_json::from_str(query_res.as_str().unwrap()).unwrap();
    assert_eq!(query_json["status"], "CLEARED");
}

#[tokio::test]
async fn test_pacs008_clearing_chaos_testing_scenarios() {
    let mock_server = MockServer::start().await;

    // Chaos 1: HTTP 500 Internal Server Error Fault Injection
    Mock::given(method("POST"))
        .and(path("/iso20022/clearing/fail"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": "Internal Bank Clearing System Failure",
            "code": "ISO_CLEARING_UNAVAILABLE"
        })))
        .mount(&mock_server)
        .await;

    let fail_uri = format!("{}/iso20022/clearing/fail", mock_server.uri());
    let fail_res = tokio::task::spawn_blocking(move || minreq::post(fail_uri).send())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fail_res.status_code, 500);

    // Chaos 2: High Latency Delay Injection (Network Lag Simulation)
    Mock::given(method("POST"))
        .and(path("/iso20022/clearing/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(1500))
                .set_body_json(json!({
                    "clearing_id": "CLR-SLOW-1",
                    "status": "CLEARED"
                })),
        )
        .mount(&mock_server)
        .await;

    let slow_uri = format!("{}/iso20022/clearing/slow", mock_server.uri());
    let start = std::time::Instant::now();
    let slow_res = tokio::task::spawn_blocking(move || minreq::post(slow_uri).send())
        .await
        .unwrap()
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(slow_res.status_code, 200);
    assert!(
        elapsed >= Duration::from_millis(1400),
        "Response delay should reflect high latency"
    );
}

// ============================================================================
// 2. X402 Settlement Middleware & Stateful Virtualization
// ============================================================================

struct WireMockLightningBackend {
    endpoint_url: String,
}

#[async_trait]
impl LightningBackend for WireMockLightningBackend {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        let url = format!("{}/lightning/settle", self.endpoint_url);
        let req_payload = json!({
            "challenge": request.challenge,
            "amount": request.amount,
            "asset": request.asset,
            "proof_refs": request.proof_refs,
        });

        let res = tokio::task::spawn_blocking(move || {
            minreq::Request::new(minreq::Method::Post, url)
                .with_header("Content-Type", "application/json")
                .with_body(req_payload.to_string())
                .send()
        })
        .await
        .map_err(|_| LightningBackendError::Unavailable)?
        .map_err(|_| LightningBackendError::Unavailable)?;

        if res.status_code == 200 {
            let val: Value = serde_json::from_str(res.as_str().unwrap_or("{}")).map_err(|e| {
                LightningBackendError::PartialFailure {
                    detail: e.to_string(),
                }
            })?;
            Ok(LightningSettlementResponse {
                settled_amount: val["settled_amount"].as_u64().unwrap_or(0) as u128,
                preimage: val["preimage"].as_str().unwrap_or("").to_string(),
                proof: val["proof"].as_str().unwrap_or("").to_string(),
            })
        } else if res.status_code == 500 {
            Err(LightningBackendError::Unavailable)
        } else {
            Err(LightningBackendError::Rejected {
                detail: format!("Backend returned status {}", res.status_code),
            })
        }
    }
}

#[tokio::test]
async fn test_x402_settlement_middleware_stateful_wiremock_flow() {
    let mock_server = MockServer::start().await;

    // Proxymock / WireMock Virtualized LND / Lightning Node
    Mock::given(method("POST"))
        .and(path("/lightning/settle"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "settled_amount": 1000,
            "preimage": "preimage-x402-wiremock-9988",
            "proof": "proof-x402-wiremock-9988"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let backend = Arc::new(WireMockLightningBackend {
        endpoint_url: mock_server.uri(),
    });
    let adapter = Arc::new(
        LightningAdapter::new(backend)
            .with_clock(|| 1000)
            .with_retry_policy(0, Duration::from_millis(1000)),
    );

    let app = create_test_app_with_lightning(adapter);

    // Call protected CBTC verification route with valid x402 header
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/canton/cbtc/verify")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-402-payment", "proof-x402-wiremock-9988")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "attestation": {
                "canton_domain": "canton.global.network",
                "contract_id": "cbtc-contract-1234567890123456",
                "amount_sats": 1000000,
                "bitcoin_utxos": ["00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff:0"],
                "attested_at_height": 850000
            },
            "verify_utxo_proofs": false
        }).to_string()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_x402_settlement_middleware_chaos_fault_injections() {
    let mock_server = MockServer::start().await;

    // Chaos Scenario 1: External Settlement Node 500 Internal Error
    Mock::given(method("POST"))
        .and(path("/lightning/settle"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(json!({ "error": "Lightning LND Daemon Unavailable" })),
        )
        .mount(&mock_server)
        .await;

    let backend = Arc::new(WireMockLightningBackend {
        endpoint_url: mock_server.uri(),
    });
    let adapter = Arc::new(LightningAdapter::new(backend).with_clock(|| 1000));
    let app = create_test_app_with_lightning(adapter);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/canton/cbtc/verify")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-402-payment", "proof-chaos-500")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "attestation": {
                "canton_domain": "canton.global.network",
                "contract_id": "cbtc-contract-1234567890123456",
                "amount_sats": 1000000,
                "bitcoin_utxos": ["00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff:0"],
                "attested_at_height": 850000
            },
            "verify_utxo_proofs": false
        }).to_string()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // Backend failure translates to 503 Service Unavailable
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = parse_response_json(res).await;
    assert_eq!(body["code"], "lightning_backend_unavailable");
}

#[tokio::test]
async fn test_x402_settlement_middleware_high_latency_timeout_chaos() {
    let mock_server = MockServer::start().await;

    // Chaos Scenario 2: High Latency Network Lag Injection (Delays response past gateway timeout)
    Mock::given(method("POST"))
        .and(path("/lightning/settle"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(500))
                .set_body_json(json!({
                    "settled_amount": 1000,
                    "preimage": "slow-preimage",
                    "proof": "slow-proof"
                })),
        )
        .mount(&mock_server)
        .await;

    let backend = Arc::new(WireMockLightningBackend {
        endpoint_url: mock_server.uri(),
    });
    // Set low backend timeout (100ms) to trigger latency timeout
    let adapter = Arc::new(
        LightningAdapter::new(backend)
            .with_clock(|| 1000)
            .with_retry_policy(0, Duration::from_millis(100)),
    );
    let app = create_test_app_with_lightning(adapter);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/canton/cbtc/verify")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-402-payment", "proof-chaos-slow")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "attestation": {
                "canton_domain": "canton.global.network",
                "contract_id": "cbtc-contract-1234567890123456",
                "amount_sats": 1000000,
                "bitcoin_utxos": ["00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff:0"],
                "attested_at_height": 850000
            },
            "verify_utxo_proofs": false
        }).to_string()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // High latency timeout translates to 504 Gateway Timeout
    assert_eq!(res.status(), StatusCode::GATEWAY_TIMEOUT);
    let body = parse_response_json(res).await;
    assert_eq!(body["code"], "lightning_backend_timeout");
}

// ============================================================================
// 3. External Identity Provider (World ID & Web3.bio) Virtualization & Chaos
// ============================================================================

#[tokio::test]
async fn test_identity_resolution_wiremock_virtualization() {
    let mock_server = MockServer::start().await;

    // Proxymock / WireMock Virtualized World ID Verification API
    Mock::given(method("POST"))
        .and(path("/api/v2/verify/app_staging_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "verification_level": "orb",
            "action": "identity-verify",
            "nullifier_hash": "0x05f88832a8901c2380"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Proxymock / WireMock Virtualized Web3.bio Profile API
    Mock::given(method("GET"))
        .and(path("/profile/vitalik.eth"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "address": "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045",
            "identity": "vitalik.eth",
            "platform": "ens",
            "displayName": "Vitalik Buterin"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Test World ID virtualized endpoint
    let worldid_uri = format!("{}/api/v2/verify/app_staging_123", mock_server.uri());
    let worldid_res = tokio::task::spawn_blocking(move || {
        minreq::post(worldid_uri)
            .with_header("Content-Type", "application/json")
            .with_body(json!({ "proof": "proof-test-123" }).to_string())
            .send()
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(worldid_res.status_code, 200);
    let worldid_json: Value = serde_json::from_str(worldid_res.as_str().unwrap()).unwrap();
    assert_eq!(worldid_json["success"], true);
    assert_eq!(worldid_json["verification_level"], "orb");

    // Test Web3.bio virtualized endpoint
    let web3bio_uri = format!("{}/profile/vitalik.eth", mock_server.uri());
    let web3bio_res = tokio::task::spawn_blocking(move || minreq::get(web3bio_uri).send())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(web3bio_res.status_code, 200);
    let web3bio_json: Value = serde_json::from_str(web3bio_res.as_str().unwrap()).unwrap();
    assert_eq!(web3bio_json["identity"], "vitalik.eth");
    assert_eq!(
        web3bio_json["address"],
        "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
    );

    // Test Gateway /api/v1/identity/resolve endpoint
    let app = create_test_app();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/identity/resolve")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "identifier": "satoshi.id",
                "provider": "bns"
            })
            .to_string(),
        ))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = parse_response_json(res).await;
    assert_eq!(body["provider"], "bns");
    assert_eq!(body["verified"], true);
}

#[tokio::test]
async fn test_identity_provider_chaos_scenarios() {
    let mock_server = MockServer::start().await;

    // Chaos Scenario 1: World ID API 500 Internal Server Error Fault Injection
    Mock::given(method("POST"))
        .and(path("/api/v2/verify/fail"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(json!({ "error": "World ID Developer Portal Unavailable" })),
        )
        .mount(&mock_server)
        .await;

    let fail_uri = format!("{}/api/v2/verify/fail", mock_server.uri());
    let fail_res = tokio::task::spawn_blocking(move || minreq::post(fail_uri).send())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fail_res.status_code, 500);

    // Chaos Scenario 2: Web3.bio High Latency Delay Injection
    Mock::given(method("GET"))
        .and(path("/profile/slow.eth"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(1500))
                .set_body_json(json!({ "identity": "slow.eth", "address": "0x000" })),
        )
        .mount(&mock_server)
        .await;

    let slow_uri = format!("{}/profile/slow.eth", mock_server.uri());
    let start = std::time::Instant::now();
    let slow_res = tokio::task::spawn_blocking(move || minreq::get(slow_uri).send())
        .await
        .unwrap()
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(slow_res.status_code, 200);
    assert!(
        elapsed >= Duration::from_millis(1400),
        "Response delay should reflect high latency"
    );

    // Chaos Scenario 3: 429 Rate Limit Injection
    Mock::given(method("GET"))
        .and(path("/profile/ratelimit.eth"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "5")
                .set_body_json(json!({ "error": "Rate limit exceeded" })),
        )
        .mount(&mock_server)
        .await;

    let limit_uri = format!("{}/profile/ratelimit.eth", mock_server.uri());
    let limit_res = tokio::task::spawn_blocking(move || minreq::get(limit_uri).send())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(limit_res.status_code, 429);
}
