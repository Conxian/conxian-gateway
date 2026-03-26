use axum::{
    body::Body,
    extract::State,
    http::{Request, Response},
    middleware::Next,
};
use conxian_core::SharedState;
use std::time::Instant;
use tracing::info;

pub async fn latency_tracker(
    State(_state): State<SharedState>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let start = Instant::now();
    let path = req.uri().path().to_string();
    let method = req.method().to_string();

    let response = next.run(req).await;

    let latency = start.elapsed();
    info!(
        method = %method,
        path = %path,
        latency = ?latency,
        status = %response.status(),
        "Request processed"
    );

    response
}
