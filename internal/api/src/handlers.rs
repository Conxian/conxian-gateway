use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use compliance::SovereignCommit;
use conxian_core::{
    evaluate_trust_metadata_json, AttestationRequest, ConxianError, JobCardSettlementRequest,
    SettlementEnvelope, SettlementProposal, TrustPolicyDecision,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};
use uuid;

pub const TEE_ATTESTATION_HEADER: &str = "x-tee-attestation";
pub const TRUST_METADATA_HEADER: &str = "x-conxian-trust-metadata";
const SETTLEMENT_LOG_MAX_ENTRIES: usize = 1000;
const WEBHOOK_REPLAY_TTL_SECONDS: u64 = 60 * 60 * 24;
const SATOSHIS_PER_SBTC: u64 = 100_000_000;

fn amount_sbtc_to_satoshis(amount_sbtc: f64) -> Result<u64, &'static str> {
    if !amount_sbtc.is_finite() || amount_sbtc < 0.0 {
        return Err("amount_sbtc must be a finite, non-negative number");
    }

    let scaled = amount_sbtc * SATOSHIS_PER_SBTC as f64;
    if !scaled.is_finite() || scaled > u64::MAX as f64 {
        return Err("amount_sbtc is out of range");
    }

    let rounded = scaled.round();
    if (scaled - rounded).abs() > 1e-6 {
        return Err("amount_sbtc must not exceed 8 decimal places");
    }

    Ok(rounded as u64)
}

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
            "epoch": s.stacks.epoch.clone(),
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
        "treasury_balance_stx": s.metrics.treasury_balance_stx,
        "treasury_balance_btc": s.metrics.treasury_balance_btc,
    }))
}

pub async fn create_fiat_session(
    State(state): State<AppState>,
    Json(payload): Json<crate::fiat::OnRampSessionRequest>,
) -> Result<Json<crate::fiat::OnRampSessionResponse>, (StatusCode, Json<Value>)> {
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
                        Json(json!({ "error": "duplicate_webhook" })),
                    ))
                }
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) }),
                    ),
                )),
            }
        }
        Ok(false) => {
            warn!("Invalid fiat webhook signature");
            Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid_signature" })),
            ))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn send_otp(
    State(state): State<AppState>,
    Json(payload): Json<crate::a2p::OtpRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.a2p.send_otp(payload).await {
        Ok((res, hmac, ts)) => Ok(Json(json!({
            "session_id": res.session_id,
            "status": res.status,
            "hmac": hmac,
            "timestamp": ts
        }))),
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
        Ok(valid) => Ok(Json(json!({ "valid": valid }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn sync_erp_ledger(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let raw_payload_hash = sha256_hex(&serde_json::to_vec(&payload).unwrap());

    match state
        .compliance
        .normalize_erp_ingress(&payload, raw_payload_hash)
    {
        Ok(envelopes) => {
            for envelope in &envelopes {
                let _ = state.compliance.commit_settlement(envelope);
            }
            Ok(Json(
                json!({ "status": "success", "count": envelopes.len() }),
            ))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn settle_job_card(
    State(state): State<AppState>,
    Json(payload): Json<JobCardSettlementRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.compliance.verify_bitvm2_settlement(&payload) {
        Ok(txid) => {
            let _ = state.compliance.commit_job_card(&payload.job_card);
            Ok(Json(json!({ "status": "success", "txid": txid })))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn generate_iso_payment(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let amount_sbtc = payload["amount_sbtc"].as_f64().ok_or((
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "amount_sbtc is required" })),
    ))?;

    let amount_satoshis = amount_sbtc_to_satoshis(amount_sbtc)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))))?;

    let receiver = payload["receiver"].as_str().ok_or((
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "receiver is required" })),
    ))?;

    let job_card = conxian_core::ConxianJobCard {
        context: "https://conxian.org/ns/job-card/v2".to_string(),
        r#type: "PaymentJob".to_string(),
        work_intent: conxian_core::WorkIntent {
            sender_address: "GENERATED".to_string(),
            receiver_address: receiver.to_string(),
            amount_sbtc: amount_satoshis,
            town_name: None,
            country_code: None,
        },
    };

    match state.compliance.format_iso20022_pacs008_v8(&job_card) {
        Ok(xml) => Ok(Json(json!({ "xml": xml }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
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

    let xml_payload = String::from_utf8(raw_payload.to_vec()).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid UTF-8 in XML payload" })),
        )
    })?;

    match state
        .compliance
        .normalize_iso20022_ingress(&xml_payload, raw_payload_hash.clone())
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

pub async fn get_external_settlements(
    State(state): State<AppState>,
) -> Json<Vec<SettlementProposal>> {
    let log = state.settlement_log.read().await;
    Json(log.iter().cloned().collect())
}

pub async fn get_alex_quote(
    State(state): State<AppState>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token_x = params["token_x"].as_str().unwrap_or("sBTC");
    let token_y = params["token_y"].as_str().unwrap_or("STX");
    let amount_str = params["amount"].as_str().unwrap_or("0");
    let amount = amount_str.parse::<u128>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid amount format" })),
        )
    })?;

    let req = conxian_core::AlexSwapRequest {
        token_x: token_x.to_string(),
        token_y: token_y.to_string(),
        factor: 100_000_000,
        amount,
        min_dy: None,
    };

    match state.alex.get_swap_quote(req).await {
        Ok(quote) => Ok(Json(json!({ "quote": quote.to_string() }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn execute_alex_swap(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<conxian_core::AlexSwapRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let _ = parse_gateway_x402_payload(&headers).map_err(|e: crate::x402::X402ParseError| {
        (
            e.status_code(),
            Json(json!({ "error": e.message(), "code": e.code() })),
        )
    })?;

    match state.alex.build_swap_payload(payload).await {
        Ok(prepared) => Ok(Json(json!({
            "status": "prepared",
            "payload": prepared
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn toggle_bounty_payouts(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let enabled = payload["enabled"].as_bool().ok_or((
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "enabled boolean is required" })),
    ))?;

    let mut s = state.shared.write().unwrap();
    s.metrics.bounty_payouts_enabled = enabled;

    info!(enabled = %enabled, "Bounty payouts toggled");

    Ok(Json(
        json!({ "status": "success", "bounty_payouts_enabled": enabled }),
    ))
}

pub async fn handle_offline_pos(
    State(state): State<AppState>,
    Json(payload): Json<conxian_core::OfflineReceipt>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state
        .compliance
        .verify_offline_receipt(&payload)
        .unwrap_or(false)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Invalid TEE device ID" })),
        ));
    }

    match state.offline_queue.enqueue(&payload) {
        Ok(_) => Ok(Json(
            json!({ "status": "enqueued", "receipt_id": payload.receipt_id }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn sync_offline_receipts(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pending = state.offline_queue.dequeue_pending().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )
    })?;

    let mut synced_count = 0;
    for mut receipt in pending {
        if state.compliance.gossip_mesh_rehearsal(&mut receipt).is_ok() {
            info!(
                "Rehearsal: Broadcasting offline receipt {} to L2...",
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
    // Enforce BIP-322 verification if signature is provided
    if let Some(ref sig) = payload.signature {
        let verifier: &dyn conxian_core::Bip322Verifier = state.compliance.as_ref();
        let message = format!("Conxian Identity Verification: {}", payload.identifier);

        // Verify that the signature is valid for the provided identifier (assuming it's a Bitcoin address)
        // If the identifier is a name (e.g. .eth), verification would happen after resolution.
        if payload.identifier.starts_with("bc1")
            || payload.identifier.starts_with("1")
            || payload.identifier.starts_with("3")
        {
            match verifier.verify_message(&payload.identifier, &message, sig) {
                Ok(true) => info!("BIP-322 verification successful for {}", payload.identifier),
                _ => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(json!({ "error": "Invalid BIP-322 signature for provided address" })),
                    ))
                }
            }
        }
    }

    match state.identity.resolve_identity(&payload).await {
        Ok(res) => Ok(Json(res)),
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

pub async fn list_supported_chains(State(state): State<AppState>) -> Json<Value> {
    let chains: Vec<String> = state.multi_chain.keys().cloned().collect();
    Json(json!({ "supported_chains": chains }))
}

pub async fn get_chain_height(
    Path(chain): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let adapter = state.multi_chain.get(&chain).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Chain adapter not found for: {}", chain) })),
        )
    })?;

    match adapter.get_latest_height().await {
        Ok(height) => Ok(Json(json!({ "chain": chain, "height": height }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn prepare_chain_tx(
    Path(chain): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let adapter = state.multi_chain.get(&chain).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Chain adapter not found for: {}", chain) })),
        )
    })?;

    match adapter.prepare_unsigned_transaction(payload).await {
        Ok(prepared) => Ok(Json(prepared)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

pub async fn verify_state_proof(
    Path(chain): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.verifier.verify_state_proof(&chain, payload).await {
        Ok(verified) => Ok(Json(json!({ "chain": chain, "verified": verified }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

fn build_settlement_proposal(
    state: &AppState,
    envelope: SettlementEnvelope,
    tee_attestation: AttestationRequest,
    raw_payload_hash: &str,
) -> Result<SettlementProposal, (StatusCode, Json<Value>)> {
    let _trigger_id = state
        .compliance
        .compute_trigger_id(
            &format!("{:?}", envelope.payload.source),
            raw_payload_hash,
            &envelope.payload.identifiers,
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to compute trigger ID: {}", e) })),
            )
        })?;

    let s = state.shared.read().unwrap();
    let current_burn_height = s.stacks.burn_block_height.unwrap_or(0);

    SettlementProposal::new(
        format!("prop-{}", uuid::Uuid::new_v4()),
        envelope,
        tee_attestation,
        current_burn_height,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to build proposal: {}", e) })),
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

fn enforce_ingress_trust_policy(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<Value>)> {
    let metadata_json = headers
        .get(TRUST_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Missing trust metadata header" })),
        ))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let decision = evaluate_trust_metadata_json(Some(metadata_json), now);

    let mut s = state.shared.write().unwrap();
    match decision {
        TrustPolicyDecision::Allow => {
            s.metrics.trust_policy_allow += 1;
            Ok(())
        }
        TrustPolicyDecision::Block(reason) => {
            s.metrics.trust_policy_block += 1;
            warn!(reason = ?reason, "Ingress blocked by trust policy");
            Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "blocked_by_trust_policy", "code": reason.as_str() })),
            ))
        }
    }
}

fn verify_tee_settlement_attestation(
    state: &AppState,
    headers: &HeaderMap,
    payload_hash: &str,
) -> Result<AttestationRequest, (StatusCode, Json<Value>)> {
    let att_json = headers
        .get("x-conxian-attestation")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Missing TEE attestation header" })),
        ))?;

    let att: AttestationRequest = serde_json::from_str(att_json).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid TEE attestation format" })),
        )
    })?;

    state
        .compliance
        .verify_settlement_trigger_attestation(&att, payload_hash)
        .map_err(|e| {
            (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
            )
        })?;

    Ok(att)
}

fn parse_gateway_x402_payload(
    headers: &HeaderMap,
) -> Result<crate::x402::X402PaymentPayload, crate::x402::X402ParseError> {
    crate::x402::parse_gateway_x402_payload(headers)
}
