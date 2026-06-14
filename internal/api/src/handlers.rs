use crate::AppState;
use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use compliance::SovereignCommit;
use conxian_core::{
    evaluate_trust_metadata_json, AttestationRequest, ConxianError, JobCardSettlementRequest,
    SettlementEnvelope, SettlementProposal, TrustPolicyDecision, TrustPolicyReasonCode,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

pub const TEE_ATTESTATION_HEADER: &str = "x-tee-attestation";
pub const TRUST_METADATA_HEADER: &str = "x-conxian-trust-metadata";
const SETTLEMENT_LOG_MAX_ENTRIES: usize = 1000;
const WEBHOOK_REPLAY_TTL_SECONDS: u64 = 60 * 60 * 24;

pub async fn get_health(State(state): State<AppState>) -> Json<Value> {
    let s = state.shared.read().unwrap();
    let bitcoin_status = if s.bitcoin.last_sync_time > 0 {
        "synced"
    } else {
        "syncing"
    };
    let stacks_status = if s.stacks.last_sync_time > 0 {
        "synced"
    } else {
        "syncing"
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock moved backwards")
        .as_secs();

    let mut overall = "ok";
    if s.bitcoin.last_sync_time > 0 && now - s.bitcoin.last_sync_time > 120 {
        overall = "degraded";
    }
    if s.stacks.last_sync_time > 0 && now - s.stacks.last_sync_time > 300 {
        overall = "degraded";
    }

    Json(json!({
        "status": overall,
        "version": env!("CARGO_PKG_VERSION"),
        "bitcoin": {
            "status": bitcoin_status,
            "height": s.bitcoin.height,
        },
        "stacks": {
            "status": stacks_status,
            "height": s.stacks.height,
            "epoch": s.stacks.epoch,
        }
    }))
}

pub async fn get_state(State(state): State<AppState>) -> Json<conxian_core::GatewayState> {
    let s = state.shared.read().unwrap();
    Json(s.clone())
}

pub async fn get_metrics(State(state): State<AppState>) -> Json<Value> {
    let s = state.shared.read().unwrap();
    Json(json!({
        "health_requests": s.metrics.health_requests,
        "state_requests": s.metrics.state_requests,
        "metrics_requests": s.metrics.metrics_requests,
        "verification_requests": s.metrics.verification_requests,
        "verification_success": s.metrics.verification_success,
        "verification_failure": s.metrics.verification_failure,
        "total_requests": s.metrics.total_requests,
        "trust_policy_allow": s.metrics.trust_policy_allow,
        "trust_policy_block": s.metrics.trust_policy_block,
        "treasury": {
            "balance_stx": s.metrics.treasury_balance_stx,
            "balance_btc": s.metrics.treasury_balance_btc,
            "last_update": s.metrics.last_treasury_update,
            "sbtc_liquidity": s.metrics.sbtc_liquidity,
            "syi_index": s.metrics.syi_index,
        },
        "bounty_payouts_enabled": s.metrics.bounty_payouts_enabled
    }))
}

pub async fn create_fiat_session(
    State(state): State<AppState>,
    Json(payload): Json<crate::fiat::OnRampSessionRequest>,
) -> Result<Json<crate::fiat::OnRampSessionResponse>, (StatusCode, Json<Value>)> {
    info!(
        "Creating fiat on-ramp session for provider: {}",
        payload.provider
    );

    match state.fiat.create_session(payload).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn verify_fiat_webhook(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(mut payload): Json<crate::fiat::WebhookPayload>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let signature_header = headers
        .get("x-ramp-signature")
        .or_else(|| headers.get("x-fiat-signature"))
        .and_then(|h| h.to_str().ok());

    if let Some(sig) = signature_header {
        payload.signature = sig.to_string();
    }

    match state
        .fiat
        .verify_webhook(&payload, &state.fiat_webhook_secret)
    {
        Ok(true) => {
            let replay_key = compute_webhook_replay_key(&payload);
            match state
                .offline_queue
                .claim_replay_key(&replay_key, WEBHOOK_REPLAY_TTL_SECONDS)
            {
                Ok(true) => Ok(StatusCode::OK),
                Ok(false) => {
                    warn!(
                        provider = %payload.provider,
                        reference_id = %payload.reference_id,
                        "Rejected duplicate webhook delivery"
                    );
                    Err((
                        StatusCode::CONFLICT,
                        Json(json!({
                            "error": "Duplicate webhook delivery rejected (replay detected)",
                            "code": "WEBHOOK_REPLAY_DETECTED"
                        })),
                    ))
                }
                Err(e) => {
                    error!(
                        error = %e,
                        provider = %payload.provider,
                        reference_id = %payload.reference_id,
                        "Failed to persist webhook replay claim"
                    );
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Unable to persist webhook replay claim" })),
                    ))
                }
            }
        }
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
        Ok((res, _hmac, _expiry)) => Ok(Json(json!(res))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn verify_otp(
    State(state): State<AppState>,
    Json(payload): Json<crate::a2p::OtpVerificationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.a2p.verify_otp(payload) {
        Ok(true) => Ok(Json(json!({ "status": "verified" }))),
        Ok(false) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid OTP or HMAC" })),
        )),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn ingress_iso20022(
    headers: HeaderMap,
    State(state): State<AppState>,
    body: Body,
) -> Result<Json<SettlementProposal>, (StatusCode, Json<Value>)> {
    enforce_ingress_trust_policy(&state, &headers)?;

    let raw_payload = body
        .collect()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Body collection failed: {}", e) })),
            )
        })?
        .to_bytes();

    let raw_payload_hash = sha256_hex(&raw_payload);
    let tee_attestation = verify_tee_settlement_attestation(&state, &headers, &raw_payload_hash)?;
    let industrial_intent = extract_industrial_intent(&headers);

    let xml_str = std::str::from_utf8(&raw_payload).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid UTF-8 in XML" })),
        )
    })?;

    match state
        .compliance
        .normalize_iso20022_ingress(xml_str, raw_payload_hash.clone())
    {
        Ok(mut envelope) => {
            if let Some(intent) = industrial_intent {
                envelope.payload.industrial_intent = intent;
            }
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
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn sync_erp_ledger(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    info!("Syncing ERP ledger via OData v4...");

    let payload_bytes = serde_json::to_vec(&payload).map_err(|e: serde_json::Error| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Serialization error: {}", e) })),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&payload_bytes);
    let raw_payload_hash = hex::encode(hasher.finalize());

    let envelopes = state
        .compliance
        .normalize_erp_ingress(&payload, raw_payload_hash)
        .map_err(|e: ConxianError| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
            )
        })?;

    let count = envelopes.len();
    Ok(Json(json!({ "status": "synced", "count": count })))
}

pub async fn settle_job_card(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    info!("Executing BitVM2-backed settlement...");

    let request: JobCardSettlementRequest = serde_json::from_value(payload).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid settlement request: {}", e) })),
        )
    })?;

    match state.compliance.verify_bitvm2_settlement(&request) {
        Ok(true) => {
            info!(
                "BitVM2 settlement verified for job: {}",
                request.bitvm_attestation.commitment_hash
            );
            Ok(Json(json!({
                "status": "settled",
                "verified": true,
                "txid": format!("bitvm-{}", request.bitvm_attestation.proof_hash)
            })))
        }
        Ok(false) => {
            warn!("BitVM2 settlement verification failed");
            Ok(Json(json!({
                "status": "failed",
                "verified": false,
                "error": "Commitment or proof mismatch"
            })))
        }
        Err(e) => {
            error!(error = %e, "BitVM2 settlement error");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
            ))
        }
    }
}

pub async fn get_external_settlements(
    State(state): State<AppState>,
) -> Json<Vec<SettlementProposal>> {
    let log = state.settlement_log.read().await;
    let list: Vec<SettlementProposal> = log.iter().cloned().collect();
    Json(list)
}

fn enforce_ingress_trust_policy(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<Value>)> {
    let decision = evaluate_trust_policy_from_headers(headers);
    record_trust_policy_metric(state, decision);

    match decision {
        TrustPolicyDecision::Allow => Ok(()),
        TrustPolicyDecision::Block(reason) => {
            warn!(
                code = reason.as_str(),
                "Ingress request rejected by trust-tier policy"
            );
            Err(trust_policy_error_response(reason))
        }
    }
}

fn evaluate_trust_policy_from_headers(headers: &HeaderMap) -> TrustPolicyDecision {
    let now_epoch_secs = unix_epoch_secs();
    let raw_metadata = match headers.get(TRUST_METADATA_HEADER) {
        Some(value) => match value.to_str() {
            Ok(raw) => Some(raw),
            Err(_) => {
                return TrustPolicyDecision::Block(TrustPolicyReasonCode::MetadataInvalid);
            }
        },
        None => None,
    };

    evaluate_trust_metadata_json(raw_metadata, now_epoch_secs)
}

fn record_trust_policy_metric(state: &AppState, decision: TrustPolicyDecision) {
    if let Ok(mut s) = state.shared.write() {
        match decision {
            TrustPolicyDecision::Allow => s.metrics.trust_policy_allow += 1,
            TrustPolicyDecision::Block(_) => s.metrics.trust_policy_block += 1,
        }
    }
}

fn trust_policy_error_response(reason: TrustPolicyReasonCode) -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "code": reason.as_str(),
            "message": trust_policy_message(reason),
        })),
    )
}

fn trust_policy_message(reason: TrustPolicyReasonCode) -> &'static str {
    match reason {
        TrustPolicyReasonCode::MetadataMissing => {
            "Missing required x-conxian-trust-metadata header"
        }
        TrustPolicyReasonCode::MetadataInvalid => "Invalid x-conxian-trust-metadata header payload",
        TrustPolicyReasonCode::MetadataStale => "Trust metadata is stale or expired",
        TrustPolicyReasonCode::PolicyBlocked => {
            "Trust tier policy blocks this bridge system and tier combination"
        }
    }
}

fn unix_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock moved backwards")
        .as_secs()
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

    let attestation: AttestationRequest =
        serde_json::from_str(attestation_raw).map_err(|e: serde_json::Error| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid TEE attestation format: {e}") })),
            )
        })?;

    match state
        .compliance
        .verify_settlement_trigger_attestation(&attestation, payload_hash)
    {
        Ok(_) => (),
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
        .expect("system clock moved backwards")
        .as_secs();

    let trigger_id = state
        .compliance
        .compute_trigger_id(
            &format!("{:?}", envelope.payload.source),
            raw_payload_hash,
            &envelope.payload.identifiers,
        )
        .map_err(|e: ConxianError| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
            )
        })?;

    SettlementProposal::new(trigger_id, envelope, tee_attestation, burn_height, now).map_err(
        |e: ConxianError| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
            )
        },
    )
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
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
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
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn execute_alex_swap(
    State(state): State<AppState>,
    Json(payload): Json<conxian_core::AlexSwapRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    info!("Building ALEX swap payload for preparation...");

    match state.alex.build_swap_payload(payload).await {
        Ok(preparation) => {
            warn!("ALEX swap execution paused: signer-enclave integration required for broadcast");
            Ok(Json(json!({
                "status": "prepared",
                "preparation": preparation,
                "message": "Payload built successfully. Signer integration required for final execution."
            })))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
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
        .map_err(|e: ConxianError| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
            )
        })?;

    state
        .compliance
        .gossip_mesh_rehearsal(&mut receipt)
        .map_err(|e: ConxianError| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
            )
        })?;

    state
        .offline_queue
        .enqueue(&receipt)
        .map_err(|e: ConxianError| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
            )
        })?;

    Ok(Json(receipt))
}

pub async fn sync_offline_receipts(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let receipts = state
        .offline_queue
        .dequeue_pending()
        .map_err(|e: ConxianError| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
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
                .map_err(|e: ConxianError| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
                    )
                })?;
            synced_count += 1;
        }
    }

    Ok(Json(
        json!({ "status": "success", "synced_count": synced_count }),
    ))
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn compute_webhook_replay_key(payload: &crate::fiat::WebhookPayload) -> String {
    let payload_hash = sha256_hex(payload.raw_payload.as_bytes());
    let provider = payload.provider.to_ascii_lowercase();

    let mut hasher = Sha256::new();
    hasher.update(provider.as_bytes());
    hasher.update([0u8]);
    hasher.update(payload.signature.as_bytes());
    hasher.update([0u8]);
    hasher.update(payload_hash.as_bytes());

    format!("fiat-webhook:{}", hex::encode(hasher.finalize()))
}

fn extract_industrial_intent(headers: &HeaderMap) -> Option<conxian_core::IndustrialIntent> {
    headers
        .get("x-industrial-intent")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| serde_json::from_str(v).ok())
}

pub async fn exchange_identity(
    State(state): State<AppState>,
    Json(payload): Json<conxian_core::GcpTokenRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.identity.exchange_token(&payload).await {
        Ok(token) => Ok(Json(json!({ "access_token": token }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn verify_attestation(
    State(state): State<AppState>,
    Json(payload): Json<AttestationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let result = match &payload {
        AttestationRequest::Ecdsa(a) => state
            .compliance
            .verify_settlement_trigger_attestation(&payload, &a.payload),
        AttestationRequest::Zkml(p) => state
            .compliance
            .verify_settlement_trigger_attestation(&payload, &p.receipt_hash),
        AttestationRequest::BitVm(_b) => {
            return Ok(Json(json!({
                "status": "partial",
                "message": "BitVM attestation requires JobCard context; use /api/v1/settle for full verification"
            })));
        }
        _ => {
            return Err((
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({
                    "error": "General verification not implemented for this attestation type"
                })),
            ))
        }
    };

    match result {
        Ok(_) => Ok(Json(json!({ "status": "verified" }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn resolve_identity_v1(
    State(state): State<AppState>,
    Json(payload): Json<conxian_core::IdentityResolutionRequest>,
) -> Result<Json<conxian_core::IdentityResolutionResponse>, (StatusCode, Json<Value>)> {
    match state.identity.resolve_identity(&payload).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn ingress_papss(
    headers: HeaderMap,
    State(state): State<AppState>,
    body: Body,
) -> Result<Json<SettlementProposal>, (StatusCode, Json<Value>)> {
    enforce_ingress_trust_policy(&state, &headers)?;

    let raw_payload = body
        .collect()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Body collection failed: {}", e) })),
            )
        })?
        .to_bytes();

    let raw_payload_hash = sha256_hex(&raw_payload);
    let tee_attestation = verify_tee_settlement_attestation(&state, &headers, &raw_payload_hash)?;
    let industrial_intent = extract_industrial_intent(&headers);

    let json_payload: Value = serde_json::from_slice(&raw_payload).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid JSON: {}", e) })),
        )
    })?;

    match state
        .compliance
        .normalize_papss_ingress(&json_payload, raw_payload_hash.clone())
    {
        Ok(mut envelope) => {
            if let Some(intent) = industrial_intent {
                envelope.payload.industrial_intent = intent;
            }
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
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn ingress_brics(
    headers: HeaderMap,
    State(state): State<AppState>,
    body: Body,
) -> Result<Json<SettlementProposal>, (StatusCode, Json<Value>)> {
    enforce_ingress_trust_policy(&state, &headers)?;

    let raw_payload = body
        .collect()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Body collection failed: {}", e) })),
            )
        })?
        .to_bytes();

    let raw_payload_hash = sha256_hex(&raw_payload);
    let tee_attestation = verify_tee_settlement_attestation(&state, &headers, &raw_payload_hash)?;
    let industrial_intent = extract_industrial_intent(&headers);

    let json_payload: Value = serde_json::from_slice(&raw_payload).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid JSON: {}", e) })),
        )
    })?;

    match state
        .compliance
        .normalize_brics_ingress(&json_payload, raw_payload_hash.clone())
    {
        Ok(mut envelope) => {
            if let Some(intent) = industrial_intent {
                envelope.payload.industrial_intent = intent;
            }
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
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn get_handoff_status(State(state): State<AppState>) -> Json<Value> {
    let s = state.shared.read().unwrap();
    Json(json!({
        "current_state": s.handoff_state,
        "bootstrap_wallet": s.wallets.bootstrap,
        "payout_destination": s.wallets.get_payout_destination(s.handoff_state),
        "treasury_destination": s.wallets.get_treasury_destination(s.handoff_state),
        "handoff_complete": s.handoff_state == conxian_core::HandoffState::HandoffComplete
    }))
}

pub async fn update_handoff_state(
    State(state): State<AppState>,
    Json(new_state): Json<conxian_core::HandoffState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut s = state.shared.write().unwrap();
    let old_state = s.handoff_state;
    s.handoff_state = new_state;

    info!(old = ?old_state, new = ?new_state, "System handoff state updated");

    Ok(Json(json!({
        "status": "success",
        "old_state": old_state,
        "new_state": new_state
    })))
}
