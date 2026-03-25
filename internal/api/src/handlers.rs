use crate::fiat::{FiatRouter, OnRampSessionRequest, OnRampSessionResponse, WebhookPayload};
use axum::{extract::State, http::StatusCode, Json};
use compliance::{IdentityManager, ZkcVerifier};
use conxian_core::{AttestationRequest, GcpTokenRequest, SharedState};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

pub async fn health_check(State(state): State<SharedState>) -> Json<Value> {
    let s = state.read().unwrap();
    let mut status = "healthy";
    let mut details = Vec::new();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Check Bitcoin sync status
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

    // Check Stacks sync status
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

    {
        let mut s_write = state.write().unwrap();
        s_write.metrics.total_requests += 1;
        s_write.metrics.health_requests += 1;
    }

    Json(json!({
        "status": status,
        "service": "conxian-gateway",
        "version": conxian_core::VERSION,
        "details": if details.is_empty() { None } else { Some(details) },
        "timestamp": now,
        "industry_enhancements": "enabled"
    }))
}

pub async fn get_state(State(state): State<SharedState>) -> Json<Value> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
        s.metrics.state_requests += 1;
    }
    let s = state.read().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let uptime = now.saturating_sub(s.start_time);

    Json(json!({
        "bitcoin": s.bitcoin,
        "stacks": s.stacks,
        "metrics": s.metrics,
        "start_time": s.start_time,
        "uptime_seconds": uptime,
        "current_timestamp": now,
        "tam_capture": {
            "sbtc_liquidity": s.metrics.sbtc_liquidity,
            "syi_index": s.metrics.syi_index
        }
    }))
}

pub async fn get_metrics(State(state): State<SharedState>) -> String {
    let mut s_write = state.write().unwrap();
    s_write.metrics.total_requests += 1;
    s_write.metrics.metrics_requests += 1;
    drop(s_write);

    let s = state.read().unwrap();
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
    State(state): State<SharedState>,
    Json(request): Json<AttestationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
        s.metrics.verification_requests += 1;
    }

    let verifier = ZkcVerifier::new();
    let (attestation_type, result) = match request {
        AttestationRequest::Ecdsa(a) => ("ECDSA", verifier.verify(&a)),
        AttestationRequest::Schnorr(a) => ("Schnorr", verifier.verify_schnorr(&a)),
        AttestationRequest::Zkml(a) => ("ZKML", verifier.verify_zkml(&a)),
        AttestationRequest::BitVm(a) => ("BitVM", verifier.verify_bitvm(&a)),
    };

    info!(
        "Processing {} attestation verification request",
        attestation_type
    );

    match result {
        Ok(valid) => {
            {
                let mut s = state.write().unwrap();
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
                let mut s = state.write().unwrap();
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
    State(state): State<SharedState>,
    Json(request): Json<GcpTokenRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
    }

    let manager = IdentityManager::new();
    match manager.exchange_token(&request).await {
        Ok(token) => Ok(Json(
            json!({ "access_token": token, "token_type": "Bearer", "expires_in": 3600 }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn generate_iso_payment(
    State(_state): State<SharedState>,
    Json(payload): Json<Value>,
) -> Result<String, (StatusCode, Json<Value>)> {
    let sender = payload["sender"].as_str().unwrap_or("CONXIAN-SENDER");
    let receiver = payload["receiver"]
        .as_str()
        .unwrap_or("INSTITUTIONAL-RECEIVER");
    let amount = payload["amount"].as_f64().unwrap_or(0.0);

    let verifier = ZkcVerifier::new();
    Ok(verifier.format_iso20022_pacs008(sender, receiver, amount))
}

/// Industry Enhancement: Create Fiat On-Ramp Session (CON-36/CON-41).
pub async fn create_fiat_session(
    State(state): State<SharedState>,
    Json(request): Json<OnRampSessionRequest>,
) -> Result<Json<OnRampSessionResponse>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
    }

    // In production, these would come from config/env
    let router = FiatRouter::new(
        "ramp-api-key".to_string(),
        "investec-client-id".to_string(),
        "investec-secret".to_string(),
        "alchemypay-app-id".to_string(),
        "alchemypay-secret".to_string(),
        "banxa-api-key".to_string(),
        "banxa-secret".to_string(),
    );

    match router.create_session(request).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// Industry Enhancement: Verify Fiat Webhook (CON-35/CON-41).
pub async fn verify_fiat_webhook(
    State(state): State<SharedState>,
    Json(payload): Json<WebhookPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
    }

    let router = FiatRouter::new(
        "ramp-api-key".to_string(),
        "investec-client-id".to_string(),
        "investec-secret".to_string(),
        "alchemypay-app-id".to_string(),
        "alchemypay-secret".to_string(),
        "banxa-api-key".to_string(),
        "banxa-secret".to_string(),
    );

    match router.verify_webhook(&payload, "shared-secret") {
        Ok(valid) => Ok(Json(
            json!({ "valid": valid, "provider": payload.provider }),
        )),
        Err(e) => Err((
            StatusCode::UNAUTHORIZED, // 403 or 401
            Json(json!({ "error": e.to_string() })),
        )),
    }
}
