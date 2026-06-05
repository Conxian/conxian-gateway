use crate::AppState;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::info;

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
