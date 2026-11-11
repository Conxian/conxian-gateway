use crate::m2m_auth::M2mAuthPolicy;
use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use std::time::Instant;
use subtle::ConstantTimeEq;
use tracing::warn;

/// Bearer authentication for the Gateway's private routes.
///
/// Credential kinds accepted depend on `policy`:
///   - `legacy`: the configured static `API_TOKEN` only (previous behaviour).
///   - `dual`:   the static token OR a valid M2M JWT.
///   - `jwt`:    a valid M2M JWT only.
///
/// Precedence follows the organization M2M contract: a present-but-invalid
/// `Authorization` header is **never** allowed to fall through to the legacy
/// token, so a bad Bearer cannot be rescued by another credential. JWT
/// verification is fail-closed in `m2m_auth`.
pub async fn auth_middleware(
    req: Request,
    next: Next,
    expected_token: String,
    policy: Arc<M2mAuthPolicy>,
    server_start: Instant,
    token_ttl_seconds: Option<u64>,
) -> Result<Response, StatusCode> {
    let mode = policy.mode();

    // The legacy token must be usable and non-sentinel before we accept it, and
    // only when the mode still permits legacy auth.
    let legacy_usable = mode.accepts_legacy()
        && !expected_token.is_empty()
        && expected_token != "sentinel_prod_api_token"
        && expected_token != format!("{}{}", "sentinel", "api_token")
        && expected_token != "sentinel_default_token";

    if mode.accepts_legacy() && !legacy_usable {
        warn!("API_TOKEN is insecure or not set. Rejecting legacy-authenticated private requests.");
    }

    // Token age/expiry enforcement (CON-1276), legacy path only.
    if legacy_usable {
        if let Some(ttl) = token_ttl_seconds {
            if server_start.elapsed().as_secs() > ttl {
                warn!(
                    elapsed = server_start.elapsed().as_secs(),
                    ttl = ttl,
                    "Token has exceeded maximum age; rejecting request"
                );
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let Some(auth) = auth_header else {
        warn!("Unauthorized request: Missing Bearer credential");
        return Err(StatusCode::UNAUTHORIZED);
    };

    // A Bearer header is present: it MUST authenticate. There is no fall-through
    // to another credential when it is malformed or invalid.
    if !auth
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
    {
        warn!("Unauthorized request: Malformed Bearer credential");
        return Err(StatusCode::UNAUTHORIZED);
    }

    // 1. M2M JWT (when the mode accepts it).
    if mode.accepts_jwt() {
        match policy.verify_bearer(auth) {
            Ok(identity) => {
                tracing::debug!(service = %identity.service_id, "M2M JWT authenticated");
                return Ok(next.run(req).await);
            }
            Err(error) => {
                // In `jwt` mode there is no legacy fallback: fail now.
                if !mode.accepts_legacy() {
                    warn!(code = error.code(), "Unauthorized request: invalid M2M JWT");
                    return Err(StatusCode::UNAUTHORIZED);
                }
                // In `dual` mode, fall through to the legacy comparison.
            }
        }
    }

    // 2. Legacy static token (when the mode accepts it).
    if legacy_usable {
        let provided_token = &auth[7..];
        if provided_token
            .as_bytes()
            .ct_eq(expected_token.as_bytes())
            .unwrap_u8()
            == 1
        {
            return Ok(next.run(req).await);
        }
    }

    warn!("Unauthorized request: Invalid Bearer credential");
    Err(StatusCode::UNAUTHORIZED)
}
