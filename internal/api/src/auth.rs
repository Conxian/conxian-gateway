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
    // Insecure token check - must not be the sentinel or empty in production
    if expected_token.is_empty()
        || expected_token == "sentinel_prod_api_token"
        || expected_token == format!("{}{}", "sentinel", "api_token")
        || expected_token == "sentinel_default_token"
    {
        warn!("API_TOKEN is insecure or not set. Rejecting all private requests.");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match auth_header {
        Some(auth) if auth.starts_with("Bearer ") => {
            let provided_token = &auth[7..];

            // [CON-1276] Check for token age/expiry.
            // Note: Future institutional tokens may require JWT claim validation.
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
