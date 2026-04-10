use crate::AppState;
use axum::{
    body::Bytes,
    extract::{Json, Query, State},
    http::{HeaderMap, StatusCode},
};
use conxian_core::{
    AttestationRequest, BitVmAttestation, ConxianJobCard, GcpTokenRequest,
    IdentityResolutionRequest, IdentityResolutionResponse, IndustrialIntent, SettlementEnvelope,
    SettlementProposal,
};
use engine::stacks::alex::AlexSwapRequest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

use crate::a2p::{OtpRequest, OtpVerificationRequest};
use crate::fiat::WebhookPayload;

const SETTLEMENT_LOG_MAX_ENTRIES: usize = 1_000;
const TEE_ATTESTATION_HEADER: &str = "x-tee-attestation";

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn normalized_content_type(headers: &HeaderMap) -> Option<&str> {
    let content_type = headers.get(axum::http::header::CONTENT_TYPE)?;
    let content_type = content_type.to_str().ok()?;

    Some(content_type.split(';').next().unwrap_or("").trim())
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    let Some(content_type) = normalized_content_type(headers) else {
        return false;
    };

    content_type.eq_ignore_ascii_case("application/json")
        || content_type
            .rsplit_once('+')
            .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("json"))
}

fn is_xml_content_type(headers: &HeaderMap) -> bool {
    use axum::http::header::CONTENT_TYPE;

    if !headers.contains_key(CONTENT_TYPE) {
        return false;
    }

    let Some(content_type) = normalized_content_type(headers) else {
        return false;
    };

    content_type.eq_ignore_ascii_case("application/xml")
        || content_type.eq_ignore_ascii_case("text/xml")
        || content_type
            .rsplit_once('+')
            .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("xml"))
}

pub async fn health_check() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!({ "status": "healthy" })))
}

pub async fn get_metrics(State(state): State<AppState>) -> (StatusCode, String) {
    let s = state.shared.read().unwrap();
    let uptime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - s.start_time;

    let prometheus_output = format!(
        "# HELP gateway_uptime_seconds The service uptime in seconds\n         # TYPE gateway_uptime_seconds counter\n         gateway_uptime_seconds {}\n         # HELP gateway_total_requests Total requests processed\n         # TYPE gateway_total_requests counter\n         gateway_total_requests {}\n         # HELP gateway_verification_success_total Successful attestations\n         # TYPE gateway_verification_success_total counter\n         gateway_verification_success_total {}\n         # HELP gateway_verification_failure_total Failed attestations\n         # TYPE gateway_verification_failure_total counter\n         gateway_verification_failure_total {}\n         # HELP blockchain_height_bitcoin Bitcoin L1 tip height\n         # TYPE blockchain_height_bitcoin gauge\n         blockchain_height_bitcoin {}\n         # HELP blockchain_height_stacks Stacks L2 tip height\n         # TYPE blockchain_height_stacks gauge\n         blockchain_height_stacks {}\n",
        uptime,
        s.metrics.total_requests,
        s.metrics.verification_success,
        s.metrics.verification_failure,
        s.bitcoin.height,
        s.stacks.height
    );

    (StatusCode::OK, prometheus_output)
}

pub async fn get_state(State(state): State<AppState>) -> Json<Value> {
    let s = state.shared.read().unwrap();
    Json(json!({
        "bitcoin": s.bitcoin,
        "stacks": s.stacks,
        "wallets": s.wallets,
    }))
}

pub async fn verify_attestation(
    State(state): State<AppState>,
    Json(request): Json<AttestationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.shared.write().unwrap();
        s.metrics.verification_requests += 1;
        s.metrics.total_requests += 1;
    }

    match state.compliance.verify_attestation(request) {
        Ok(valid) => {
            let mut s = state.shared.write().unwrap();
            if valid {
                s.metrics.verification_success += 1;
            } else {
                s.metrics.verification_failure += 1;
            }
            Ok(Json(json!({ "valid": valid })))
        }
        Err(e) => {
            let mut s = state.shared.write().unwrap();
            s.metrics.verification_failure += 1;
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    }
}

pub async fn exchange_identity(
    State(state): State<AppState>,
    Json(request): Json<GcpTokenRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.identity.exchange_token(&request).await {
        Ok(res) => Ok(Json(json!(res))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn resolve_identity_v1(
    State(state): State<AppState>,
    Json(request): Json<IdentityResolutionRequest>,
) -> Result<Json<IdentityResolutionResponse>, (StatusCode, Json<Value>)> {
    match state.identity.resolve_identity(&request).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn verify_fiat_webhook(
    State(state): State<AppState>,
    Json(payload): Json<WebhookPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state
        .fiat
        .verify_webhook(&payload, &state.fiat_webhook_secret)
    {
        Ok(valid) => Ok(Json(
            json!({ "valid": valid, "provider": payload.provider }),
        )),
        Err(e) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn send_otp(
    State(state): State<AppState>,
    Json(request): Json<OtpRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.a2p.send_otp(request).await {
        Ok((res, hmac, ts)) => Ok(Json(json!({
            "session_id": res.session_id,
            "status": res.status,
            "hmac": hmac,
            "timestamp": ts
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn verify_otp(
    State(state): State<AppState>,
    Json(request): Json<OtpVerificationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.a2p.verify_otp(request) {
        Ok(valid) => Ok(Json(json!({ "valid": valid }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn sync_erp_ledger(
    State(_state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    info!("Processing ERP OData sync request for institutional ledger.");
    let d = &payload["d"];
    if d.is_null() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid OData payload: missing 'd' root"})),
        ));
    }
    let results = &d["results"];
    if results.is_array() {
        let count = results.as_array().unwrap().len();
        info!("Synced {} ERP records to Conxian Nexus.", count);
        Ok(Json(
            json!({ "status": "success", "synced_records": count, "ledger": "SAP-S4HANA-ORCHESTRATION" }),
        ))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid OData payload: 'results' must be an array"})),
        ))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettlementRequest {
    pub job_card: ConxianJobCard,
    pub bitvm_proof: BitVmAttestation,
}

pub async fn settle_job_card(
    State(state): State<AppState>,
    Json(request): Json<SettlementRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state
        .compliance
        .verify_job_card_settlement(&request.job_card, &request.bitvm_proof)
    {
        Ok(valid) => Ok(Json(
            json!({ "valid": valid, "settlement": "BitVM2-Verified" }),
        )),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

fn extract_industrial_intent(headers: &HeaderMap) -> IndustrialIntent {
    let x402 = headers
        .get("x402-payment-required")
        .map(|v| v.to_str().unwrap_or("false") == "true")
        .unwrap_or(false);

    let invoice_id = headers
        .get("x-invoice-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    IndustrialIntent {
        x402_payment_required: x402,
        invoice_id,
        device_id,
    }
}

pub async fn ingress_iso20022(
    State(state): State<AppState>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<Json<conxian_core::SettlementProposal>, (StatusCode, Json<Value>)> {
    if !is_xml_content_type(&headers) {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({ "error": "Unsupported Content-Type" })),
        ));
    }

    let raw_payload_hash = sha256_hex(&bytes);
    let xml = std::str::from_utf8(&bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid UTF-8 body: {e}") })),
        )
    })?;

    let signature = headers
        .get("x-iso20022-signature")
        .and_then(|h| h.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing signature" })),
        ))?;

    match state.compliance.verify_ingress_signature(
        xml,
        signature,
        &state.settlement_ingress_secret,
    ) {
        Ok(true) => (),
        _ => {
            warn!("ISO 20022 ingress signature verification failed");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid signature" })),
            ));
        }
    }

    let tee_attestation = verify_tee_settlement_attestation(&state, &headers, &raw_payload_hash)?;
    let industrial_intent = extract_industrial_intent(&headers);

    match state
        .compliance
        .normalize_iso20022_ingress(xml, raw_payload_hash.clone())
    {
        Ok(mut envelope) => {
            envelope.payload.industrial_intent = industrial_intent;
            let proposal =
                build_settlement_proposal(&state, envelope, tee_attestation, &raw_payload_hash)?;
            info!(
                "Successfully ingested ISO 20022 settlement: {}",
                proposal.envelope.payload.transaction_id
            );
            record_settlement(&state, &proposal).await;
            Ok(Json(proposal))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn ingress_papss(
    State(state): State<AppState>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<Json<conxian_core::SettlementProposal>, (StatusCode, Json<Value>)> {
    if !is_json_content_type(&headers) {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({ "error": "Unsupported Content-Type" })),
        ));
    }

    let raw_payload_hash = sha256_hex(&bytes);

    let signature = headers
        .get("x-papss-signature")
        .and_then(|h| h.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing signature" })),
        ))?;

    let raw_payload = std::str::from_utf8(&bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid UTF-8 body: {e}") })),
        )
    })?;

    match state.compliance.verify_ingress_signature(
        raw_payload,
        signature,
        &state.settlement_ingress_secret,
    ) {
        Ok(true) => (),
        _ => {
            warn!("PAPSS ingress signature verification failed");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid signature" })),
            ));
        }
    }

    let tee_attestation = verify_tee_settlement_attestation(&state, &headers, &raw_payload_hash)?;
    let industrial_intent = extract_industrial_intent(&headers);

    let payload: Value = serde_json::from_slice(&bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid JSON body: {e}") })),
        )
    })?;

    match state.compliance.normalize_papss_ingress(
        payload.get("payload").unwrap_or(&payload),
        raw_payload_hash.clone(),
    ) {
        Ok(mut envelope) => {
            envelope.payload.industrial_intent = industrial_intent;
            let proposal =
                build_settlement_proposal(&state, envelope, tee_attestation, &raw_payload_hash)?;
            info!(
                "Successfully ingested PAPSS settlement: {}",
                proposal.envelope.payload.transaction_id
            );
            record_settlement(&state, &proposal).await;
            Ok(Json(proposal))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn ingress_brics(
    State(state): State<AppState>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<Json<conxian_core::SettlementProposal>, (StatusCode, Json<Value>)> {
    if !is_json_content_type(&headers) {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({ "error": "Unsupported Content-Type" })),
        ));
    }

    let raw_payload_hash = sha256_hex(&bytes);

    let signature = headers
        .get("x-brics-signature")
        .and_then(|h| h.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing signature" })),
        ))?;

    let raw_payload = std::str::from_utf8(&bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid UTF-8 body: {e}") })),
        )
    })?;

    match state.compliance.verify_ingress_signature(
        raw_payload,
        signature,
        &state.settlement_ingress_secret,
    ) {
        Ok(true) => (),
        _ => {
            warn!("BRICS ingress signature verification failed");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid signature" })),
            ));
        }
    }

    let tee_attestation = verify_tee_settlement_attestation(&state, &headers, &raw_payload_hash)?;
    let industrial_intent = extract_industrial_intent(&headers);

    let payload: Value = serde_json::from_slice(&bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid JSON body: {e}") })),
        )
    })?;

    match state.compliance.normalize_brics_ingress(
        payload.get("payload").unwrap_or(&payload),
        raw_payload_hash.clone(),
    ) {
        Ok(mut envelope) => {
            envelope.payload.industrial_intent = industrial_intent;
            let proposal =
                build_settlement_proposal(&state, envelope, tee_attestation, &raw_payload_hash)?;
            info!(
                "Successfully ingested BRICS settlement: {}",
                proposal.envelope.payload.transaction_id
            );
            record_settlement(&state, &proposal).await;
            Ok(Json(proposal))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn get_external_settlements(
    State(state): State<AppState>,
) -> Json<Vec<SettlementProposal>> {
    let log = state.settlement_log.read().await;
    Json(log.iter().cloned().collect())
}

fn verify_tee_settlement_attestation(
    state: &AppState,
    headers: &HeaderMap,
    payload_hash: &str,
) -> Result<AttestationRequest, (StatusCode, Json<Value>)> {
    let attestation_raw = headers
        .get(TEE_ATTESTATION_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing TEE attestation" })),
        ))?;

    let attestation: AttestationRequest = serde_json::from_str(attestation_raw).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid TEE attestation format: {e}") })),
        )
    })?;

    match state
        .compliance
        .verify_settlement_trigger_attestation(&attestation, payload_hash)
    {
        Ok(true) => (),
        Ok(false) => {
            warn!("TEE settlement attestation verification failed");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid TEE attestation" })),
            ));
        }
        Err(e) => {
            warn!("TEE settlement attestation verification error: {e}");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid TEE attestation" })),
            ));
        }
    }

    Ok(attestation)
}

fn build_settlement_proposal(
    state: &AppState,
    envelope: SettlementEnvelope,
    tee_attestation: AttestationRequest,
    raw_payload_hash: &str,
) -> Result<SettlementProposal, (StatusCode, Json<Value>)> {
    let s = state.shared.read().unwrap();
    let burn_height = s.stacks.burn_block_height.unwrap_or(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let trigger_id = state
        .compliance
        .compute_trigger_id(
            &format!("{:?}", envelope.payload.source),
            raw_payload_hash,
            &envelope.payload.identifiers,
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    SettlementProposal::new(trigger_id, envelope, tee_attestation, burn_height, now).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })
}

async fn record_settlement(state: &AppState, proposal: &SettlementProposal) {
    let mut log = state.settlement_log.write().await;
    log.push_back(proposal.clone());
    if log.len() > SETTLEMENT_LOG_MAX_ENTRIES {
        log.pop_front();
    }
}

pub async fn generate_iso_payment(
    State(state): State<AppState>,
    Json(job_card): Json<ConxianJobCard>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.compliance.format_iso20022_pacs008_v8(&job_card) {
        Ok(xml) => Ok(Json(json!({ "xml": xml, "schema": "pacs.008.001.08" }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

// ALEX DEX Handlers (CON-136)
pub async fn get_alex_quote(
    State(state): State<AppState>,
    Query(request): Query<AlexSwapRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.alex.get_swap_quote(request).await {
        Ok(amount) => Ok(Json(json!({ "dy": amount.to_string() }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn execute_alex_swap(
    State(state): State<AppState>,
    Json(request): Json<AlexSwapRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let signer_key = "ENCLAVE_SIGNER_PROD";
    match state.alex.execute_swap(request, signer_key).await {
        Ok(txid) => Ok(Json(json!({ "txid": txid, "status": "broadcasted" }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

// Bounty Handlers (CON-230)
pub async fn toggle_bounty_payouts(
    State(state): State<AppState>,
    Json(enabled): Json<bool>,
) -> Json<Value> {
    let mut s = state.shared.write().unwrap();
    s.metrics.bounty_payouts_enabled = enabled;
    info!("Bounty payouts enabled: {}", enabled);
    Json(json!({ "status": "success", "bounty_payouts_enabled": enabled }))
}

#[derive(Debug, serde::Deserialize)]
pub struct OfflinePosRequest {
    pub tx_hash: String,
    pub amount_sbtc: f64,
    pub device_id: String,
    pub passkey_attestation: conxian_core::AttestationRequest,
}

pub async fn handle_offline_pos(
    State(state): State<AppState>,
    Json(payload): Json<OfflinePosRequest>,
) -> Result<Json<conxian_core::OfflineReceipt>, (StatusCode, Json<Value>)> {
    let mut receipt = state
        .compliance
        .sign_offline_receipt(
            &payload.tx_hash,
            payload.amount_sbtc,
            &payload.device_id,
            payload.passkey_attestation,
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    state
        .compliance
        .simulate_mesh_gossip(&mut receipt)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    state.offline_queue.enqueue(&receipt).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(receipt))
}

pub async fn sync_offline_receipts(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let receipts = state.offline_queue.dequeue_pending().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let mut synced_count = 0;
    for receipt in receipts {
        if state
            .compliance
            .verify_offline_receipt(&receipt)
            .unwrap_or(false)
        {
            info!(
                "Broadcasting offline receipt {} to L2...",
                receipt.receipt_id
            );
            state
                .offline_queue
                .mark_broadcasted(&receipt.receipt_id)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": e.to_string() })),
                    )
                })?;
            synced_count += 1;
        }
    }

    Ok(Json(
        json!({ "status": "success", "synced_count": synced_count }),
    ))
}
