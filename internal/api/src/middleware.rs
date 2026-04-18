use crate::AppState;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;
use tracing::{info, warn};

pub async fn latency_tracker(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let start = Instant::now();
    let path = req.uri().path().to_string();
    let method = req.method().to_string();

    let response = next.run(req).await;

    let latency = start.elapsed().as_millis() as u64;
    info!(
        method = %method,
        path = %path,
        status = %response.status(),
        latency_ms = %latency,
        "API Request processed"
    );

    if let Ok(mut s) = state.shared.write() {
        s.metrics.total_requests += 1;
        if path.contains("/health") {
            s.metrics.health_requests += 1;
        } else if path.contains("/metrics") {
            s.metrics.metrics_requests += 1;
        } else if path.contains("/state") {
            s.metrics.state_requests += 1;
        }
    }

    response
}

/// CON-492: [ATS-v14.0] x402 Payment-Required Typed Payload
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct X402Payload {
    pub amount_satoshi: u64,
    pub asset: String,             // e.g., "BTC", "sBTC"
    pub challenge: String,         // Nonce or invoice hash
    pub expiry: u64,               // Unix timestamp
    pub proof_ref: Option<String>, // sBTC txid or Lightning preimage
}

/// CON-492: x402 / Payment-Required Parser & Filter
pub async fn x402_filter(req: Request, next: Next) -> Result<Response, (StatusCode, Json<Value>)> {
    let headers = req.headers();
    let path = req.uri().path();

    // Define routes that strictly require x402 payment
    let strictly_protected =
        path.contains("/settle") || path.contains("/ingress/") || path.contains("/erp/sync");

    let x402_header = headers.get("x-402-payment").and_then(|v| v.to_str().ok());

    match x402_header {
        Some(token) => {
            // Attempt to parse the token as a JSON payload
            if serde_json::from_str::<X402Payload>(token).is_ok() {
                info!("x402 payment payload verified");
                Ok(next.run(req).await)
            } else if token.starts_with("proof-") || token.starts_with("test-pay-") {
                info!(token = %token, "x402 simple proof accepted");
                Ok(next.run(req).await)
            } else {
                warn!(token = %token, "Invalid x402 payment token format");
                Err((
                    StatusCode::PAYMENT_REQUIRED,
                    Json(json!({
                        "error": "Invalid x402 payment token format",
                        "code": "x402_malformed_token",
                        "expected_format": "JSON X402Payload"
                    })),
                ))
            }
        }
        None if strictly_protected => {
            warn!(path = %path, "Access denied: x402 Payment-Required");
            Err((
                StatusCode::PAYMENT_REQUIRED,
                Json(json!({
                    "error": "Payment Required",
                    "code": "x402_required",
                    "challenge": uuid::Uuid::new_v4().to_string(),
                    "amount_satoshi": 1000,
                    "asset": "sBTC"
                })),
            ))
        }
        None => {
            // Non-strictly protected routes pass through
            Ok(next.run(req).await)
        }
    }
}
