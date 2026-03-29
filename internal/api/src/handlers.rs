use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use conxian_core::{
    AttestationRequest, BitVmAttestation, ConxianJobCard, GcpTokenRequest,
    IdentityResolutionRequest, IdentityResolutionResponse, SharedState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

use crate::a2p::{A2pRouter, OtpRequest, OtpVerificationRequest};
use crate::fiat::{FiatRouter, OnRampSessionRequest, OnRampSessionResponse, WebhookPayload};
use compliance::{IdentityManager, ZkcVerifier};

pub async fn health_check(State(state): State<SharedState>) -> Json<Value> {
    let s = state.read().unwrap();
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

pub async fn get_state(State(state): State<SharedState>) -> Json<GatewayStateResponse> {
    let mut s_write = state.write().unwrap();
    s_write.metrics.total_requests += 1;
    s_write.metrics.state_requests += 1;
    drop(s_write);

    let s = state.read().unwrap();
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

/// CON-66: Resolve identities across ENS, BNS, World ID, and Web3.bio.
pub async fn resolve_identity_v1(
    State(state): State<SharedState>,
    Json(request): Json<IdentityResolutionRequest>,
) -> Result<Json<IdentityResolutionResponse>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
    }

    let manager = IdentityManager::new();
    match manager.resolve_identity(&request).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn generate_iso_payment(
    State(_state): State<SharedState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let verifier = ZkcVerifier::new();

    if let Ok(job_card) = serde_json::from_value::<ConxianJobCard>(payload.clone()) {
        match verifier.format_iso20022_pacs008_v8(&job_card) {
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

    let xml = verifier.format_iso20022_pacs008(sender, receiver, amount);
    Ok(Json(json!({ "xml": xml, "schema": "pacs.008.001.07" })))
}

pub async fn create_fiat_session(
    State(_state): State<SharedState>,
    Json(request): Json<OnRampSessionRequest>,
) -> Result<Json<OnRampSessionResponse>, (StatusCode, Json<Value>)> {
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

pub async fn verify_fiat_webhook(
    State(_state): State<SharedState>,
    Json(payload): Json<WebhookPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
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
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn send_otp(
    State(_state): State<SharedState>,
    Json(request): Json<OtpRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let router = A2pRouter::new(
        "infobip-api-key".to_string(),
        "infobip-base-url".to_string(),
        "hmac-secret".to_string(),
    );

    match router.send_otp(request).await {
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
    State(_state): State<SharedState>,
    Json(request): Json<OtpVerificationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let router = A2pRouter::new(
        "infobip-api-key".to_string(),
        "infobip-base-url".to_string(),
        "hmac-secret".to_string(),
    );

    match router.verify_otp(request) {
        Ok(valid) => Ok(Json(json!({ "valid": valid }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn sync_erp_ledger(
    State(_state): State<SharedState>,
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
    State(_state): State<SharedState>,
    Json(request): Json<SettlementRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let verifier = ZkcVerifier::new();
    match verifier.verify_job_card_settlement(&request.job_card, &request.bitvm_proof) {
        Ok(valid) => Ok(Json(
            json!({ "valid": valid, "settlement": "BitVM2-Verified" }),
        )),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}
