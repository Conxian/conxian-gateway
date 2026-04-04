use crate::AppState;
use axum::{
    body::Bytes,
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
};
use conxian_core::{
    AttestationRequest, BitVmAttestation, ConxianJobCard, GcpTokenRequest,
    IdentityResolutionRequest, IdentityResolutionResponse, SettlementEnvelope, SettlementProposal,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

use crate::a2p::{OtpRequest, OtpVerificationRequest};
use crate::fiat::{OnRampSessionRequest, OnRampSessionResponse, WebhookPayload};

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

async fn record_settlement(state: &AppState, proposal: &SettlementProposal) {
    let proposal = proposal.clone();
    let mut log = state.settlement_log.write().await;
    log.push_back(proposal);

    while log.len() > SETTLEMENT_LOG_MAX_ENTRIES {
        log.pop_front();
    }
}

pub async fn get_external_settlements(
    State(state): State<AppState>,
) -> Json<Vec<SettlementProposal>> {
    let items = {
        let log = state.settlement_log.read().await;
        log.iter().cloned().collect()
    };

    Json(items)
}

fn extract_tee_attestation(
    headers: &HeaderMap,
) -> Result<AttestationRequest, (StatusCode, Json<Value>)> {
    let value = headers
        .get(TEE_ATTESTATION_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing TEE attestation" })),
        ))?;

    serde_json::from_str::<AttestationRequest>(value).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid TEE attestation JSON: {e}") })),
        )
    })
}

fn verify_tee_settlement_attestation(
    state: &AppState,
    headers: &HeaderMap,
    raw_payload_hash: &str,
) -> Result<AttestationRequest, (StatusCode, Json<Value>)> {
    fn invalid_tee_attestation_response() -> (StatusCode, Json<Value>) {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid TEE attestation" })),
        )
    }

    let tee_attestation = extract_tee_attestation(headers)?;
    match state
        .compliance
        .verify_settlement_trigger_attestation(&tee_attestation, raw_payload_hash)
    {
        Ok(true) => Ok(tee_attestation),
        Ok(false) => {
            warn!("TEE settlement attestation verification failed");
            Err(invalid_tee_attestation_response())
        }
        Err(e) => {
            // We intentionally collapse all verifier errors into a stable client-facing response.
            // Detailed failure reasons are logged but not returned to callers.
            warn!("TEE settlement attestation verification error: {e}");
            Err(invalid_tee_attestation_response())
        }
    }
}

/// Returns the current Stacks burn block height.
///
/// Rejects settlement ingress with `503 SERVICE_UNAVAILABLE` if the burn block height is not yet
/// known. This intentionally fails closed rather than falling back to the chain tip height.
fn get_stacks_burn_block_height(state: &AppState) -> Result<u64, (StatusCode, Json<Value>)> {
    let s = state.shared.read().map_err(|_| {
        warn!("Failed to acquire read lock on shared gateway state");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "Gateway state unavailable" })),
        )
    })?;

    s.stacks.burn_block_height.ok_or_else(|| {
        warn!("Stacks burn block height unavailable; rejecting settlement ingress");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "Stacks burn block height unavailable" })),
        )
    })
}

fn current_unix_timestamp() -> Result<u64, std::time::SystemTimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
}

fn current_unix_timestamp_http() -> Result<u64, (StatusCode, Json<Value>)> {
    current_unix_timestamp().map_err(|e| {
        error!("System clock error: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "System clock error" })),
        )
    })
}

fn build_settlement_proposal(
    state: &AppState,
    envelope: SettlementEnvelope,
    tee_attestation: AttestationRequest,
) -> Result<SettlementProposal, (StatusCode, Json<Value>)> {
    let stacks_burn_block_height = get_stacks_burn_block_height(state)?;
    let now = current_unix_timestamp_http()?;
    SettlementProposal::new(envelope, tee_attestation, stacks_burn_block_height, now).map_err(|e| {
        error!("Failed to create settlement proposal: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to create settlement proposal" })),
        )
    })
}

pub async fn health_check(State(state): State<AppState>) -> Json<Value> {
    let s = state.shared.read().unwrap();
    let mut status = "healthy";
    let mut details = Vec::new();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if s.bitcoin.status.contains("error") {
        status = "degraded";
        details.push(format!("Bitcoin error: {}", s.bitcoin.status));
    } else if s.bitcoin.last_sync_time > 0 && now.saturating_sub(s.bitcoin.last_sync_time) > 120 {
        status = "degraded";
        details.push(format!(
            "Bitcoin sync is stale (last sync: {}s ago)",
            now.saturating_sub(s.bitcoin.last_sync_time)
        ));
    }

    if s.stacks.status.contains("error") {
        status = "degraded";
        details.push(format!("Stacks error: {}", s.stacks.status));
    } else if s.stacks.last_sync_time > 0 && now.saturating_sub(s.stacks.last_sync_time) > 300 {
        status = "degraded";
        details.push(format!(
            "Stacks sync is stale (last sync: {}s ago)",
            now.saturating_sub(s.stacks.last_sync_time)
        ));
    }

    Json(json!({
        "status": status,
        "details": details,
        "version": conxian_core::VERSION,
        "timestamp": now
    }))
}

pub async fn get_state(State(state): State<AppState>) -> Json<GatewayStateResponse> {
    let mut s_write = state.shared.write().unwrap();
    s_write.metrics.total_requests += 1;
    s_write.metrics.state_requests += 1;
    drop(s_write);

    let s = state.shared.read().unwrap();
    Json(GatewayStateResponse {
        bitcoin: s.bitcoin.clone(),
        stacks: s.stacks.clone(),
        metrics: s.metrics.clone(),
    })
}

#[derive(Serialize)]
pub struct GatewayStateResponse {
    pub bitcoin: conxian_core::ChainState,
    pub stacks: conxian_core::ChainState,
    pub metrics: conxian_core::Metrics,
}

pub async fn get_metrics(State(state): State<AppState>) -> String {
    let mut s_write = state.shared.write().unwrap();
    s_write.metrics.total_requests += 1;
    s_write.metrics.metrics_requests += 1;
    drop(s_write);

    let s = state.shared.read().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let uptime = now.saturating_sub(s.start_time);

    format!(
        "# HELP gateway_total_requests The total number of API requests received.\n         # TYPE gateway_total_requests counter\n         gateway_total_requests {}\n         # HELP gateway_health_requests The number of health check requests.\n         # TYPE gateway_health_requests counter\n         gateway_health_requests {}\n         # HELP gateway_state_requests The number of state requests.\n         # TYPE gateway_state_requests counter\n         gateway_state_requests {}\n         # HELP gateway_metrics_requests The number of metrics requests.\n         # TYPE gateway_metrics_requests counter\n         gateway_metrics_requests {}\n         # HELP gateway_verification_requests The total number of attestation verifications attempted.\n         # TYPE gateway_verification_requests counter\n         gateway_verification_requests {}\n         # HELP gateway_verification_success The number of successful attestation verifications.\n         # TYPE gateway_verification_success counter\n         gateway_verification_success {}\n         # HELP gateway_verification_failure The number of failed attestation verifications.\n         # TYPE gateway_verification_failure counter\n         gateway_verification_failure {}\n         # HELP bitcoin_block_height The current block height of the Bitcoin chain.\n         # TYPE bitcoin_block_height gauge\n         bitcoin_block_height {}\n         # HELP stacks_block_height The current block height of the Stacks chain.\n         # TYPE stacks_block_height gauge\n         stacks_block_height {}\n         # HELP bitcoin_last_sync_timestamp The last successful sync timestamp for Bitcoin.\n         # TYPE bitcoin_last_sync_timestamp gauge\n         bitcoin_last_sync_timestamp {}\n         # HELP stacks_last_sync_timestamp The last successful sync timestamp for Stacks.\n         # TYPE stacks_last_sync_timestamp gauge\n         stacks_last_sync_timestamp {}\n         # HELP gateway_uptime_seconds The total uptime of the gateway in seconds.\n         # TYPE gateway_uptime_seconds counter\n         gateway_uptime_seconds {}\n         # HELP treasury_balance_stx Current STX balance in treasury.\n         # TYPE treasury_balance_stx gauge\n         treasury_balance_stx {}\n         # HELP treasury_balance_btc Current BTC balance in treasury.\n         # TYPE treasury_balance_btc gauge\n         treasury_balance_btc {}\n         # HELP sbtc_liquidity Current sBTC liquidity in $ (TAM Capture).\n         # TYPE sbtc_liquidity gauge\n         sbtc_liquidity {}\n         # HELP syi_index Current Sovereign Yield Index value.\n         # TYPE syi_index gauge\n         syi_index {}\n",
        s.metrics.total_requests,
        s.metrics.health_requests,
        s.metrics.state_requests,
        s.metrics.metrics_requests,
        s.metrics.verification_requests,
        s.metrics.verification_success,
        s.metrics.verification_failure,
        s.bitcoin.height,
        s.stacks.height,
        s.bitcoin.last_sync_time,
        s.stacks.last_sync_time,
        uptime,
        s.metrics.treasury_balance_stx,
        s.metrics.treasury_balance_btc,
        s.metrics.sbtc_liquidity,
        s.metrics.syi_index
    )
}

pub async fn verify_attestation(
    State(state): State<AppState>,
    Json(request): Json<AttestationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.shared.write().unwrap();
        s.metrics.total_requests += 1;
        s.metrics.verification_requests += 1;
    }

    let (attestation_type, result) = match request {
        AttestationRequest::Ecdsa(a) => ("ECDSA", state.compliance.verify(&a)),
        AttestationRequest::Schnorr(a) => ("Schnorr", state.compliance.verify_schnorr(&a)),
        AttestationRequest::Zkml(a) => ("ZKML", state.compliance.verify_zkml(&a)),
        AttestationRequest::BitVm(a) => ("BitVM", state.compliance.verify_bitvm(&a)),
    };

    info!(
        "Processing {} attestation verification request",
        attestation_type
    );

    match result {
        Ok(valid) => {
            {
                let mut s = state.shared.write().unwrap();
                if valid {
                    s.metrics.verification_success += 1;
                    info!("{} attestation verified successfully", attestation_type);
                } else {
                    s.metrics.verification_failure += 1;
                    info!(
                        "{} attestation verification failed: invalid signature",
                        attestation_type
                    );
                }
            }
            Ok(Json(json!({ "valid": valid, "type": attestation_type })))
        }
        Err(e) => {
            {
                let mut s = state.shared.write().unwrap();
                s.metrics.verification_failure += 1;
            }
            info!("{} attestation verification error: {}", attestation_type, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string(), "type": attestation_type })),
            ))
        }
    }
}

pub async fn exchange_identity(
    State(state): State<AppState>,
    Json(request): Json<GcpTokenRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.shared.write().unwrap();
        s.metrics.total_requests += 1;
    }

    match state.identity.exchange_token(&request).await {
        Ok(token) => Ok(Json(
            json!({ "access_token": token, "token_type": "Bearer", "expires_in": 3600 }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// CON-66: Resolve identities across ENS, BNS, World ID, and Web3.bio.
pub async fn resolve_identity_v1(
    State(state): State<AppState>,
    Json(request): Json<IdentityResolutionRequest>,
) -> Result<Json<IdentityResolutionResponse>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.shared.write().unwrap();
        s.metrics.total_requests += 1;
    }

    match state.identity.resolve_identity(&request).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn generate_iso_payment(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if let Ok(job_card) = serde_json::from_value::<ConxianJobCard>(payload.clone()) {
        match state.compliance.format_iso20022_pacs008_v8(&job_card) {
            Ok(xml) => return Ok(Json(json!({ "xml": xml, "schema": "pacs.008.001.08" }))),
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": e.to_string(), "code": "ISO-404" })),
                ))
            }
        }
    }

    let sender = payload["sender"].as_str().unwrap_or("CONXIAN-SENDER");
    let receiver = payload["receiver"]
        .as_str()
        .unwrap_or("INSTITUTIONAL-RECEIVER");
    let amount = payload["amount"].as_f64().unwrap_or(0.0);

    let xml = state
        .compliance
        .format_iso20022_pacs008(sender, receiver, amount);
    Ok(Json(json!({ "xml": xml, "schema": "pacs.008.001.07" })))
}

pub async fn create_fiat_session(
    State(state): State<AppState>,
    Json(request): Json<OnRampSessionRequest>,
) -> Result<Json<OnRampSessionResponse>, (StatusCode, Json<Value>)> {
    match state.fiat.create_session(request).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
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

    match state
        .compliance
        .normalize_iso20022_ingress(xml, raw_payload_hash)
    {
        Ok(envelope) => {
            let proposal = build_settlement_proposal(&state, envelope, tee_attestation)?;
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

    let payload: Value = serde_json::from_slice(&bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid JSON body: {e}") })),
        )
    })?;

    match state
        .compliance
        .normalize_papss_ingress(payload.get("payload").unwrap_or(&payload), raw_payload_hash)
    {
        Ok(envelope) => {
            let proposal = build_settlement_proposal(&state, envelope, tee_attestation)?;
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

    let payload: Value = serde_json::from_slice(&bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid JSON body: {e}") })),
        )
    })?;

    match state
        .compliance
        .normalize_brics_ingress(payload.get("payload").unwrap_or(&payload), raw_payload_hash)
    {
        Ok(envelope) => {
            let proposal = build_settlement_proposal(&state, envelope, tee_attestation)?;
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
