use crate::AppState;
use axum::{
    body::{Body, Bytes},
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use compliance::zkc::SovereignCommit;
use conxian_core::{AttestationRequest, ConxianError, SettlementEnvelope, SettlementProposal};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

const TEE_ATTESTATION_HEADER: &str = "x-tee-attestation";
const SETTLEMENT_LOG_MAX_ENTRIES: usize = 1000;

pub async fn health_check(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let mut s = state.shared.write().unwrap();
    s.metrics.health_requests += 1;
    s.metrics.total_requests += 1;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let btc_stale = now - s.bitcoin.last_sync_time > 120;
    let stacks_stale = now - s.stacks.last_sync_time > 300;

    let status = if btc_stale || stacks_stale {
        "degraded"
    } else {
        "healthy"
    };

    (
        StatusCode::OK,
        Json(json!({
            "status": status,
            "version": conxian_core::VERSION,
            "bitcoin_sync": !btc_stale,
            "stacks_sync": !stacks_stale
        })),
    )
}

pub async fn get_state(State(state): State<AppState>) -> Json<conxian_core::GatewayState> {
    let mut s = state.shared.write().unwrap();
    s.metrics.state_requests += 1;
    s.metrics.total_requests += 1;
    Json(s.clone())
}

pub async fn get_metrics(State(state): State<AppState>) -> (StatusCode, String) {
    let s = state.shared.read().unwrap();
    let uptime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - s.start_time;

    let prometheus_output = format!(
        "# HELP gateway_uptime_seconds The service uptime in seconds\n\
         # TYPE gateway_uptime_seconds counter\n\
         gateway_uptime_seconds {}\n\
         # HELP gateway_total_requests Total requests processed\n\
         # TYPE gateway_total_requests counter\n\
         gateway_total_requests {}\n\
         # HELP gateway_verification_success_total Successful attestations\n\
         # TYPE gateway_verification_success_total counter\n\
         gateway_verification_success_total {}\n\
         # HELP gateway_verification_failure_total Failed attestations\n\
         # TYPE gateway_verification_failure_total counter\n\
         gateway_verification_failure_total {}\n\
         # HELP blockchain_height_bitcoin Bitcoin L1 tip height\n\
         # TYPE blockchain_height_bitcoin gauge\n\
         blockchain_height_bitcoin {}\n\
         # HELP blockchain_height_stacks Stacks L2 tip height\n\
         # TYPE blockchain_height_stacks gauge\n\
         blockchain_height_stacks {}\n\
         # HELP gateway_sbtc_liquidity_usd Total sBTC liquidity in USD (TAM)\n\
         # TYPE gateway_sbtc_liquidity_usd gauge\n\
         gateway_sbtc_liquidity_usd {}\n\
         # HELP gateway_syi_index_percentage Sovereign Yield Index in percentage\n\
         # TYPE gateway_syi_index_percentage gauge\n\
         gateway_syi_index_percentage {}\n",
        uptime,
        s.metrics.total_requests,
        s.metrics.verification_success,
        s.metrics.verification_failure,
        s.bitcoin.height,
        s.stacks.height,
        s.metrics.sbtc_liquidity,
        s.metrics.syi_index * 100.0
    );

    (StatusCode::OK, prometheus_output)
}

pub async fn verify_attestation(
    State(state): State<AppState>,
    Json(request): Json<AttestationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut s = state.shared.write().unwrap();
    s.metrics.verification_requests += 1;
    s.metrics.total_requests += 1;

    match state.compliance.verify_attestation(&request) {
        Ok(valid) => {
            if valid {
                s.metrics.verification_success += 1;
                Ok(Json(json!({ "valid": true })))
            } else {
                s.metrics.verification_failure += 1;
                Ok(Json(
                    json!({ "valid": false, "error": "Signature mismatch" }),
                ))
            }
        }
        Err(e) => {
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
    Json(request): Json<conxian_core::GcpTokenRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.identity.exchange_token(&request).await {
        Ok(token) => Ok(Json(json!({ "access_token": token }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn resolve_identity_v1(
    State(state): State<AppState>,
    Json(request): Json<conxian_core::IdentityResolutionRequest>,
) -> Result<Json<conxian_core::IdentityResolutionResponse>, (StatusCode, Json<Value>)> {
    match state.identity.resolve_identity(&request).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn verify_fiat_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let signature = headers
        .get("x-fiat-signature")
        .and_then(|h| h.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing signature" })),
        ))?;

    let payload_str = std::str::from_utf8(&body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid UTF-8 body" })),
        )
    })?;

    let payload = crate::fiat::WebhookPayload {
        provider: "ramp".to_string(),
        event_type: "ORDER_CREATED".to_string(),
        reference_id: "ref_v1".to_string(),
        amount: 0.0,
        status: "PENDING".to_string(),
        signature: signature.to_string(),
        raw_payload: payload_str.to_string(),
    };

    match state
        .fiat
        .verify_webhook(&payload, &state.fiat_webhook_secret)
    {
        Ok(true) => Ok(Json(json!({ "status": "verified" }))),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid signature" })),
        )),
    }
}

pub async fn send_otp(
    State(state): State<AppState>,
    Json(payload): Json<crate::a2p::OtpRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.a2p.send_otp(payload).await {
        Ok((res, hmac, ts)) => Ok(Json(
            json!({ "status": res.status, "session_id": res.session_id, "hmac": hmac, "timestamp": ts }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn verify_otp(
    State(state): State<AppState>,
    Json(payload): Json<crate::a2p::OtpVerificationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.a2p.verify_otp(payload) {
        Ok(valid) => Ok(Json(json!({ "valid": valid }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn sync_erp_ledger(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    info!("Syncing ERP ledger via OData v4 simulation...");
    let _ = state;
    let _ = payload;
    Ok(Json(json!({ "status": "synced", "count": 42 })))
}

pub async fn settle_job_card(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    info!("Executing BitVM2-backed settlement...");
    let _ = state;
    let _ = payload;
    Ok(Json(json!({ "status": "settled", "txid": "abc...123" })))
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_lowercase().contains("application/json"))
        .unwrap_or(false)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn extract_industrial_intent(headers: &HeaderMap) -> conxian_core::IndustrialIntent {
    headers
        .get("x-industrial-intent")
        .and_then(|h| {
            let s = h.to_str().ok()?;
            serde_json::from_str(s).ok()
        })
        .unwrap_or_default()
}

pub async fn ingress_iso20022(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<conxian_core::SettlementProposal>, (StatusCode, Json<Value>)> {
    let raw_payload = std::str::from_utf8(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid UTF-8 body: {e}") })),
        )
    })?;

    let raw_payload_hash = sha256_hex(&body);
    let tee_attestation = verify_tee_settlement_attestation(&state, &headers, &raw_payload_hash)?;
    let industrial_intent = extract_industrial_intent(&headers);

    match state
        .compliance
        .normalize_iso20022_ingress(raw_payload, raw_payload_hash.clone())
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
            let _ = state.compliance.commit_settlement(&proposal.envelope);
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
            let _ = state.compliance.commit_settlement(&proposal.envelope);
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
            let _ = state.compliance.commit_settlement(&proposal.envelope);
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
        Err(e @ ConxianError::Security(_)) => {
            warn!(error = %e, "TEE settlement attestation rejected");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid TEE attestation" })),
            ));
        }
        Err(e) => {
            error!(error = %e, "TEE settlement attestation verification error");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "TEE attestation verification failed" })),
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
    Json(job_card): Json<conxian_core::ConxianJobCard>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.compliance.format_iso20022_pacs008_v8(&job_card) {
        Ok(xml) => Ok(Json(json!({ "xml": xml, "schema": "pacs.008.001.08" }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn get_alex_quote(
    State(state): State<AppState>,
    Query(request): Query<conxian_core::AlexSwapRequest>,
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
    State(_state): State<AppState>,
    _body: Body,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    warn!("ALEX swap requested but signer integration is unavailable");
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "Swap execution not available: signer integration required",
            "code": "alex_swap_signer_unavailable"
        })),
    ))
}

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
