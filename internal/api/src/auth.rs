use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;
use tracing::warn;

pub async fn auth_middleware(
    req: Request,
    next: Next,
    expected_token: String,
) -> Result<Response, StatusCode> {
    // CON-492: Enhanced API token security standards.
    // Minimum 32-character requirement and prohibited sentinel/insecure values.
    if expected_token.len() < 32
        || expected_token == "REQUIRED_FOR_PROD_API_TOKEN"
        || expected_token == "institutional-default-token"
        || expected_token.to_lowercase().contains("changeme")
    {
        warn!(
            token_len = expected_token.len(),
            "Rejecting private request: API_TOKEN is weak, insecure, or matches a prohibited sentinel value."
        );
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match auth_header {
        Some(auth) if auth.starts_with("Bearer ") => {
            let provided_token = &auth[7..];
            // Cryptographic constant-time comparison to prevent timing attacks
            if provided_token
                .as_bytes()
                .ct_eq(expected_token.as_bytes())
                .unwrap_u8()
                == 1
            {
                Ok(next.run(req).await)
            } else {
                warn!("Unauthorized request: Invalid Bearer token");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => {
            warn!("Unauthorized request: Missing or malformed Bearer token");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
