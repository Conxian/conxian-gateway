//! Multi-tenant API-key authentication, Fusion JWT verification, and the
//! combined managed-auth middleware for the Managed SaaS Gateway surface.
//!
//! The managed surface (`https://api.conxian-labs.com/v1/agent`) accepts one of:
//!   * a managed API key (`cxn_agent_…`) minted by the Merchant-of-Record
//!     billing webhook and stored in Neon Postgres (mirrored to env for the
//!     gateway process), or
//!   * a short-lived Fusion JWT (HS256) issued by the same billing flow.
//!
//! Both resolve to a `TenantContext` (tenant id + tier) which feeds per-tenant
//! rate limiting and Sentinel secret filtering. The legacy institutional
//! `API_TOKEN` remains accepted for administrative routes only.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tracing::warn;

use crate::rate_limit::RateLimiter;
use crate::sentinel::Sentinel;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_RATE_LIMIT_PER_MINUTE: u32 = 120;

#[cfg(test)]
const FUSION_JWT_TTL_SECONDS: u64 = 300;

/// A resolved tenant identity for an authenticated managed subscriber.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantContext {
    pub tenant_id: String,
    pub tier: String,
}

/// Minimal HS256 JWT verifier for Fusion-issued short-lived tokens.
#[derive(Clone)]
pub struct FusionJwt {
    secret: Option<String>,
}

#[derive(Debug)]
pub enum JwtError {
    Malformed,
    BadSignature,
    Expired,
    MissingClaims,
}

#[derive(Deserialize)]
struct FusionClaims {
    sub: Option<String>,
    tier: Option<String>,
    exp: Option<u64>,
}

impl FusionJwt {
    pub fn new(secret: Option<String>) -> Self {
        Self { secret }
    }

    /// Verify an HS256 JWT and return its tenant context.
    pub fn verify(&self, token: &str) -> Result<TenantContext, JwtError> {
        let secret = self.secret.as_deref().ok_or(JwtError::BadSignature)?;
        if secret.is_empty() {
            return Err(JwtError::BadSignature);
        }

        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(JwtError::Malformed);
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);

        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| JwtError::BadSignature)?;
        mac.update(signing_input.as_bytes());
        let expected = mac.finalize().into_bytes();

        let provided = URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|_| JwtError::BadSignature)?;

        if expected.len() != provided.len() || expected.as_slice().ct_eq(&provided).unwrap_u8() != 1
        {
            return Err(JwtError::BadSignature);
        }

        let payload = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| JwtError::Malformed)?;
        let claims: FusionClaims =
            serde_json::from_slice(&payload).map_err(|_| JwtError::Malformed)?;

        let tenant_id = claims.sub.ok_or(JwtError::MissingClaims)?;
        let tier = claims.tier.unwrap_or_else(|| "managed".to_string());

        if let Some(exp) = claims.exp {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if exp <= now {
                return Err(JwtError::Expired);
            }
        }

        Ok(TenantContext { tenant_id, tier })
    }

    /// Issue a token (test/offline helper; the billing webhook is the issuer in
    /// production).
    #[cfg(test)]
    fn issue(&self, tenant_id: &str, tier: &str, ttl_seconds: u64) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as E;
        let header = E.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let claims = format!(
            r#"{{"sub":"{}","tier":"{}","exp":{}}}"#,
            tenant_id,
            tier,
            now + ttl_seconds
        );
        let payload = E.encode(claims.as_bytes());
        let signing_input = format!("{}.{}", header, payload);
        let mut mac =
            HmacSha256::new_from_slice(self.secret.as_deref().unwrap().as_bytes()).unwrap();
        mac.update(signing_input.as_bytes());
        let sig = E.encode(mac.finalize().into_bytes());
        format!("{}.{}.{}", header, payload, sig)
    }
}

/// Registry of managed API keys (key -> tenant context) plus the legacy token.
#[derive(Clone)]
pub struct TenantKeyRegistry {
    keys: Arc<HashMap<String, TenantContext>>,
    legacy_token: Option<String>,
}

impl TenantKeyRegistry {
    pub fn from_env(legacy_token: &str) -> Self {
        let mut keys = HashMap::new();
        if let Ok(raw) = std::env::var("MANAGED_API_KEYS") {
            // Expected shape: {"<api_key>": "<tenant_id>"}
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&raw) {
                for (api_key, tenant_id) in map {
                    if !api_key.is_empty() && !tenant_id.is_empty() {
                        keys.insert(
                            api_key,
                            TenantContext {
                                tenant_id,
                                tier: "managed".to_string(),
                            },
                        );
                    }
                }
            }
        }

        let legacy_token = if legacy_token.is_empty() {
            None
        } else {
            Some(legacy_token.to_string())
        };

        Self {
            keys: Arc::new(keys),
            legacy_token,
        }
    }

    /// Resolve a bearer token to a tenant context, checking managed keys first
    /// then the legacy institutional token.
    pub fn authenticate(&self, bearer: &str) -> Option<TenantContext> {
        if let Some(ctx) = self.keys.get(bearer) {
            return Some(ctx.clone());
        }
        if let Some(legacy) = self.legacy_token.as_deref() {
            if !legacy.is_empty() && bearer.as_bytes().ct_eq(legacy.as_bytes()).unwrap_u8() == 1 {
                return Some(TenantContext {
                    tenant_id: "institutional".to_string(),
                    tier: "strict".to_string(),
                });
            }
        }
        None
    }

    /// All live secret values held by this registry (for Sentinel redaction).
    pub fn secret_values(&self) -> Vec<String> {
        let mut out: Vec<String> = self.keys.keys().cloned().collect();
        if let Some(legacy) = self.legacy_token.clone() {
            out.push(legacy);
        }
        out
    }
}

/// Combined managed-auth state: registry + fusion JWT + rate limiter + sentinel.
#[derive(Clone)]
pub struct ManagedAuth {
    registry: TenantKeyRegistry,
    fusion: FusionJwt,
    limiter: RateLimiter,
    sentinel: Sentinel,
}

impl ManagedAuth {
    pub fn from_env(legacy_token: &str) -> Self {
        let registry = TenantKeyRegistry::from_env(legacy_token);
        let fusion_secret = std::env::var("FUSION_JWT_SECRET")
            .ok()
            .filter(|s| !s.is_empty());
        let fusion = FusionJwt::new(fusion_secret);

        let limit = std::env::var("RATE_LIMIT_REQUESTS_PER_MINUTE")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(DEFAULT_RATE_LIMIT_PER_MINUTE);
        let limiter = RateLimiter::new(limit, Duration::from_secs(60));

        let sentinel = Sentinel::new(registry.secret_values());

        Self {
            registry,
            fusion,
            limiter,
            sentinel,
        }
    }

    /// Authenticate a request, preferring a Fusion JWT then falling back to a
    /// managed API key or the legacy institutional token.
    fn authenticate_request(&self, req: &Request) -> Option<TenantContext> {
        let header = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
        let bearer = header.strip_prefix("Bearer ")?;

        if let Ok(ctx) = self.fusion.verify(bearer) {
            return Some(ctx);
        }
        self.registry.authenticate(bearer)
    }
}

/// Axum middleware enforcing managed-subscriber authentication + rate limiting.
pub async fn managed_auth_middleware(
    State(auth): State<ManagedAuth>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let tenant = match auth.authenticate_request(&req) {
        Some(t) => t,
        None => {
            warn!(
                path = %auth.sentinel.redact(req.uri().path()),
                "Managed request rejected: invalid or missing API key/JWT"
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    if !auth.limiter.check(&tenant.tenant_id) {
        warn!(
            tenant = %auth.sentinel.redact(&tenant.tenant_id),
            "Managed request rate-limited"
        );
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `std::env::set_var`/`remove_var` are process-global, so tests that mutate
    // MANAGED_API_KEYS must run serially to avoid racing each other.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn fusion_jwt_round_trips_and_verifies() {
        let fusion = FusionJwt::new(Some("test-fusion-secret".to_string()));
        let token = fusion.issue("tenant-42", "managed", FUSION_JWT_TTL_SECONDS);
        let ctx = fusion.verify(&token).expect("valid token");
        assert_eq!(ctx.tenant_id, "tenant-42");
        assert_eq!(ctx.tier, "managed");
    }

    #[test]
    fn fusion_jwt_rejects_bad_signature() {
        let fusion = FusionJwt::new(Some("test-fusion-secret".to_string()));
        let token = fusion.issue("tenant-42", "managed", FUSION_JWT_TTL_SECONDS);
        // Corrupt the signature segment.
        let mut parts: Vec<&str> = token.split('.').collect();
        parts[2] = "AAAA";
        assert!(matches!(
            fusion.verify(&format!("{}.{}.{}", parts[0], parts[1], parts[2])),
            Err(JwtError::BadSignature)
        ));
    }

    #[test]
    fn fusion_jwt_rejects_expired_token() {
        let fusion = FusionJwt::new(Some("test-fusion-secret".to_string()));
        let token = fusion.issue("tenant-42", "managed", 0); // exp = now
        assert!(matches!(fusion.verify(&token), Err(JwtError::Expired)));
    }

    #[test]
    fn registry_authenticates_managed_key_and_legacy_token() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("MANAGED_API_KEYS", r#"{"cxn_agent_abc":"tenant-7"}"#);
        let registry = TenantKeyRegistry::from_env("legacy-institutional-token");
        let managed = registry.authenticate("cxn_agent_abc").expect("managed key");
        assert_eq!(managed.tenant_id, "tenant-7");
        assert_eq!(managed.tier, "managed");

        let legacy = registry
            .authenticate("legacy-institutional-token")
            .expect("legacy");
        assert_eq!(legacy.tenant_id, "institutional");

        assert!(registry.authenticate("unknown-key").is_none());
        std::env::remove_var("MANAGED_API_KEYS");
    }

    #[test]
    fn sentinel_redacts_live_managed_keys() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("MANAGED_API_KEYS", r#"{"cxn_agent_secret_999":"tenant-9"}"#);
        let registry = TenantKeyRegistry::from_env("legacy");
        let sentinel = Sentinel::new(registry.secret_values());
        let cleaned = sentinel.redact("Authorization: Bearer cxn_agent_secret_999");
        assert!(!cleaned.contains("cxn_agent_secret_999"));
        assert!(cleaned.contains("[REDACTED]"));
        std::env::remove_var("MANAGED_API_KEYS");
    }
}
