use crate::m2m_auth::{M2mAuthPolicy, VerifiedM2mIdentity};
use axum::{
    extract::Request,
    http::{header, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use std::time::Instant;
use subtle::ConstantTimeEq;
use tracing::warn;

#[derive(Debug)]
enum AuthenticatedCredential {
    Legacy,
    M2m(VerifiedM2mIdentity),
}

/// Bearer authentication for the Gateway's private routes.
///
/// Legacy credentials remain privileged only when the selected mode explicitly
/// permits them and the presented value exactly matches the configured token.
/// M2M JWTs additionally require a method/path scope from the route matrix below.
/// Unknown protected routes deny JWT credentials until their scope is defined.
pub async fn auth_middleware(
    mut req: Request,
    next: Next,
    expected_token: String,
    policy: Arc<M2mAuthPolicy>,
    server_start: Instant,
    token_ttl_seconds: Option<u64>,
) -> Result<Response, StatusCode> {
    if !policy.configuration_valid() {
        warn!(
            mode = ?policy.mode(),
            "M2M authentication mode is invalid or missing required JWT configuration; denying protected request"
        );
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            warn!("Unauthorized request: Missing Bearer credential");
            StatusCode::UNAUTHORIZED
        })?;

    match authenticate_bearer(auth, &expected_token, &policy)? {
        AuthenticatedCredential::Legacy => {
            if let Some(ttl) = token_ttl_seconds {
                if server_start.elapsed().as_secs() > ttl {
                    warn!(
                        elapsed = server_start.elapsed().as_secs(),
                        ttl = ttl,
                        "Legacy API token has exceeded maximum age; rejecting request"
                    );
                    return Err(StatusCode::UNAUTHORIZED);
                }
            }
            Ok(next.run(req).await)
        }
        AuthenticatedCredential::M2m(identity) => {
            let required_scope = required_scope_for_request(req.method(), req.uri().path())
                .ok_or_else(|| {
                    warn!(
                        method = %req.method(),
                        path = %req.uri().path(),
                        service = %identity.service_id,
                        "M2M request denied: protected route has no scope mapping"
                    );
                    StatusCode::FORBIDDEN
                })?;

            if !identity.has_scope(required_scope) {
                warn!(
                    method = %req.method(),
                    path = %req.uri().path(),
                    service = %identity.service_id,
                    required_scope = required_scope,
                    "M2M request denied: insufficient route scope"
                );
                return Err(StatusCode::FORBIDDEN);
            }

            tracing::debug!(
                service = %identity.service_id,
                required_scope = required_scope,
                "M2M request authenticated and authorized"
            );
            req.extensions_mut().insert(identity);
            Ok(next.run(req).await)
        }
    }
}

/// Authenticate one Bearer credential. In dual mode, the explicit precedence is:
/// exact configured legacy-token match first; otherwise the credential must be
/// a valid M2M JWT. An invalid JWT never falls through to legacy comparison.
fn authenticate_bearer(
    authorization: &str,
    expected_token: &str,
    policy: &M2mAuthPolicy,
) -> Result<AuthenticatedCredential, StatusCode> {
    if !policy.configuration_valid() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let prefix = authorization
        .get(..7)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !prefix.eq_ignore_ascii_case("bearer ") {
        warn!("Unauthorized request: Malformed Bearer credential");
        return Err(StatusCode::UNAUTHORIZED);
    }
    let provided = &authorization[7..];
    let mode = policy.mode();

    let legacy_usable = mode.accepts_legacy()
        && !expected_token.is_empty()
        && expected_token != "sentinel_prod_api_token"
        && expected_token != format!("{}{}", "sentinel", "api_token")
        && expected_token != "sentinel_default_token";

    if mode.accepts_legacy() && !legacy_usable {
        warn!("API_TOKEN is insecure or not set. Rejecting legacy-authenticated private requests.");
    }

    if legacy_usable
        && provided
            .as_bytes()
            .ct_eq(expected_token.as_bytes())
            .unwrap_u8()
            == 1
    {
        return Ok(AuthenticatedCredential::Legacy);
    }

    if mode.accepts_jwt() {
        return policy
            .verify_bearer(authorization)
            .map(AuthenticatedCredential::M2m)
            .map_err(|error| {
                warn!(code = error.code(), "Unauthorized request: invalid M2M JWT");
                StatusCode::UNAUTHORIZED
            });
    }

    warn!("Unauthorized request: Invalid Bearer credential");
    Err(StatusCode::UNAUTHORIZED)
}

fn normalized_route_path(path: &str) -> &str {
    for prefix in ["/api/v1", "/admin/v1"] {
        if path == prefix {
            return "/";
        }
        if let Some(rest) = path.strip_prefix(prefix) {
            if rest.starts_with('/') {
                return rest;
            }
        }
    }
    path
}

fn is_chain_route(path: &str, action: &str) -> bool {
    let mut segments = path.strip_prefix('/').unwrap_or(path).split('/');
    match (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) {
        (Some("chains"), Some(chain), Some(route_action), None) => {
            !chain.is_empty() && route_action == action
        }
        _ => false,
    }
}

/// Required M2M scope for every route protected by the private, admin, and ALEX
/// bearer-auth middleware. Keep this matrix in sync with routes.rs. Returning
/// None is an intentional deny-by-default for any new or unrecognised route.
fn required_scope_for_request(method: &Method, raw_path: &str) -> Option<&'static str> {
    let path = normalized_route_path(raw_path);

    match (method.as_str(), path) {
        ("GET", "/capabilities" | "/state") => Some("read:admin"),
        ("GET", "/metrics") => Some("read:metrics"),
        ("GET", "/treasury/syi" | "/alex/quote") => Some("read:treasury"),
        ("GET", "/bitcoin/mempool/telemetry" | "/bitcoin/core/shadow-observation") => {
            Some("read:telemetry")
        }
        ("GET", "/settlements/external" | "/chains/list") => Some("read:settlement"),
        ("GET", "/handoff/status") => Some("read:governance"),

        ("POST", "/releases/request-approval" | "/releases/decision") => Some("write:admin"),
        ("POST", "/governance/decision" | "/identity/exchange" | "/handoff/update") => {
            Some("write:governance")
        }
        ("POST", "/bounties/payouts/toggle") => Some("write:treasury"),
        ("POST", "/musig2/aggregate-keys") => Some("admin:secrets"),
        ("POST", "/a2p/otp" | "/a2p/verify") => Some("write:admin"),

        ("POST", "/verify" | "/identity/resolve" | "/identity/resolve/machine" | "/verify/worldcoin") => {
            Some("read:governance")
        }
        ("POST", "/canton/cbtc/verify" | "/canton/state/translate" | "/alex/prepare") => {
            Some("read:settlement")
        }
        ("POST", "/rwa/machine/verify-revenue") => Some("read:treasury"),
        _ => None,
    }
    .or_else(|| {
        if method == Method::GET && is_chain_route(path, "height") {
            Some("read:settlement")
        } else if method == Method::POST && is_chain_route(path, "verify") {
            Some("read:settlement")
        } else if method == Method::POST && is_chain_route(path, "prepare") {
            Some("write:settlement")
        } else if method == Method::POST
            && matches!(
                path,
                "/fiat/session"
                    | "/fiat/webhook"
                    | "/erp/sync"
                    | "/settle"
                    | "/iso20022/payment"
                    | "/iso20022/pain001"
                    | "/iso20022/pacs008"
                    | "/iso20022/pacs009"
                    | "/iso20022/camt053/generate"
                    | "/iso20022/camt054/generate"
                    | "/settlement/papss"
                    | "/settlement/brics"
                    | "/settlement/cips"
                    | "/settlement/spfs"
                    | "/settlement/mbridge"
                    | "/ingress/iso20022"
                    | "/ingress/pain001"
                    | "/ingress/papss"
                    | "/ingress/brics"
                    | "/ingress/cips"
                    | "/ingress/spfs"
                    | "/ingress/mbridge"
                    | "/pos/offline"
                    | "/pos/event"
                    | "/ingress/pos"
                    | "/ingress/edi"
                    | "/supply/edi"
                    | "/invoicing/ubl"
                    | "/ingress/invoice"
                    | "/compliance/zk-kyc"
                    | "/ingress/kyc"
                    | "/pos/sync"
                    | "/dlc/bond"
                    | "/dlc/cet"
                    | "/nwc/relay"
                    | "/m2m/settle"
                    | "/ccip/route"
                    | "/alex/swap"
            )
        {
            Some("write:settlement")
        } else {
            None
        }
    })
}

fn m2m_scope_authorizes(
    identity: &VerifiedM2mIdentity,
    method: &Method,
    path: &str,
) -> bool {
    required_scope_for_request(method, path)
        .is_some_and(|required| identity.has_scope(required))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::m2m_auth::{GatewayAuthMode, M2mJwtConfig};

    fn identity(service_id: &str, scopes: &[&str]) -> VerifiedM2mIdentity {
        VerifiedM2mIdentity {
            service_id: service_id.to_string(),
            scopes: scopes.iter().map(|scope| scope.to_string()).collect(),
        }
    }

    #[test]
    fn route_scope_matrix_covers_every_current_bearer_protected_route() {
        let routes = [
            ("GET", "/api/v1/capabilities"),
            ("GET", "/api/v1/metrics"),
            ("GET", "/api/v1/bitcoin/mempool/telemetry"),
            ("GET", "/api/v1/bitcoin/core/shadow-observation"),
            ("GET", "/api/v1/state"),
            ("GET", "/api/v1/treasury/syi"),
            ("POST", "/api/v1/verify"),
            ("POST", "/api/v1/identity/exchange"),
            ("POST", "/api/v1/identity/resolve"),
            ("POST", "/api/v1/identity/resolve/machine"),
            ("POST", "/api/v1/fiat/session"),
            ("POST", "/api/v1/fiat/webhook"),
            ("POST", "/api/v1/a2p/otp"),
            ("POST", "/api/v1/a2p/verify"),
            ("POST", "/api/v1/erp/sync"),
            ("POST", "/api/v1/settle"),
            ("POST", "/api/v1/iso20022/payment"),
            ("POST", "/api/v1/iso20022/pain001"),
            ("POST", "/api/v1/iso20022/pacs008"),
            ("POST", "/api/v1/iso20022/pacs009"),
            ("POST", "/api/v1/iso20022/camt053/generate"),
            ("POST", "/api/v1/iso20022/camt054/generate"),
            ("POST", "/api/v1/settlement/papss"),
            ("POST", "/api/v1/settlement/brics"),
            ("POST", "/api/v1/settlement/cips"),
            ("POST", "/api/v1/settlement/spfs"),
            ("POST", "/api/v1/settlement/mbridge"),
            ("POST", "/api/v1/ingress/iso20022"),
            ("POST", "/api/v1/ingress/pain001"),
            ("POST", "/api/v1/ingress/papss"),
            ("POST", "/api/v1/ingress/brics"),
            ("POST", "/api/v1/ingress/cips"),
            ("POST", "/api/v1/ingress/spfs"),
            ("POST", "/api/v1/ingress/mbridge"),
            ("GET", "/api/v1/settlements/external"),
            ("POST", "/api/v1/bounties/payouts/toggle"),
            ("POST", "/api/v1/pos/offline"),
            ("POST", "/api/v1/pos/event"),
            ("POST", "/api/v1/ingress/pos"),
            ("POST", "/api/v1/ingress/edi"),
            ("POST", "/api/v1/supply/edi"),
            ("POST", "/api/v1/invoicing/ubl"),
            ("POST", "/api/v1/ingress/invoice"),
            ("POST", "/api/v1/compliance/zk-kyc"),
            ("POST", "/api/v1/ingress/kyc"),
            ("POST", "/api/v1/pos/sync"),
            ("GET", "/api/v1/handoff/status"),
            ("POST", "/api/v1/handoff/update"),
            ("GET", "/api/v1/chains/list"),
            ("GET", "/api/v1/chains/bitcoin/height"),
            ("POST", "/api/v1/chains/bitcoin/prepare"),
            ("POST", "/api/v1/chains/bitcoin/verify"),
            ("POST", "/api/v1/canton/cbtc/verify"),
            ("POST", "/api/v1/dlc/bond"),
            ("POST", "/api/v1/dlc/cet"),
            ("POST", "/api/v1/musig2/aggregate-keys"),
            ("POST", "/api/v1/verify/worldcoin"),
            ("POST", "/api/v1/nwc/relay"),
            ("POST", "/api/v1/m2m/settle"),
            ("POST", "/api/v1/canton/state/translate"),
            ("POST", "/api/v1/ccip/route"),
            ("POST", "/api/v1/rwa/machine/verify-revenue"),
            ("GET", "/api/v1/alex/quote"),
            ("POST", "/api/v1/alex/prepare"),
            ("POST", "/api/v1/alex/swap"),
            ("POST", "/admin/v1/releases/request-approval"),
            ("POST", "/admin/v1/releases/decision"),
            ("POST", "/admin/v1/governance/decision"),
        ];

        for (method, path) in routes {
            let method = Method::from_bytes(method.as_bytes()).unwrap();
            assert!(
                required_scope_for_request(&method, path).is_some(),
                "missing M2M scope mapping for {method} {path}"
            );
        }
    }

    #[test]
    fn gateway_read_only_identity_cannot_mutate_release_governance_or_settlement() {
        let gateway = identity(
            "gateway",
            &[
                "read:admin",
                "read:governance",
                "read:treasury",
                "read:metrics",
                "m2m:internal",
            ],
        );

        assert!(!m2m_scope_authorizes(
            &gateway,
            &Method::POST,
            "/admin/v1/releases/decision"
        ));
        assert!(!m2m_scope_authorizes(
            &gateway,
            &Method::POST,
            "/admin/v1/governance/decision"
        ));
        assert!(!m2m_scope_authorizes(
            &gateway,
            &Method::POST,
            "/api/v1/settle"
        ));
    }

    #[test]
    fn write_scopes_are_operation_specific() {
        let admin = identity(
            "admin-dashboard",
            &[
                "write:admin",
                "write:governance",
                "write:treasury",
                "write:settlement",
                "admin:secrets",
                "m2m:internal",
            ],
        );
        assert!(m2m_scope_authorizes(
            &admin,
            &Method::POST,
            "/admin/v1/releases/decision"
        ));
        assert!(m2m_scope_authorizes(
            &admin,
            &Method::POST,
            "/admin/v1/governance/decision"
        ));
        assert!(m2m_scope_authorizes(
            &admin,
            &Method::POST,
            "/api/v1/settle"
        ));
        assert!(!m2m_scope_authorizes(
            &identity("nexus", &["write:governance", "m2m:internal"]),
            &Method::POST,
            "/admin/v1/releases/decision"
        ));
        assert!(!m2m_scope_authorizes(
            &identity("admin-dashboard", &["write:admin", "m2m:internal"]),
            &Method::POST,
            "/admin/v1/governance/decision"
        ));
        assert!(!m2m_scope_authorizes(
            &identity("admin-dashboard", &["write:settlement", "m2m:internal"]),
            &Method::POST,
            "/api/v1/bounties/payouts/toggle"
        ));
    }

    #[test]
    fn unknown_routes_and_method_mismatches_deny_m2m() {
        let admin = identity(
            "admin-dashboard",
            &["write:admin", "write:settlement", "m2m:internal"],
        );
        assert!(!m2m_scope_authorizes(
            &admin,
            &Method::POST,
            "/api/v1/new-unmapped-route"
        ));
        assert!(!m2m_scope_authorizes(
            &admin,
            &Method::GET,
            "/api/v1/settle"
        ));
    }

    #[test]
    fn dual_mode_has_explicit_legacy_precedence_without_invalid_jwt_fallback() {
        let secret = "x".repeat(32);
        let config = M2mJwtConfig::new(
            &secret,
            "conxian-platform".to_string(),
            "conxian-gateway".to_string(),
            30,
        )
        .unwrap();
        let policy = M2mAuthPolicy::new_for_test(GatewayAuthMode::Dual, Some(config));

        assert!(matches!(
            authenticate_bearer("Bearer legacy-token", "legacy-token", &policy),
            Ok(AuthenticatedCredential::Legacy)
        ));
        assert!(matches!(
            authenticate_bearer(
                "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJnYXRld2F5In0.invalid",
                "legacy-token",
                &policy
            ),
            Err(StatusCode::UNAUTHORIZED)
        ));
    }

    #[test]
    fn dual_mode_with_missing_jwt_configuration_rejects_even_legacy_credentials() {
        let policy = M2mAuthPolicy::new_for_test(GatewayAuthMode::Dual, None);
        assert!(matches!(
            authenticate_bearer("Bearer legacy-token", "legacy-token", &policy),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        ));
    }
}
