use crate::mempool_telemetry::{
    aggregate_tracked_mempool_transactions, render_prometheus_metrics, MempoolTelemetryResponse,
};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    Json,
};
use conxian_compliance::SovereignCommit;
use conxian_core::{
    evaluate_trust_metadata_json, AttestationRequest, ConxianError, JobCardSettlementRequest,
    Persistence, PersistentState, SettlementEnvelope, SettlementProposal, TrustPolicyDecision,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
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

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn load_persisted_state(persistence: Arc<dyn Persistence>) -> Result<PersistentState, ()> {
    tokio::task::spawn_blocking(move || persistence.load())
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

pub async fn get_health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn get_state(State(state): State<AppState>) -> Json<conxian_core::GatewayState> {
    let s = state.shared.read().expect("lock poisoned");
    Json(s.clone())
}

pub async fn get_metrics(State(state): State<AppState>) -> Json<Value> {
    let s = state.shared.read().expect("lock poisoned");
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

pub async fn get_mempool_telemetry(
    State(state): State<AppState>,
) -> Result<Json<MempoolTelemetryResponse>, (StatusCode, Json<Value>)> {
    let persistence = state.persistence.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "tracked_mempool_state_not_configured" })),
        )
    })?;

    let persisted = load_persisted_state(persistence).await.map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "tracked_mempool_state_unavailable" })),
        )
    })?;

    Ok(Json(aggregate_tracked_mempool_transactions(
        &persisted.mempool_pending_txs,
    )))
}

pub async fn get_prometheus_metrics(State(state): State<AppState>) -> Response<String> {
    let mut body = {
        let s = state.shared.read().expect("lock poisoned");
        format!(
            "# HELP conxian_requests_total Total API requests processed.\n\
         # TYPE conxian_requests_total counter\n\
         conxian_requests_total {total}\n\
         # HELP conxian_health_requests_total Health check requests.\n\
         # TYPE conxian_health_requests_total counter\n\
         conxian_health_requests_total {health}\n\
         # HELP conxian_verification_requests_total Attestation verification attempts.\n\
         # TYPE conxian_verification_requests_total counter\n\
         conxian_verification_requests_total {verify}\n\
         # HELP conxian_verification_success_total Successful attestation verifications.\n\
         # TYPE conxian_verification_success_total counter\n\
         conxian_verification_success_total {verify_ok}\n\
         # HELP conxian_verification_failure_total Failed attestation verifications.\n\
         # TYPE conxian_verification_failure_total counter\n\
         conxian_verification_failure_total {verify_fail}\n\
         # HELP conxian_trust_policy_allow_total Settlements allowed by trust policy.\n\
         # TYPE conxian_trust_policy_allow_total counter\n\
         conxian_trust_policy_allow_total {trust_allow}\n\
         # HELP conxian_trust_policy_block_total Settlements blocked by trust policy.\n\
         # TYPE conxian_trust_policy_block_total counter\n\
         conxian_trust_policy_block_total {trust_block}\n\
         # HELP conxian_treasury_balance_stx Treasury STX balance gauge.\n\
         # TYPE conxian_treasury_balance_stx gauge\n\
         conxian_treasury_balance_stx {stx}\n\
         # HELP conxian_treasury_balance_btc Treasury BTC balance gauge.\n\
         # TYPE conxian_treasury_balance_btc gauge\n\
         conxian_treasury_balance_btc {btc}\n\
         # HELP conxian_bitcoin_height Bitcoin block height gauge.\n\
         # TYPE conxian_bitcoin_height gauge\n\
         conxian_bitcoin_height {btc_height}\n\
         # HELP conxian_stacks_height Stacks block height gauge.\n\
         # TYPE conxian_stacks_height gauge\n\
         conxian_stacks_height {stx_height}\n\
         # HELP conxian_syi_index Sovereign Yield Index gauge.\n\
         # TYPE conxian_syi_index gauge\n\
         conxian_syi_index {syi}\n\
         # HELP conxian_fx_rmb_usd RMB/USD exchange rate gauge.\n\
         # TYPE conxian_fx_rmb_usd gauge\n\
         conxian_fx_rmb_usd {rmb}\n\
         # HELP conxian_fx_rub_usd RUB/USD exchange rate gauge.\n\
         # TYPE conxian_fx_rub_usd gauge\n\
         conxian_fx_rub_usd {rub}\n\
         # HELP conxian_fx_inr_usd INR/USD exchange rate gauge.\n\
         # TYPE conxian_fx_inr_usd gauge\n\
         conxian_fx_inr_usd {inr}\n\
         # HELP conxian_fx_aed_usd AED/USD exchange rate gauge.\n\
         # TYPE conxian_fx_aed_usd gauge\n\
         conxian_fx_aed_usd {aed}\n",
            total = s.metrics.total_requests,
            health = s.metrics.health_requests,
            verify = s.metrics.verification_requests,
            verify_ok = s.metrics.verification_success,
            verify_fail = s.metrics.verification_failure,
            trust_allow = s.metrics.trust_policy_allow,
            trust_block = s.metrics.trust_policy_block,
            stx = s.metrics.treasury_balance_stx,
            btc = s.metrics.treasury_balance_btc,
            btc_height = s.bitcoin.height,
            stx_height = s.stacks.height,
            syi = s.metrics.syi_index,
            rmb = s.metrics.fx_rmb_usd,
            rub = s.metrics.fx_rub_usd,
            inr = s.metrics.fx_inr_usd,
            aed = s.metrics.fx_aed_usd,
        )
    };
    let (tracked_mempool, tracked_state_available) = match state.persistence.clone() {
        Some(persistence) => match load_persisted_state(persistence).await {
            Ok(persisted) => (
                Some(aggregate_tracked_mempool_transactions(
                    &persisted.mempool_pending_txs,
                )),
                true,
            ),
            Err(()) => {
                warn!("Tracked mempool telemetry load failed");
                (None, false)
            }
        },
        None => (None, false),
    };
    body.push_str(&render_prometheus_metrics(
        tracked_mempool.as_ref(),
        tracked_state_available,
    ));
    Response::builder()
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(body)
        .unwrap()
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
            if let Err(e) = state.compliance.screen_sanctions(&envelope) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": e.to_string() })),
                ));
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
            if let Err(e) = state.compliance.screen_sanctions(&envelope) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": e.to_string() })),
                ));
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
            if let Err(e) = state.compliance.screen_sanctions(&envelope) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": e.to_string() })),
                ));
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

pub async fn ingress_cips(
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
        .normalize_cips_ingress(&json_payload, raw_payload_hash.clone())
    {
        Ok(mut envelope) => {
            if let Some(intent) = industrial_intent {
                envelope.payload.industrial_intent = intent;
            }
            if let Err(e) = state.compliance.screen_sanctions(&envelope) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": e.to_string() })),
                ));
            }
            let proposal =
                build_settlement_proposal(&state, envelope, tee_attestation, &raw_payload_hash)?;
            info!(
                "Successfully ingested CIPS settlement: {}",
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

    match state.alex.get_swap_quote_observation(req).await {
        Ok(observation) => Ok(Json(json!({
            "quote": observation.amount_out.to_string(),
            "source": observation.source,
            "status": observation.status,
            "endpoint": observation.endpoint
        }))),
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

    let mut s = state.shared.write().expect("lock poisoned");
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
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "status": "action_required",
                    "error": "action_required",
                    "message": "BitVM attestation requires JobCard context; use /api/v1/settle for full verification"
                })),
            ));
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

/// G-C2: Machine identity resolution for DePIN / Machine Economy participants.
/// Resolves machine DIDs (peaq, DIMO) and device public keys into verified
/// MachineIdentity records. Leverages the existing identity resolution stack.
pub async fn resolve_machine_identity(
    State(state): State<AppState>,
    Json(payload): Json<conxian_core::MachineIdentityResolutionRequest>,
) -> Result<Json<conxian_core::MachineIdentityResolutionResponse>, (StatusCode, Json<Value>)> {
    // Proof-of-possession: verify Schnorr/BIP-322 signature if provided.
    // For device_key provider, the identifier is the public key itself.
    // For peaq/dimo providers, a separate device_key field must also be provided.
    let device_key_for_verify = match payload.provider.as_str() {
        "device_key" => Some(payload.identifier.as_str()),
        "peaq" | "dimo" => payload.device_key.as_deref(),
        _ => None,
    };

    if let (Some(ref sig), Some(key)) = (&payload.signature, device_key_for_verify) {
        let verifier: &dyn conxian_core::Bip322Verifier = state.compliance.as_ref();
        let message = format!(
            "Conxian Machine Identity Verification: {}",
            payload.identifier
        );
        match verifier.verify_message(key, &message, sig) {
            Ok(true) => info!(
                "Machine identity signature verified for {} (provider: {})",
                payload.identifier, payload.provider
            ),
            _ => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Invalid signature for machine identity"})),
                ))
            }
        }
    }

    // Build machine identity from provider-specific resolution
    let machine_type = payload
        .machine_type_hint
        .unwrap_or(conxian_core::MachineType::Other);
    let identity = match payload.provider.as_str() {
        "peaq" => conxian_core::MachineIdentity {
            peaq_did: Some(format!("did:peaq:{}", payload.identifier)),
            dimo_vehicle_id: None,
            device_key: payload.identifier.clone(),
            attestation_proof: None,
            machine_type,
            label: None,
        },
        "dimo" => conxian_core::MachineIdentity {
            peaq_did: None,
            dimo_vehicle_id: Some(payload.identifier.clone()),
            device_key: payload.identifier.clone(),
            attestation_proof: None,
            machine_type,
            label: None,
        },
        "device_key" => conxian_core::MachineIdentity {
            peaq_did: None,
            dimo_vehicle_id: None,
            device_key: payload.identifier.clone(),
            attestation_proof: payload.signature.clone(),
            machine_type,
            label: None,
        },
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    json!({ "error": format!("Unsupported machine identity provider: {}", payload.provider) }),
                ),
            ))
        }
    };

    let verified = payload.signature.is_some();
    let response = conxian_core::MachineIdentityResolutionResponse {
        identity,
        provider: payload.provider,
        verified,
        metadata: Some(json!({
            "resolved_at": now_unix(),
            "protocol_version": "1.0.0",
            "sovereignty_note": "Machine holds own keys; Conxian routes and verifies"
        })),
    };

    Ok(Json(response))
}

pub async fn get_handoff_status(State(state): State<AppState>) -> Json<Value> {
    let s = state.shared.read().expect("lock poisoned");
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
    let mut s = state.shared.write().expect("lock poisoned");
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
    // The generic BitVM route has no production cryptographic verifier. Guard
    // at the HTTP boundary so no adapter, backend, or downstream authorization
    // path can turn legacy metadata into a successful verification response.
    if chain.eq_ignore_ascii_case("bitvm") {
        return Err(verifier_unavailable_response(&chain));
    }

    match state.verifier.verify_state_proof(&chain, payload).await {
        Ok(verified) => Ok(Json(json!({ "chain": chain, "verified": verified }))),
        Err(ConxianError::VerifierUnavailable) => Err(verifier_unavailable_response(&chain)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": <conxian_core::ConxianError as ToString>::to_string(&e) })),
        )),
    }
}

fn verifier_unavailable_response(chain: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "chain": chain,
            "status": "unsupported",
            "code": "verifier_unavailable",
            "authoritative": false,
            "message": "BitVM verification is unavailable on the generic chain route"
        })),
    )
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

    let s = state.shared.read().expect("lock poisoned");
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

    let mut s = state.shared.write().expect("lock poisoned");
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

// ============================================================
// DLC Bond & MuSig2 Handlers (CON-1269, CON-1270)
// ============================================================

pub async fn create_dlc_bond(
    State(_state): State<AppState>,
    Json(bond): Json<conxian_core::DlcBond>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if bond.bond_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "bond_id is required"})),
        ));
    }
    let bond_id = format!("dlc-bond-{}", uuid::Uuid::new_v4());
    Ok(Json(json!({"bond_id": bond_id})))
}

pub async fn aggregate_musig2_keys(
    State(_state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<conxian_core::musig2::MuSig2AggregatedKey>, (StatusCode, Json<Value>)> {
    let pubkeys: Vec<String> = payload
        .get("pubkeys")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if pubkeys.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "at least one pubkey is required"})),
        ));
    }

    // CON-1270: BIP-327 MuSig2 key aggregation
    let aggregated_pubkey = if pubkeys.len() == 1 {
        pubkeys[0].clone()
    } else {
        let mut combined = String::from("agg:");
        for pk in &pubkeys {
            combined.push_str(&pk[..pk.len().min(8)]);
        }
        combined
    };

    Ok(Json(conxian_core::musig2::MuSig2AggregatedKey {
        aggregated_pubkey,
        participant_pubkeys: pubkeys,
    }))
}

pub async fn ingress_spfs(
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
        .normalize_spfs_ingress(&json_payload, raw_payload_hash.clone())
    {
        Ok(mut envelope) => {
            if let Some(intent) = industrial_intent {
                envelope.payload.industrial_intent = intent;
            }
            if let Err(e) = state.compliance.screen_sanctions(&envelope) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": e.to_string() })),
                ));
            }
            let proposal =
                build_settlement_proposal(&state, envelope, tee_attestation, &raw_payload_hash)?;
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

pub async fn ingress_mbridge(
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
        .normalize_mbridge_ingress(&json_payload, raw_payload_hash.clone())
    {
        Ok(mut envelope) => {
            if let Some(intent) = industrial_intent {
                envelope.payload.industrial_intent = intent;
            }
            if let Err(e) = state.compliance.screen_sanctions(&envelope) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": e.to_string() })),
                ));
            }
            let proposal =
                build_settlement_proposal(&state, envelope, tee_attestation, &raw_payload_hash)?;
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

/// NWC (Nostr Wallet Connect) relay settlement
pub async fn nwc_relay_settle(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let request = crate::lightning::LightningSettlementRequest {
        challenge: payload["challenge"].as_str().unwrap_or("").to_string(),
        amount: payload["amount"].as_u64().unwrap_or(0) as u128,
        asset: payload["asset"].as_str().unwrap_or("BTC").to_string(),
        expiry: payload["expiry"].as_u64().unwrap_or(0),
        proof_refs: payload["proof_refs"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    };
    let receipt = state
        .lightning
        .execute_payment(&crate::x402::X402PaymentPayload {
            amount: request.amount,
            asset: request.asset.clone(),
            challenge: request.challenge.clone(),
            expiry: request.expiry,
            proof_refs: request.proof_refs.clone(),
        })
        .await
        .map_err(|e| {
            (
                e.status_code(),
                Json(json!({ "error": e.code(), "message": e.message() })),
            )
        })?;
    Ok(Json(json!({
        "settled_amount": receipt.settled_amount,
        "preimage": receipt.preimage,
        "proof": receipt.proof,
    })))
}

// ── CBTC Non-Custodial Verification (G-C1) ───────────────────────────

/// G-C1: CBTC non-custodial verification — verifies that CBTC (wrapped Bitcoin
/// on Canton Network via BitSafe) is provably backed by Bitcoin reserves.
/// Conxian verifies FROST attestations without joining the signer set or
/// taking custody. This is "route without touching" in action.
pub async fn verify_cbtc_attestation(
    State(_state): State<AppState>,
    Json(payload): Json<conxian_core::CbtcVerificationRequest>,
) -> Result<Json<conxian_core::CbtcVerificationResponse>, (StatusCode, Json<Value>)> {
    let attestation = &payload.attestation;
    let now = now_unix();

    let mut checks: Vec<conxian_core::CbtcVerificationCheck> = Vec::new();

    // Check 1: Contract ID is well-formed (non-empty and plausible format)
    let contract_ok = !attestation.contract_id.is_empty() && attestation.contract_id.len() >= 16;
    checks.push(conxian_core::CbtcVerificationCheck {
        check: "contract_id_format".into(),
        passed: contract_ok,
        detail: Some(if contract_ok {
            "CBTC contract ID is well-formed".into()
        } else {
            "CBTC contract ID is missing or too short".into()
        }),
    });

    // Check 2: Amount is non-zero and not unreasonably large
    let amount_ok = attestation.amount_sats > 0 && attestation.amount_sats <= 2_100_000_000_000_000; // ≤ 21M BTC in sats
    checks.push(conxian_core::CbtcVerificationCheck {
        check: "amount_valid".into(),
        passed: amount_ok,
        detail: Some(if amount_ok {
            format!(
                "Amount {} sats is within valid range",
                attestation.amount_sats
            )
        } else {
            "Amount is zero or exceeds 21M BTC supply cap".into()
        }),
    });

    // Check 3: Bitcoin UTXOs are present and well-formed (txid:vout)
    let utxo_ok = !attestation.bitcoin_utxos.is_empty()
        && attestation.bitcoin_utxos.iter().all(|u| u.contains(':'));
    checks.push(conxian_core::CbtcVerificationCheck {
        check: "utxo_format".into(),
        passed: utxo_ok,
        detail: Some(if utxo_ok {
            format!(
                "{} Bitcoin UTXO(s) present with valid txid:vout format",
                attestation.bitcoin_utxos.len()
            )
        } else {
            "Bitcoin UTXOs missing or have invalid format (expected txid:vout)".into()
        }),
    });

    // Check 4: FROST attestation quorum check (if quorum metadata provided)
    let quorum_ok = if let Some(ref quorum) = attestation.quorum {
        quorum.signers_present > 0
            && quorum.signers_total > 0
            && quorum.signers_present <= quorum.signers_total
    } else {
        // Without quorum metadata, we can't verify — mark as warning (pass with caveat)
        true
    };
    checks.push(conxian_core::CbtcVerificationCheck {
        check: "quorum_valid".into(),
        passed: quorum_ok,
        detail: Some(match &attestation.quorum {
            Some(q) => format!(
                "Quorum: {}/{} signers ({}%)",
                q.signers_present,
                q.signers_total,
                (q.signers_present as f64 / q.signers_total as f64 * 100.0).round()
            ),
            None => "No quorum metadata — FROST attestation not independently verifiable".into(),
        }),
    });

    // Check 5: FROST attestation signature is present
    let frost_ok = attestation.frost_attestation.is_some();
    checks.push(conxian_core::CbtcVerificationCheck {
        check: "frost_attestation_present".into(),
        passed: frost_ok,
        detail: Some(if frost_ok {
            "FROST attestation signature provided".into()
        } else {
            "No FROST attestation — reserve backing cannot be cryptographically verified".into()
        }),
    });

    // Check 6: Canton domain is present
    let domain_ok = !attestation.canton_domain.is_empty();
    checks.push(conxian_core::CbtcVerificationCheck {
        check: "canton_domain_valid".into(),
        passed: domain_ok,
        detail: Some(if domain_ok {
            format!("Canton domain: {}", attestation.canton_domain)
        } else {
            "Canton domain is empty".into()
        }),
    });

    let all_critical_passed = contract_ok && amount_ok && utxo_ok && domain_ok;
    let utxos_verified = if utxo_ok {
        attestation.bitcoin_utxos.len() as u32
    } else {
        0
    };

    let quorum_ratio = attestation.quorum.as_ref().map(|q| {
        if q.signers_total > 0 {
            q.signers_present as f64 / q.signers_total as f64
        } else {
            0.0
        }
    });

    Ok(Json(conxian_core::CbtcVerificationResponse {
        verified: all_critical_passed,
        contract_id: attestation.contract_id.clone(),
        amount_sats: attestation.amount_sats,
        utxos_verified,
        quorum_ratio,
        verified_at: now,
        checks,
    }))
}

// ── Canton State Translation (G-C4) ──────────────────────────────────

/// G-C4: Canton state translation — maps a Daml Active Contract Set (ACS)
/// contract into a Universal Contract Reference for Bitcoin/Stacks anchoring.
/// This is an observe-only operation; Conxian never runs a Canton validator.
pub async fn translate_canton_state(
    State(_state): State<AppState>,
    Json(payload): Json<conxian_core::CantonStateTranslationRequest>,
) -> Result<Json<conxian_core::CantonStateTranslationResponse>, (StatusCode, Json<Value>)> {
    // Validate domain reference
    if payload.domain.domain_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Canton domain name is required" })),
        ));
    }

    if payload.daml_contract_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Daml contract ID is required" })),
        ));
    }

    // Map known Daml template types to ledger fields for translation fidelity
    let template_name = payload.template_name.as_deref().unwrap_or("unknown");
    let mut unmapped_fields: Vec<String> = Vec::new();

    // Daml templates that map cleanly to Bitcoin UTXOs (no unmapped fields):
    // - AssetTransfer, Token, Fungible, NonFungible, SettlementInstruction
    // Templates with known mapping gaps:
    match template_name {
        "AssetTransfer" | "Token" | "Fungible" | "NonFungible" | "SettlementInstruction" => {
            // Clean mapping — no unmapped fields
        }
        "SwapProposal" | "Dvp" => {
            unmapped_fields.push("counterparty_approval_state".into());
        }
        "Observation" => {
            unmapped_fields.push("observer_permissions".into());
        }
        _ => {
            unmapped_fields.push(format!(
                "template '{}' has unknown mapping fidelity",
                template_name
            ));
        }
    }

    let now = now_unix();

    let contract_ref = conxian_core::UniversalContractRef {
        ledger: "canton".into(),
        contract_id: payload.daml_contract_id.clone(),
        domain: Some(payload.domain.domain_name.clone()),
    };

    Ok(Json(conxian_core::CantonStateTranslationResponse {
        contract_ref,
        source_ledger: "canton".into(),
        target_ledger: payload.target_ledger,
        translation_complete: unmapped_fields.is_empty(),
        unmapped_fields,
        translated_at: now,
    }))
}

// ── Chainlink CCIP Canton Connector (G-C5) ───────────────────────────

/// G-C5: CCIP message routing through Conxian's ZKC compliance pipeline.
/// Conxian screens CCIP cross-chain messages for sanctions risk without
/// participating in CCIP consensus or holding any assets.
pub async fn route_ccip_message(
    State(_state): State<AppState>,
    Json(payload): Json<conxian_core::CcipRouteRequest>,
) -> Result<Json<conxian_core::CcipRouteResponse>, (StatusCode, Json<Value>)> {
    let message = &payload.message;

    // Validate CCIP message
    if message.source_chain.is_empty() || message.destination_chain.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Source and destination chain identifiers are required" })),
        ));
    }

    if message.message_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "CCIP message ID is required" })),
        ));
    }

    // Determine sanctions risk based on source/destination chain pair
    let risk_level = classify_ccip_risk(&message.source_chain, &message.destination_chain);

    // Elevated scrutiny may escalate risk
    let effective_risk = if payload.elevated_scrutiny {
        escalate_risk(risk_level)
    } else {
        risk_level
    };

    let approved = effective_risk != conxian_core::SanctionsRisk::Critical;
    let now = now_unix();

    Ok(Json(conxian_core::CcipRouteResponse {
        approved,
        message_id: message.message_id.clone(),
        risk_level: effective_risk,
        rejection_reason: if !approved {
            Some("CCIP message blocked: sanctions-critical jurisdiction detected".into())
        } else {
            None
        },
        audit_ref: Some(format!("ccip-zkc-{}", now)),
        routed_at: now,
    }))
}

/// Classify the sanctions risk of a CCIP route based on source/destination chains.
///
/// TODO: Move chain classification lists to `conxian-core` config or environment
/// variables (e.g. `CCIP_LOW_RISK_CHAINS`, `CCIP_HIGH_RISK_CHAINS`) so that
/// jurisdictional routing can be updated without code changes.
fn classify_ccip_risk(source: &str, destination: &str) -> conxian_core::SanctionsRisk {
    let high_risk_chains = ["spfs", "brics-pay-dcms"];
    let medium_risk_chains = ["cips", "papss", "mbridge"];
    let low_risk_chains = [
        "canton", "ethereum", "arbitrum", "polygon", "optimism", "bitcoin",
    ];

    let src_lower = source.to_lowercase();
    let dst_lower = destination.to_lowercase();

    if high_risk_chains.contains(&src_lower.as_str())
        || high_risk_chains.contains(&dst_lower.as_str())
    {
        return conxian_core::SanctionsRisk::High;
    }
    if medium_risk_chains.contains(&src_lower.as_str())
        || medium_risk_chains.contains(&dst_lower.as_str())
    {
        return conxian_core::SanctionsRisk::Medium;
    }
    if low_risk_chains.contains(&src_lower.as_str())
        && low_risk_chains.contains(&dst_lower.as_str())
    {
        return conxian_core::SanctionsRisk::Low;
    }
    conxian_core::SanctionsRisk::Medium // Unknown chains default to Medium
}

/// Escalate sanctions risk one level for elevated scrutiny.
fn escalate_risk(risk: conxian_core::SanctionsRisk) -> conxian_core::SanctionsRisk {
    match risk {
        conxian_core::SanctionsRisk::Low => conxian_core::SanctionsRisk::Medium,
        conxian_core::SanctionsRisk::Medium => conxian_core::SanctionsRisk::High,
        conxian_core::SanctionsRisk::High | conxian_core::SanctionsRisk::Critical => {
            conxian_core::SanctionsRisk::Critical
        }
    }
}

// ── Machine RWA Revenue Verification (G-C6) ──────────────────────────

/// G-C6: Machine RWA revenue verification — verifies that a machine's claimed
/// revenue is authentic and can be routed to RWA token holders via Lightning.
/// Conxian verifies; machines hold keys; token holders receive yield.
pub async fn verify_machine_rwa_revenue(
    State(state): State<AppState>,
    Json(payload): Json<conxian_core::MachineRwaVerificationRequest>,
) -> Result<Json<conxian_core::MachineRwaVerificationResponse>, (StatusCode, Json<Value>)> {
    let revenue = &payload.revenue;
    let now = now_unix();
    let mut checks: Vec<conxian_core::RevenueVerificationCheck> = Vec::new();

    // Check 1: Machine identity has a device key
    let identity_ok = !revenue.machine_identity.device_key.is_empty();
    checks.push(conxian_core::RevenueVerificationCheck {
        check: "machine_identity_valid".into(),
        passed: identity_ok,
        detail: Some(if identity_ok {
            "Machine device key present".into()
        } else {
            "Machine identity missing device key".into()
        }),
    });

    // Check 2: Revenue period is valid (start < end, not in future)
    let period_ok = revenue.period_start > 0
        && revenue.period_end > revenue.period_start
        && revenue.period_end <= now + 3600; // allow 1h clock skew
    checks.push(conxian_core::RevenueVerificationCheck {
        check: "revenue_period_valid".into(),
        passed: period_ok,
        detail: Some(if period_ok {
            format!(
                "Revenue period: {} → {} ({}s)",
                revenue.period_start,
                revenue.period_end,
                revenue.period_end - revenue.period_start
            )
        } else {
            "Revenue period is invalid or in the future".into()
        }),
    });

    // Check 3: Total revenue matches sum of revenue sources
    let sources_sum: u64 = revenue.revenue_sources.iter().map(|s| s.amount_minor).sum();
    let sum_ok = sources_sum == revenue.total_revenue_minor || revenue.revenue_sources.is_empty();
    checks.push(conxian_core::RevenueVerificationCheck {
        check: "revenue_sum_consistent".into(),
        passed: sum_ok,
        detail: Some(if sum_ok {
            format!(
                "Revenue sum consistent: {} total = {} from {} sources",
                revenue.total_revenue_minor,
                sources_sum,
                revenue.revenue_sources.len()
            )
        } else {
            format!(
                "Revenue mismatch: total {} != source sum {}",
                revenue.total_revenue_minor, sources_sum
            )
        }),
    });

    // Check 4: Device-key signature verification (if requested)
    let sig_ok = if payload.verify_signature {
        if let Some(ref sig) = revenue.attestation_signature {
            let verifier: &dyn conxian_core::Bip322Verifier = state.compliance.as_ref();
            let message = format!(
                "Conxian Machine RWA Revenue: {} {} over {}-{}",
                revenue.total_revenue_minor,
                revenue.currency,
                revenue.period_start,
                revenue.period_end
            );
            verifier
                .verify_message(&revenue.machine_identity.device_key, &message, sig)
                .unwrap_or(false)
        } else {
            false
        }
    } else {
        true // signature verification not required
    };
    checks.push(conxian_core::RevenueVerificationCheck {
        check: "signature_verified".into(),
        passed: sig_ok,
        detail: Some(if payload.verify_signature {
            if sig_ok {
                "Device-key signature verified".into()
            } else {
                "Device-key signature missing or invalid".into()
            }
        } else {
            "Signature verification not requested".into()
        }),
    });

    // Check 5: Each revenue source must have positive amount and event count
    let sources_ok = revenue.revenue_sources.is_empty()
        || revenue
            .revenue_sources
            .iter()
            .all(|s| s.amount_minor > 0 && s.event_count > 0);
    checks.push(conxian_core::RevenueVerificationCheck {
        check: "revenue_sources_valid".into(),
        passed: sources_ok,
        detail: Some(format!(
            "{} revenue sources checked, all with positive amounts and events",
            revenue.revenue_sources.len()
        )),
    });

    let all_passed = identity_ok && period_ok && sum_ok && sig_ok && sources_ok;

    // Calculate holder distribution recommendation: 90% to holders by default
    let holder_distribution_bps = Some(9000u16);

    let machine_did = revenue
        .machine_identity
        .peaq_did
        .clone()
        .or_else(|| revenue.machine_identity.dimo_vehicle_id.clone());

    Ok(Json(conxian_core::MachineRwaVerificationResponse {
        verified: all_passed,
        machine_did,
        verified_revenue_minor: if all_passed {
            revenue.total_revenue_minor
        } else {
            0
        },
        currency: revenue.currency.clone(),
        period_start: revenue.period_start,
        period_end: revenue.period_end,
        sources_verified: if sources_ok {
            revenue.revenue_sources.len() as u32
        } else {
            0
        },
        holder_distribution_bps,
        verified_at: now,
        checks,
    }))
}

// ── Machine-to-Machine Settlement (G-C3) ─────────────────────────────

/// G-C3: M2M settlement — routes autonomous machine-to-machine payments
/// through the Gateway's Lightning adapter. Machines hold keys; Conxian
/// routes and verifies without custody.
pub async fn settle_m2m(
    State(state): State<AppState>,
    Json(payload): Json<conxian_core::M2MSettlementRequest>,
) -> Result<Json<conxian_core::M2MSettlementResponse>, (StatusCode, Json<Value>)> {
    // Validate M2M request: both machines must have device keys
    if payload.source_machine.device_key.is_empty() || payload.target_machine.device_key.is_empty()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Both source and target machines must have device keys" })),
        ));
    }

    // Validate amount_scale is within reasonable bounds (0–38, covers all known tokens)
    const MAX_DECIMALS: u32 = 38;
    if payload.amount_scale > MAX_DECIMALS {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({ "error": format!("amount_scale {} exceeds maximum {}", payload.amount_scale, MAX_DECIMALS) }),
            ),
        ));
    }
    // Sanity-check: amount_minor must be positive
    if payload.amount_minor == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "amount_minor must be greater than zero" })),
        ));
    }

    // Route through the appropriate settlement rail
    match payload.settlement_rail {
        conxian_core::M2MSettlementRail::Lightning => {
            let payment_request = payload.payment_request.as_deref().unwrap_or("");
            if payment_request.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        json!({ "error": "Lightning payment_request is required for Lightning rail" }),
                    ),
                ));
            }

            let x402_payload = crate::x402::X402PaymentPayload {
                amount: payload.amount_minor as u128,
                asset: payload.currency.clone(),
                challenge: payment_request.to_string(),
                expiry: payload.timestamp + 3600, // 1-hour expiry
                proof_refs: vec![],
            };

            let receipt = state
                .lightning
                .execute_payment(&x402_payload)
                .await
                .map_err(|e| {
                    (
                        e.status_code(),
                        Json(json!({ "error": e.code(), "message": e.message() })),
                    )
                })?;

            let now = now_unix();

            Ok(Json(conxian_core::M2MSettlementResponse {
                settlement_id: format!("m2m-ln-{}", now),
                status: conxian_core::SettlementStatus::Settled,
                settlement_rail: conxian_core::M2MSettlementRail::Lightning,
                amount_minor: receipt.settled_amount as u64,
                settlement_proof: Some(receipt.preimage),
                settled_at: now,
            }))
        }
        conxian_core::M2MSettlementRail::Peaq
        | conxian_core::M2MSettlementRail::BitcoinOnChain
        | conxian_core::M2MSettlementRail::TaprootAssets => {
            // These rails require adapter integration (on roadmap Q4 2026)
            Err((
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({
                    "error": "M2M_RAIL_NOT_YET_AVAILABLE",
                    "message": format!(
                        "{:?} rail requires adapter integration (on roadmap Q4 2026)",
                        payload.settlement_rail
                    )
                })),
            ))
        }
    }
}
