//! M2M (machine-to-machine) JWT verification for the Gateway.
//!
//! Implements the Gateway side of the organization M2M contract defined by
//! `conxius-platform` (`docs/M2M_AUTHENTICATION.md` and the
//! `2026-07-22-issue-1160-m2m-jwt-auth` spec). The platform issues short-lived
//! HS256 service tokens; the Gateway verifies them here so machine callers
//! carry a scoped, per-service, rotatable identity instead of one shared static
//! bearer token.
//!
//! Contract, in exact agreement with the platform verifier:
//!   - `alg` MUST be exactly `HS256`; `typ` MUST be `JWT`; a `kid` header MUST be
//!     absent (this contract uses a single configured secret, not a JWKS).
//!   - Required claims: exact `iss`, exact single-string `aud`, registered
//!     service `sub`, space-delimited `scope`, integer `iat`/`nbf`/`exp`, and a
//!     non-empty `jti`.
//!   - TTL is bounded to 60..900s (default 300); clock skew is bounded to 0..60s
//!     (default 30).
//!   - Scopes MUST be known, non-duplicate, a subset of the service's permission
//!     ceiling, and MUST include `m2m:internal`.
//!   - Verification is fail-closed: any deviation returns an error and never
//!     authorizes a request.
//!
//! This module verifies only. Issuance stays server-side on the platform.

use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Exact algorithm allowed by the M2M contract.
pub const M2M_JWT_ALG: &str = "HS256";
/// Exact type allowed by the M2M contract.
pub const M2M_JWT_TYP: &str = "JWT";
pub const DEFAULT_JWT_TTL_SECONDS: u64 = 300;
pub const MIN_JWT_TTL_SECONDS: u64 = 60;
pub const MAX_JWT_TTL_SECONDS: u64 = 900;
pub const DEFAULT_JWT_CLOCK_SKEW_SECONDS: u64 = 30;
pub const MAX_JWT_CLOCK_SKEW_SECONDS: u64 = 60;
pub const MIN_JWT_SECRET_BYTES: usize = 32;
pub const MAX_BEARER_TOKEN_BYTES: usize = 4096;
pub const MAX_JTI_LENGTH: usize = 128;
pub const M2M_INTERNAL_SCOPE: &str = "m2m:internal";

/// Registered service identities that may hold an M2M JWT. Mirrors
/// `SERVICE_IDS` (excluding `external`, which cannot hold a JWT).
pub const JWT_SERVICE_IDS: &[&str] = &[
    "gateway",
    "elizaos",
    "nexus",
    "orbit",
    "wallet",
    "ui",
    "admin-dashboard",
    "pulse-bos",
];

/// Known scope vocabulary. Mirrors `KNOWN_SCOPES`.
pub const KNOWN_SCOPES: &[&str] = &[
    "read:admin",
    "write:admin",
    "read:governance",
    "write:governance",
    "read:treasury",
    "write:treasury",
    "read:metrics",
    "admin:secrets",
    "admin:deploy",
    "m2m:internal",
    "read:telemetry",
    "read:settlement",
    "write:settlement",
];

/// Per-service permission ceiling. Mirrors `SERVICE_PERMISSIONS`. A token's
/// scopes MUST be a subset of the ceiling for its `sub`.
pub fn service_permissions(service: &str) -> Option<&'static [&'static str]> {
    Some(match service {
        "gateway" => &[
            "read:admin",
            "read:governance",
            "read:treasury",
            "read:metrics",
            "m2m:internal",
        ],
        "elizaos" => &[
            "read:admin",
            "read:governance",
            "read:metrics",
            "m2m:internal",
        ],
        "nexus" => &[
            "read:governance",
            "read:treasury",
            "read:metrics",
            "write:governance",
            "m2m:internal",
        ],
        "orbit" => &["admin:deploy", "m2m:internal"],
        "wallet" => &["read:treasury", "write:governance", "m2m:internal"],
        "ui" => &[
            "read:admin",
            "read:governance",
            "read:treasury",
            "read:metrics",
            "m2m:internal",
        ],
        "admin-dashboard" => &[
            "read:admin",
            "write:admin",
            "read:governance",
            "write:governance",
            "read:treasury",
            "write:treasury",
            "read:metrics",
            "admin:secrets",
            "admin:deploy",
            "m2m:internal",
            "read:telemetry",
            "read:settlement",
            "write:settlement",
        ],
        "pulse-bos" => &[
            "read:admin",
            "read:treasury",
            "read:metrics",
            "m2m:internal",
        ],
        _ => return None,
    })
}

/// Gateway bearer-auth mode, mirroring `M2M_GATEWAY_AUTH_MODE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayAuthMode {
    /// Only the legacy static bearer token is accepted.
    Legacy,
    /// Legacy token OR a valid M2M JWT is accepted.
    Dual,
    /// Only a valid M2M JWT is accepted.
    Jwt,
    /// An explicitly configured but unsupported mode; protected routes deny.
    Invalid,
}

impl GatewayAuthMode {
    pub fn from_env_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "legacy" => Self::Legacy,
            "dual" => Self::Dual,
            "jwt" => Self::Jwt,
            _ => Self::Invalid,
        }
    }

    pub fn accepts_legacy(self) -> bool {
        matches!(self, Self::Legacy | Self::Dual)
    }

    pub fn accepts_jwt(self) -> bool {
        matches!(self, Self::Dual | Self::Jwt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum M2mJwtError {
    /// `Authorization` missing, malformed, or not a bounded `Bearer <token>`.
    MalformedBearer,
    /// Secret missing, too short, or otherwise unusable — fail closed.
    InvalidConfiguration,
    /// Header is not exactly `alg:HS256`, `typ:JWT`, with no `kid`.
    InvalidHeader,
    /// Signature did not verify.
    InvalidSignature,
    /// A required claim is absent or of the wrong type/value.
    InvalidClaims,
    /// `iss` did not match the configured issuer.
    WrongIssuer,
    /// `aud` did not match the configured audience.
    WrongAudience,
    /// `sub` is not a registered JWT service id.
    UnknownService,
    /// `scope` is malformed, unknown, duplicated, or exceeds the ceiling.
    InvalidScope,
    /// `exp`/`nbf`/`iat` outside the bounded lifetime or skew window.
    LifetimeOutOfBounds,
}

impl M2mJwtError {
    /// A stable, non-sensitive code for logging/telemetry. Never echoes input.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MalformedBearer => "m2m_malformed_bearer",
            Self::InvalidConfiguration => "m2m_invalid_configuration",
            Self::InvalidHeader => "m2m_invalid_header",
            Self::InvalidSignature => "m2m_invalid_signature",
            Self::InvalidClaims => "m2m_invalid_claims",
            Self::WrongIssuer => "m2m_wrong_issuer",
            Self::WrongAudience => "m2m_wrong_audience",
            Self::UnknownService => "m2m_unknown_service",
            Self::InvalidScope => "m2m_invalid_scope",
            Self::LifetimeOutOfBounds => "m2m_lifetime_out_of_bounds",
        }
    }
}

/// A verified M2M identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedM2mIdentity {
    pub service_id: String,
    pub scopes: Vec<String>,
}

impl VerifiedM2mIdentity {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|candidate| candidate == scope)
    }
}

#[derive(Debug, Deserialize)]
struct RawHeader {
    alg: Option<String>,
    typ: Option<String>,
    kid: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawClaims {
    iss: Option<String>,
    /// Contract requires an exact single-string audience. A JSON array or any
    /// non-string value fails to deserialize and is rejected as invalid claims.
    aud: Option<String>,
    sub: Option<String>,
    scope: Option<String>,
    iat: Option<i64>,
    nbf: Option<i64>,
    exp: Option<i64>,
    jti: Option<String>,
}

/// Configuration for verifying M2M JWTs, resolved from the environment.
#[derive(Debug, Clone)]
pub struct M2mJwtConfig {
    secret: Vec<u8>,
    issuer: String,
    audience: String,
    clock_skew_seconds: u64,
}

impl M2mJwtConfig {
    /// Build from explicit values, validating the secret length. Returns
    /// `InvalidConfiguration` rather than panicking on a bad secret.
    pub fn new(
        secret: &str,
        issuer: String,
        audience: String,
        clock_skew_seconds: u64,
    ) -> Result<Self, M2mJwtError> {
        let issuer = issuer.trim().to_string();
        let audience = audience.trim().to_string();
        if secret.len() < MIN_JWT_SECRET_BYTES
            || issuer.is_empty()
            || audience.is_empty()
            || clock_skew_seconds > MAX_JWT_CLOCK_SKEW_SECONDS
        {
            return Err(M2mJwtError::InvalidConfiguration);
        }
        Ok(Self {
            secret: secret.as_bytes().to_vec(),
            issuer,
            audience,
            clock_skew_seconds,
        })
    }

    /// Resolve from the environment, mirroring the platform env names. Returns
    /// `None` when JWT is not configured (caller keeps the legacy path).
    pub fn from_env() -> Option<Self> {
        let secret = std::env::var("GATEWAY_JWT_SECRET").ok()?;
        let issuer = match std::env::var("GATEWAY_JWT_ISSUER") {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => "conxian-platform".to_string(),
            Err(_) => return None,
        };
        let audience = match std::env::var("GATEWAY_JWT_AUDIENCE") {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => "conxian-gateway".to_string(),
            Err(_) => return None,
        };
        let skew = match std::env::var("GATEWAY_JWT_CLOCK_SKEW_SECONDS") {
            Ok(value) => value.parse::<u64>().ok()?,
            Err(std::env::VarError::NotPresent) => DEFAULT_JWT_CLOCK_SKEW_SECONDS,
            Err(_) => return None,
        };
        Self::new(secret.trim(), issuer, audience, skew).ok()
    }
}

/// Parse `Authorization: Bearer <token>` with a bounded token size.
pub fn parse_bearer(authorization: &str) -> Result<&str, M2mJwtError> {
    let trimmed = authorization.trim();
    let prefix = trimmed.get(..7).ok_or(M2mJwtError::MalformedBearer)?;
    if !prefix.eq_ignore_ascii_case("bearer ") {
        return Err(M2mJwtError::MalformedBearer);
    }
    let token = trimmed[7..].trim();
    if token.is_empty() || token.len() > MAX_BEARER_TOKEN_BYTES {
        return Err(M2mJwtError::MalformedBearer);
    }
    Ok(token)
}

fn b64url_decode(input: &str) -> Result<Vec<u8>, M2mJwtError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input.as_bytes())
        .map_err(|_| M2mJwtError::InvalidClaims)
}

/// Verify a compact HS256 JWT against the contract. `now_seconds` is injected so
/// the behaviour is deterministic under test.
pub fn verify_m2m_jwt(
    token: &str,
    config: &M2mJwtConfig,
    now_seconds: i64,
) -> Result<VerifiedM2mIdentity, M2mJwtError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(M2mJwtError::InvalidHeader);
    }
    let (header_b64, payload_b64, signature_b64) = (parts[0], parts[1], parts[2]);

    // 1. Header must be exactly HS256/JWT with no `kid`.
    let header: RawHeader = serde_json::from_slice(&b64url_decode(header_b64)?)
        .map_err(|_| M2mJwtError::InvalidHeader)?;
    if header.alg.as_deref() != Some(M2M_JWT_ALG)
        || header.typ.as_deref() != Some(M2M_JWT_TYP)
        || header.kid.is_some()
    {
        return Err(M2mJwtError::InvalidHeader);
    }

    // 2. Signature over the exact signing input, constant-time compared.
    let mut mac = HmacSha256::new_from_slice(&config.secret)
        .map_err(|_| M2mJwtError::InvalidConfiguration)?;
    mac.update(format!("{header_b64}.{payload_b64}").as_bytes());
    let expected = mac.finalize().into_bytes();
    let provided = b64url_decode(signature_b64)?;
    if provided.len() != expected.len()
        || provided.as_slice().ct_eq(expected.as_slice()).unwrap_u8() != 1
    {
        return Err(M2mJwtError::InvalidSignature);
    }

    // 3. Claims.
    let claims: RawClaims = serde_json::from_slice(&b64url_decode(payload_b64)?)
        .map_err(|_| M2mJwtError::InvalidClaims)?;

    if claims.iss.as_deref() != Some(config.issuer.as_str()) {
        return Err(M2mJwtError::WrongIssuer);
    }
    if claims.aud.as_deref() != Some(config.audience.as_str()) {
        return Err(M2mJwtError::WrongAudience);
    }
    let service_id = claims.sub.clone().ok_or(M2mJwtError::InvalidClaims)?;
    if !JWT_SERVICE_IDS.contains(&service_id.as_str()) {
        return Err(M2mJwtError::UnknownService);
    }
    let (iat, nbf, exp) = match (claims.iat, claims.nbf, claims.exp) {
        (Some(iat), Some(nbf), Some(exp)) => (iat, nbf, exp),
        _ => return Err(M2mJwtError::InvalidClaims),
    };
    let jti = claims.jti.clone().ok_or(M2mJwtError::InvalidClaims)?;
    if jti.is_empty()
        || jti.len() > MAX_JTI_LENGTH
        || jti.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(M2mJwtError::InvalidClaims);
    }

    // 4. Lifetime within the bounded window and clock skew.
    let skew = config.clock_skew_seconds as i64;
    if iat < 0 || nbf < 0 || exp < 0 {
        return Err(M2mJwtError::LifetimeOutOfBounds);
    }
    let ttl = exp - iat;
    if !(MIN_JWT_TTL_SECONDS as i64..=MAX_JWT_TTL_SECONDS as i64).contains(&ttl) {
        return Err(M2mJwtError::LifetimeOutOfBounds);
    }
    if nbf > iat + MAX_JWT_CLOCK_SKEW_SECONDS as i64
        || nbf < iat - MAX_JWT_CLOCK_SKEW_SECONDS as i64
    {
        return Err(M2mJwtError::LifetimeOutOfBounds);
    }
    if exp <= now_seconds - skew {
        return Err(M2mJwtError::LifetimeOutOfBounds);
    }
    if nbf > now_seconds + skew {
        return Err(M2mJwtError::LifetimeOutOfBounds);
    }

    // 5. Scopes: known, non-duplicate, subset of the ceiling, includes m2m:internal.
    let scope_claim = claims.scope.clone().ok_or(M2mJwtError::InvalidScope)?;
    let scopes = parse_scope_claim(&scope_claim)?;
    let ceiling = service_permissions(&service_id).ok_or(M2mJwtError::UnknownService)?;
    if !scopes.iter().any(|scope| scope == M2M_INTERNAL_SCOPE) {
        return Err(M2mJwtError::InvalidScope);
    }
    for scope in &scopes {
        if !ceiling.contains(&scope.as_str()) {
            return Err(M2mJwtError::InvalidScope);
        }
    }

    Ok(VerifiedM2mIdentity { service_id, scopes })
}

/// Parse a space-delimited scope claim: non-empty, single-space separated,
/// known, and free of duplicates.
fn parse_scope_claim(claim: &str) -> Result<Vec<String>, M2mJwtError> {
    if claim.is_empty() || claim.starts_with(' ') || claim.ends_with(' ') || claim.contains("  ") {
        return Err(M2mJwtError::InvalidScope);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut scopes = Vec::new();
    for scope in claim.split(' ') {
        if !KNOWN_SCOPES.contains(&scope) {
            return Err(M2mJwtError::InvalidScope);
        }
        if !seen.insert(scope.to_string()) {
            return Err(M2mJwtError::InvalidScope);
        }
        scopes.push(scope.to_string());
    }
    if scopes.is_empty() {
        return Err(M2mJwtError::InvalidScope);
    }
    Ok(scopes)
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The Gateway's bearer-auth policy: which credential kinds are accepted, and
/// the M2M JWT configuration when JWT is enabled. Built once from the
/// environment and shared by the auth middleware.
#[derive(Debug, Clone)]
pub struct M2mAuthPolicy {
    mode: GatewayAuthMode,
    config: Option<M2mJwtConfig>,
    configuration_valid: bool,
}

impl M2mAuthPolicy {
    /// Resolve from the environment. An explicit JWT/dual request without a
    /// complete usable JWT configuration stays in that requested mode and
    /// denies protected requests; it never downgrades to legacy authentication.
    pub fn from_env() -> Self {
        let mode = match std::env::var("M2M_GATEWAY_AUTH_MODE") {
            Ok(value) => GatewayAuthMode::from_env_value(&value),
            Err(std::env::VarError::NotPresent) => GatewayAuthMode::Legacy,
            Err(_) => GatewayAuthMode::Invalid,
        };
        let config = if mode.accepts_jwt() {
            M2mJwtConfig::from_env()
        } else {
            None
        };
        let policy = Self::from_parts(mode, config);
        if !policy.configuration_valid {
            tracing::error!(
                mode = ?policy.mode,
                "M2M auth configuration is invalid; all protected requests will fail closed"
            );
        }
        policy
    }

    fn from_parts(mode: GatewayAuthMode, config: Option<M2mJwtConfig>) -> Self {
        let configuration_valid = match mode {
            GatewayAuthMode::Legacy => true,
            GatewayAuthMode::Dual | GatewayAuthMode::Jwt => config.is_some(),
            GatewayAuthMode::Invalid => false,
        };
        Self {
            mode,
            config,
            configuration_valid,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        mode: GatewayAuthMode,
        config: Option<M2mJwtConfig>,
    ) -> Self {
        Self::from_parts(mode, config)
    }

    pub fn mode(&self) -> GatewayAuthMode {
        self.mode
    }

    pub fn configuration_valid(&self) -> bool {
        self.configuration_valid
    }

    /// Verify a Bearer value as an M2M JWT. Only meaningful when the selected
    /// mode accepts JWT and its full configuration was validated at startup.
    pub fn verify_bearer(&self, authorization: &str) -> Result<VerifiedM2mIdentity, M2mJwtError> {
        if !self.configuration_valid || !self.mode.accepts_jwt() {
            return Err(M2mJwtError::InvalidConfiguration);
        }
        let config = self
            .config
            .as_ref()
            .ok_or(M2mJwtError::InvalidConfiguration)?;
        let token = parse_bearer(authorization)?;
        verify_m2m_jwt(token, config, now_unix_secs())
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn jwt_modes_do_not_downgrade_when_configuration_is_missing() {
        let dual = M2mAuthPolicy::from_parts(GatewayAuthMode::Dual, None);
        let jwt = M2mAuthPolicy::from_parts(GatewayAuthMode::Jwt, None);
        let legacy = M2mAuthPolicy::from_parts(GatewayAuthMode::Legacy, None);
        let invalid = M2mAuthPolicy::from_parts(GatewayAuthMode::Invalid, None);

        assert!(!dual.configuration_valid());
        assert!(!jwt.configuration_valid());
        assert!(legacy.configuration_valid());
        assert!(!invalid.configuration_valid());
        assert_eq!(
            dual.verify_bearer("Bearer invalid.jwt.token").unwrap_err(),
            M2mJwtError::InvalidConfiguration
        );
        assert_eq!(
            jwt.verify_bearer("Bearer invalid.jwt.token").unwrap_err(),
            M2mJwtError::InvalidConfiguration
        );
    }

    #[test]
    fn jwt_configuration_rejects_empty_issuer_and_audience() {
        let secret = "x".repeat(MIN_JWT_SECRET_BYTES);
        assert_eq!(
            M2mJwtConfig::new(&secret, " ".into(), "audience".into(), 30).unwrap_err(),
            M2mJwtError::InvalidConfiguration
        );
        assert_eq!(
            M2mJwtConfig::new(&secret, "issuer".into(), "".into(), 30).unwrap_err(),
            M2mJwtError::InvalidConfiguration
        );
    }
}
