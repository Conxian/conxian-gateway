use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::warn;

pub async fn auth_middleware(
    req: Request,
    next: Next,
    expected_token: String,
) -> Result<Response, StatusCode> {
    // Insecure token check - must not be the sentinel or empty in production
    if expected_token.is_empty() || expected_token == "CHANGEME_API_TOKEN" {
        warn!("API_TOKEN is insecure or not set. Rejecting all private requests.");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match auth_header {
        Some(auth) if auth.starts_with("Bearer ") && auth[7..] == expected_token => {
            Ok(next.run(req).await)
        }
        _ => {
            warn!("Unauthorized request: Invalid or missing Bearer token");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
