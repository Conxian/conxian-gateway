//! Conformance tests for the Gateway M2M JWT verifier.
//!
//! These mirror the platform's `m2m.test.ts` adversarial cases so the Gateway
//! verifier and the platform issuer stay in exact agreement.

use conxian_api::m2m_auth::{
    parse_bearer, service_permissions, verify_m2m_jwt, GatewayAuthMode, M2mJwtConfig, M2mJwtError,
    DEFAULT_JWT_CLOCK_SKEW_SECONDS,
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

// The signing secret is random per test run (no hard-coded cryptographic value
// anywhere in the fixture), held in one process-wide cell so `config()` and
// `sign()` agree. The tests assert verification *behaviour*, which does not
// depend on the secret's value.
fn test_secret() -> &'static str {
    use std::sync::OnceLock;
    static SECRET: OnceLock<String> = OnceLock::new();
    SECRET.get_or_init(|| {
        let bytes: [u8; 32] = rand::random();
        hex::encode(bytes) // 64 hex chars >= the 32-byte minimum
    })
}

/// A second, independent random secret, used to prove a wrong-secret token is
/// rejected. Distinct from `test_secret()` by construction.
fn different_secret() -> String {
    let bytes: [u8; 32] = rand::random();
    hex::encode(bytes)
}

const ISSUER: &str = "conxian-platform";
const AUDIENCE: &str = "conxian-gateway";
const NOW: i64 = 1_700_000_000;

fn b64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Sign a token with the given header/claims (or raw header override).
fn sign_with(header: serde_json::Value, claims: serde_json::Value, secret: &str) -> String {
    let header_b64 = b64url(header.to_string().as_bytes());
    let payload_b64 = b64url(claims.to_string().as_bytes());
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{header_b64}.{payload_b64}").as_bytes());
    let sig = b64url(&mac.finalize().into_bytes());
    format!("{header_b64}.{payload_b64}.{sig}")
}

fn good_claims() -> serde_json::Value {
    json!({
        "iss": ISSUER,
        "aud": AUDIENCE,
        "sub": "gateway",
        "scope": "read:admin m2m:internal",
        "iat": NOW,
        "nbf": NOW,
        "exp": NOW + 300,
        "jti": "jti-123"
    })
}

fn good_header() -> serde_json::Value {
    json!({ "alg": "HS256", "typ": "JWT" })
}

fn config() -> M2mJwtConfig {
    M2mJwtConfig::new(
        test_secret(),
        ISSUER.to_string(),
        AUDIENCE.to_string(),
        DEFAULT_JWT_CLOCK_SKEW_SECONDS,
    )
    .unwrap()
}

fn sign(claims: serde_json::Value) -> String {
    sign_with(good_header(), claims, test_secret())
}

#[test]
fn valid_token_verifies_and_exposes_service_and_scopes() {
    let identity = verify_m2m_jwt(&sign(good_claims()), &config(), NOW).unwrap();
    assert_eq!(identity.service_id, "gateway");
    assert!(identity.has_scope("read:admin"));
    assert!(identity.has_scope("m2m:internal"));
}

#[test]
fn rejects_short_secret_configuration() {
    assert_eq!(
        M2mJwtConfig::new("too-short", ISSUER.into(), AUDIENCE.into(), 30).unwrap_err(),
        M2mJwtError::InvalidConfiguration
    );
}

#[test]
fn rejects_algorithm_and_kid_header_deviations() {
    let cfg = config();
    for header in [
        json!({ "alg": "none", "typ": "JWT" }),
        json!({ "alg": "HS384", "typ": "JWT" }),
        json!({ "alg": "HS256", "typ": "JWT", "kid": "k1" }),
        json!({ "alg": "HS256" }),
    ] {
        let token = sign_with(header, good_claims(), test_secret());
        assert_eq!(
            verify_m2m_jwt(&token, &cfg, NOW).unwrap_err(),
            M2mJwtError::InvalidHeader
        );
    }
}

#[test]
fn rejects_bad_signature() {
    let token = sign_with(good_header(), good_claims(), &different_secret());
    assert_eq!(
        verify_m2m_jwt(&token, &config(), NOW).unwrap_err(),
        M2mJwtError::InvalidSignature
    );
}

#[test]
fn rejects_wrong_issuer_and_audience() {
    let cfg = config();
    let mut claims = good_claims();
    claims["iss"] = json!("someone-else");
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::WrongIssuer
    );

    let mut claims = good_claims();
    claims["aud"] = json!("another-audience");
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::WrongAudience
    );
}

#[test]
fn rejects_unknown_service() {
    let mut claims = good_claims();
    claims["sub"] = json!("not-a-service");
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &config(), NOW).unwrap_err(),
        M2mJwtError::UnknownService
    );
}

#[test]
fn rejects_missing_required_claims() {
    for key in ["aud", "sub", "scope", "iat", "nbf", "exp", "jti"] {
        let mut claims = good_claims();
        claims.as_object_mut().unwrap().remove(key);
        let err = verify_m2m_jwt(&sign(claims), &config(), NOW).unwrap_err();
        // Every variant here is fail-closed; a missing claim never authorizes.
        assert!(
            matches!(
                err,
                M2mJwtError::InvalidClaims
                    | M2mJwtError::InvalidScope
                    | M2mJwtError::WrongAudience
                    | M2mJwtError::UnknownService
            ),
            "missing {key} should fail closed, got {err:?}"
        );
    }
}

#[test]
fn rejects_expired_and_future_tokens_outside_skew() {
    let cfg = config();
    let mut claims = good_claims();
    claims["iat"] = json!(NOW - 1000);
    claims["nbf"] = json!(NOW - 1000);
    claims["exp"] = json!(NOW - 100); // exp <= now - skew
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::LifetimeOutOfBounds
    );

    let mut claims = good_claims();
    claims["iat"] = json!(NOW + 500);
    claims["nbf"] = json!(NOW + 500);
    claims["exp"] = json!(NOW + 800);
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::LifetimeOutOfBounds
    );
}

#[test]
fn rejects_out_of_bounds_ttl() {
    let cfg = config();
    let mut claims = good_claims();
    claims["exp"] = json!(NOW + 30); // TTL 30 < MIN 60
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::LifetimeOutOfBounds
    );

    let mut claims = good_claims();
    claims["exp"] = json!(NOW + 5000); // TTL > MAX 900
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::LifetimeOutOfBounds
    );
}

#[test]
fn rejects_scope_violations() {
    let cfg = config();
    // Unknown scope.
    let mut claims = good_claims();
    claims["scope"] = json!("read:admin made:up m2m:internal");
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::InvalidScope
    );

    // Duplicate scope.
    let mut claims = good_claims();
    claims["scope"] = json!("read:admin read:admin m2m:internal");
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::InvalidScope
    );

    // Exceeds the service ceiling (gateway may not hold write:treasury).
    let mut claims = good_claims();
    claims["scope"] = json!("write:treasury m2m:internal");
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::InvalidScope
    );

    // Missing m2m:internal.
    let mut claims = good_claims();
    claims["scope"] = json!("read:admin");
    assert_eq!(
        verify_m2m_jwt(&sign(claims), &cfg, NOW).unwrap_err(),
        M2mJwtError::InvalidScope
    );
}

#[test]
fn parse_bearer_is_bounded_and_case_insensitive() {
    assert_eq!(parse_bearer("Bearer abc.def.ghi").unwrap(), "abc.def.ghi");
    assert_eq!(parse_bearer("bearer abc.def.ghi").unwrap(), "abc.def.ghi");
    assert_eq!(
        parse_bearer("Basic abc").unwrap_err(),
        M2mJwtError::MalformedBearer
    );
    assert_eq!(
        parse_bearer("Bearer   ").unwrap_err(),
        M2mJwtError::MalformedBearer
    );
    let huge = format!("Bearer {}", "a".repeat(5000));
    assert_eq!(
        parse_bearer(&huge).unwrap_err(),
        M2mJwtError::MalformedBearer
    );
}

#[test]
fn auth_mode_acceptance_matrix() {
    assert_eq!(
        GatewayAuthMode::from_env_value("legacy"),
        GatewayAuthMode::Legacy
    );
    assert_eq!(
        GatewayAuthMode::from_env_value("DUAL"),
        GatewayAuthMode::Dual
    );
    assert_eq!(GatewayAuthMode::from_env_value("jwt"), GatewayAuthMode::Jwt);
    assert_eq!(
        GatewayAuthMode::from_env_value("anything-else"),
        GatewayAuthMode::Legacy
    );

    assert!(GatewayAuthMode::Legacy.accepts_legacy());
    assert!(!GatewayAuthMode::Legacy.accepts_jwt());
    assert!(GatewayAuthMode::Dual.accepts_legacy() && GatewayAuthMode::Dual.accepts_jwt());
    assert!(!GatewayAuthMode::Jwt.accepts_legacy());
    assert!(GatewayAuthMode::Jwt.accepts_jwt());
}

#[test]
fn service_permissions_cover_registered_services() {
    for service in [
        "gateway",
        "nexus",
        "wallet",
        "admin-dashboard",
        "orbit",
        "ui",
        "elizaos",
        "pulse-bos",
    ] {
        let ceiling = service_permissions(service).unwrap();
        assert!(
            ceiling.contains(&"m2m:internal"),
            "{service} ceiling must include m2m:internal"
        );
    }
    assert!(service_permissions("external").is_none());
    assert!(service_permissions("nope").is_none());
}
