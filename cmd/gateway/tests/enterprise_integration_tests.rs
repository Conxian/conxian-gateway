//! E2E Integration tests for Enterprise ERP & Blockchain Integration Spec.
//!
//! Covers the 5 enterprise integration workflows:
//! 1. SWIFT CBPR+ / ISO 20022 (pain.001 / pacs.008 XML validation & translation to EVM / Bitcoin L2)
//! 2. Retail & POS (JSON Webhook event normalization targeting L2 State Channels)
//! 3. Logistics & Supply (EDI Purchase Order SHA-256 document hashing targeting Merkle Tree Storage)
//! 4. SME Invoicing (UBL XML / line item state sync targeting Escrow Contracts)
//! 5. Compliance & KYC (PostalAddress extraction, sanitization & ZK audit commitments)

use async_trait::async_trait;
use axum::{body::Body, http::Request};
use conxian_api::lightning::{
    LightningAdapter, LightningBackend, LightningBackendError, LightningSettlementRequest,
    LightningSettlementResponse,
};
use conxian_api::{configure_routes, AppState};
use conxian_compliance::ZkcVerifier;
use http_body_util::BodyExt;
use serde_json::json;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, RwLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
use sha2::Digest;

const TEST_TOKEN: &str = "valid_test_token_123456";

fn make_attestation_header(device_id: &str, payload_hash: &str) -> String {
    use secp256k1::{Message, Secp256k1, SecretKey};
    use sha2::{Digest, Sha256};

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

    fn claim_replay_key(&self, _replay_key: &str, _ttl_seconds: u64) -> conxian_core::ConxianResult<bool> {
        Ok(true)
    }
}

struct DummyBackend;

#[async_trait]
impl LightningBackend for DummyBackend {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        let preimage = request
            .proof_refs
            .iter()
            .find(|v| v.starts_with("preimage-") || v.contains("preimage"))
            .cloned()
            .unwrap_or_else(|| "preimage-123".to_string());

        let proof = request
            .proof_refs
            .iter()
            .find(|v| v.starts_with("proof-") || v.contains("proof"))
            .cloned()
            .or_else(|| request.proof_refs.first().cloned())
            .unwrap_or_else(|| "proof-123".to_string());

        Ok(LightningSettlementResponse {
            settled_amount: request.amount,
            preimage,
            proof,
        })
    }
}

fn test_app() -> axum::Router {
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
    let identity = Arc::new(conxian_compliance::IdentityManager::new());
    let compliance = Arc::new(ZkcVerifier::new());
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

    configure_routes(state, TEST_TOKEN.to_string(), Instant::now(), None)
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

#[tokio::test]
async fn test_enterprise_workflow_1_swift_cbpr_iso20022_pain001() {
    let app = test_app();

    let gen_req = Request::builder()
        .method("POST")
        .uri("/api/v1/iso20022/pain001")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "amount_sbtc": 1.5,
                "receiver": "bc1qreceiveraddress123456789"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(gen_req).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    if status != 200 { println!("RESPONSE BODY: {}", String::from_utf8_lossy(&body_bytes)); }
    assert_eq!(status, 200);
    let gen_res: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let xml = gen_res["xml"].as_str().unwrap();
    assert!(xml.contains("pain.001.001.08"));
    assert!(xml.contains("CstmrCdtTrfInitn"));

    let mut hasher = sha2::Sha256::new();
    hasher.update(xml.as_bytes());
    let payload_hash = hex::encode(hasher.finalize());
    let device_id = format!("{}test-device", conxian_compliance::zkc::TEE_DEVICE_ID_PREFIX);
    let att_header = make_attestation_header(&device_id, &payload_hash);

    let ingress_req = Request::builder()
        .method("POST")
        .uri("/api/v1/ingress/pain001")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-402-payment", "proof-test")
        .header("x-conxian-attestation", att_header)
        .header("x-conxian-trust-metadata", trust_metadata_json())
        .header("Content-Type", "application/xml")
        .body(Body::from(xml.to_string()))
        .unwrap();

    let ingress_res = app.oneshot(ingress_req).await.unwrap();
    let status = ingress_res.status();
    let res_bytes = ingress_res.into_body().collect().await.unwrap().to_bytes();
    if status != 200 { println!("INGRESS TEST 1 ERROR BODY: {}", String::from_utf8_lossy(&res_bytes)); }
    assert_eq!(status, 200);

    let proposal: serde_json::Value = serde_json::from_slice(&res_bytes).unwrap();
    assert_eq!(
        proposal["envelope"]["payload"]["source"],
        "ISO20022_PAIN001"
    );
}

#[tokio::test]
async fn test_enterprise_workflow_2_retail_pos_event_normalization() {
    let app = test_app();

    let pos_payload = json!({
        "terminal_id": "POS-TERM-8842",
        "merchant_id": "MERCHANT-RETAIL-991",
        "amount_minor": 12500,
        "currency": "USD",
        "payment_method": "TAP_TO_PAY",
        "timestamp": 1776600000u64
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/pos/event")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-conxian-trust-metadata", trust_metadata_json())
        .header("Content-Type", "application/json")
        .body(Body::from(pos_payload.to_string()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    if status != 200 { println!("RESPONSE BODY: {}", String::from_utf8_lossy(&body_bytes)); }
    assert_eq!(status, 200);
    let res: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(res["status"], "SETTLED");
    assert_eq!(res["target_rail"], "L2_STATE_CHANNEL");
    assert_eq!(res["envelope"]["payload"]["amount_minor"], 12500);
}

#[tokio::test]
async fn test_enterprise_workflow_3_logistics_edi_purchase_order() {
    let app = test_app();

    let edi_payload = json!({
        "po_number": "PO-2026-LOG-8810",
        "buyer_id": "BUYER-GLOBAL-LOGISTICS",
        "seller_id": "SELLER-MARITIME-SUPPLY",
        "total_amount": 4500000,
        "currency": "EUR",
        "line_items_count": 12,
        "document_raw": "<EDI_DOCUMENT><HEADER>PO-2026-LOG-8810</HEADER></EDI_DOCUMENT>"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/supply/edi")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-conxian-trust-metadata", trust_metadata_json())
        .header("Content-Type", "application/json")
        .body(Body::from(edi_payload.to_string()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    if status != 200 { println!("RESPONSE BODY: {}", String::from_utf8_lossy(&body_bytes)); }
    assert_eq!(status, 200);
    let res: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(res["status"], "INGESTED");
    assert_eq!(res["merkle_target"], "mmr_nodes");
    assert!(!res["document_hash"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_enterprise_workflow_4_sme_ubl_invoice_escrow_sync() {
    let app = test_app();

    let ubl_payload = json!({
        "invoice_id": "UBL-INV-9901",
        "supplier_id": "SUPPLIER-SME-TECH",
        "customer_id": "CUSTOMER-CORP-INT",
        "issue_date": "2026-08-20",
        "total_amount_minor": 850000,
        "currency": "USD",
        "line_items": [
            {
                "line_id": "LINE-1",
                "item_name": "Server Hardware Provisioning",
                "quantity": 2,
                "unit_price_minor": 300000,
                "total_minor": 600000
            },
            {
                "line_id": "LINE-2",
                "item_name": "SaaS License 1-Year",
                "quantity": 1,
                "unit_price_minor": 250000,
                "total_minor": 250000
            }
        ]
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/invoicing/ubl")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-conxian-trust-metadata", trust_metadata_json())
        .header("Content-Type", "application/json")
        .body(Body::from(ubl_payload.to_string()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    if status != 200 { println!("RESPONSE BODY: {}", String::from_utf8_lossy(&body_bytes)); }
    assert_eq!(status, 200);
    let res: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(res["status"], "PROVISIONAL_ESCROW_LOCKED");
    assert_eq!(res["escrow_target"], "ESCROW_CONTRACT");
    assert_eq!(res["line_items_count"], 2);
}

#[tokio::test]
async fn test_enterprise_workflow_5_compliance_zk_kyc_extraction() {
    let app = test_app();

    let kyc_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
    <PstlAdr>
        <StrtNm>456 Institutional Blvd</StrtNm>
        <BldgNb>Suite 800</BldgNb>
        <PstCd>60311</PstCd>
        <TwnNm>Frankfurt</TwnNm>
        <Ctry>DE</Ctry>
    </PstlAdr>
</Document>"#;

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/compliance/zk-kyc")
        .header("Authorization", format!("Bearer {}", TEST_TOKEN))
        .header("x-conxian-trust-metadata", trust_metadata_json())
        .header("Content-Type", "application/xml")
        .body(Body::from(kyc_xml))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    if status != 200 { println!("RESPONSE BODY: {}", String::from_utf8_lossy(&body_bytes)); }
    assert_eq!(status, 200);
    let res: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(res["status"], "SANITIZED");
    assert_eq!(res["target_verifier"], "ZK_VERIFIER_CONTRACT");
    assert_eq!(res["country"], "DE");
    assert_eq!(res["town_name"], "Frankfurt");
    assert!(!res["zk_commitment"].as_str().unwrap().is_empty());
}
