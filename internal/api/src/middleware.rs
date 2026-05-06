use crate::x402::{parse_x402_payload, X402ParseError};
use crate::AppState;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
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

/// CON-492: Unified x402 / Payment-Required Parser & Filter.
/// Utilizes the advanced parser from x402.rs to enforce institutional payment standards.
pub async fn x402_filter(req: Request, next: Next) -> Result<Response, (StatusCode, Json<Value>)> {
    let headers = req.headers();
    let path = req.uri().path();

    // Define routes that strictly require x402 payment
    let strictly_protected =
        path.contains("/settle") || path.contains("/ingress/") || path.contains("/erp/sync");

    match parse_x402_payload(headers) {
        Ok(payload) => {
            info!(
                amount = %payload.amount,
                asset = %payload.asset,
                "x402 payment verified via advanced parser"
            );
            Ok(next.run(req).await)
        }
        Err(X402ParseError::MissingHeader { .. }) if strictly_protected => {
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
        Err(X402ParseError::MissingHeader { .. }) => {
            // Non-strictly protected routes pass through if header is missing
            Ok(next.run(req).await)
        }
        Err(e) => {
            warn!(error = %e, "Invalid x402 payment payload");
            Err((
                e.status_code(),
                Json(json!({
                    "error": e.to_string(),
                    "code": e.code(),
                    "expected_format": "Institutional x402 Standard (ATS-v14.0)"
                })),
            ))
        }
    }
}
