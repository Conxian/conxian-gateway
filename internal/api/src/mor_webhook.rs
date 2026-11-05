//! Merchant-of-Record (MoR) billing webhook — Gateway #507.
//!
//! The missing "money in -> entitlement" link. The managed surface accepts
//! `cxn_agent_*` keys that are meant to be minted by a MoR (Lemon Squeezy /
//! Paddle) subscription webhook, but no handler existed. This module implements
//! the provider-neutral receiver:
//!
//! ```text
//! MoR webhook (HMAC-signed)
//!   -> verify signature over the raw body (fail closed)
//!   -> parse event (provider-neutral)
//!   -> idempotency: dedupe on the event id
//!   -> provision: mint / rotate / revoke the tenant API key
//! ```
//!
//! Signature verification is provider-specific (Lemon Squeezy signs the raw body
//! with `X-Signature`; Paddle signs `{ts}:{body}` with `Paddle-Signature`), but
//! both are HMAC-SHA256 hex. The key lifecycle and idempotency ledger are
//! provider-neutral.
//!
//! The durable store is injected behind [`KeyStore`] so orchestration is
//! testable without a database; the gateway wires a Neon/Postgres store at
//! runtime. Minted keys are returned as plaintext exactly once and persisted
//! only as a SHA-256 hash. A key is never logged.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Paddle's documented default replay tolerance between `ts` and now.
const PADDLE_TOLERANCE_SECONDS: u64 = 5;

/// Provider of a MoR webhook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    LemonSqueezy,
    Paddle,
}

impl Provider {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "lemonsqueezy" | "lemon_squeezy" | "ls" => Some(Self::LemonSqueezy),
            "paddle" => Some(Self::Paddle),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebhookError {
    /// Signature header missing, malformed, or the HMAC did not match.
    InvalidSignature,
    /// Paddle timestamp outside the replay window (or malformed).
    StaleTimestamp,
    /// Body is not a JSON object / lacks a usable event id.
    Malformed,
}

impl std::fmt::Display for WebhookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "invalid webhook signature"),
            Self::StaleTimestamp => write!(f, "webhook timestamp outside replay window"),
            Self::Malformed => write!(f, "malformed webhook payload"),
        }
    }
}

/// The entitlement action a subscription event implies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// First activation: mint a new active key.
    Mint,
    /// Plan/period change: rotate to a new active key, retire the previous.
    Rotate,
    /// Cancellation/expiry/payment failure: revoke the active key.
    Revoke,
}

/// A provider-neutral, normalized subscription event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriptionEvent {
    /// Unique event id used as the idempotency key.
    pub event_id: String,
    /// Tenant the entitlement belongs to.
    pub tenant_id: String,
    /// Commercial plan label (e.g. `indie`, `business`).
    pub plan: String,
    pub action: KeyAction,
}

/// Verifies a webhook signature over the raw request body.
///
/// The body must be the exact bytes received; any re-serialization changes the
/// signature. Paddle binds the timestamp into the signed string and enforces a
/// replay window; Lemon Squeezy signs the raw body alone.
pub fn verify_signature(
    provider: Provider,
    secret: &str,
    headers: &HeaderMap,
    body: &[u8],
    now_secs: u64,
) -> Result<(), WebhookError> {
    if secret.is_empty() {
        return Err(WebhookError::InvalidSignature);
    }
    match provider {
        Provider::LemonSqueezy => {
            let signature = headers
                .get("X-Signature")
                .and_then(|v| v.to_str().ok())
                .ok_or(WebhookError::InvalidSignature)?;
            let expected = hmac_hex(secret, body);
            let provided = signature.trim().as_bytes();
            if expected.len() == provided.len()
                && expected.as_slice().ct_eq(provided).unwrap_u8() == 1
            {
                Ok(())
            } else {
                Err(WebhookError::InvalidSignature)
            }
        }
        Provider::Paddle => {
            let header = headers
                .get("Paddle-Signature")
                .and_then(|v| v.to_str().ok())
                .ok_or(WebhookError::InvalidSignature)?;
            let (ts, signatures) = parse_paddle_signature(header)?;
            let age = now_secs.abs_diff(ts);
            if age > PADDLE_TOLERANCE_SECONDS {
                return Err(WebhookError::StaleTimestamp);
            }
            let mut signed = ts.to_string().into_bytes();
            signed.push(b':');
            signed.extend_from_slice(body);
            let expected = hmac_hex(secret, &signed);
            let matched = signatures.iter().any(|provided| {
                expected.len() == provided.len()
                    && expected.as_slice().ct_eq(provided.as_bytes()).unwrap_u8() == 1
            });
            if matched {
                Ok(())
            } else {
                Err(WebhookError::InvalidSignature)
            }
        }
    }
}

fn hmac_hex(secret: &str, message: &[u8]) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(message);
    hex::encode(mac.finalize().into_bytes()).into_bytes()
}

/// Parses a `ts=<unix>;h1=<hex>[;h1=<hex>]` Paddle signature header.
fn parse_paddle_signature(header: &str) -> Result<(u64, Vec<String>), WebhookError> {
    let mut ts: Option<u64> = None;
    let mut signatures = Vec::new();
    for part in header.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part.split_once('=').ok_or(WebhookError::InvalidSignature)?;
        match key {
            "ts" => {
                ts = Some(
                    value
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| WebhookError::InvalidSignature)?,
                );
            }
            "h1" => signatures.push(value.trim().to_ascii_lowercase()),
            _ => {}
        }
    }
    let ts = ts.ok_or(WebhookError::InvalidSignature)?;
    if signatures.is_empty() {
        return Err(WebhookError::InvalidSignature);
    }
    Ok((ts, signatures))
}

/// Parses a verified provider payload into a normalized event.
///
/// Lemon Squeezy carries the event name in `meta.event_name` and the tenant in
/// `meta.custom_data.tenant_id`. Paddle carries `event_id`/`event_type` at the
/// top level and the tenant in `data.custom_data.tenant_id`. Neither provider
/// documents a Lemon Squeezy event id, so a caller-supplied fallback may be used.
pub fn parse_event(
    provider: Provider,
    body: &[u8],
    fallback_event_id: Option<&str>,
) -> Result<SubscriptionEvent, WebhookError> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| WebhookError::Malformed)?;
    match provider {
        Provider::LemonSqueezy => {
            let event_name = value
                .pointer("/meta/event_name")
                .and_then(|v| v.as_str())
                .ok_or(WebhookError::Malformed)?;
            let action = map_lemon_event(event_name).ok_or(WebhookError::Malformed)?;
            let tenant_id = value
                .pointer("/meta/custom_data/tenant_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or(WebhookError::Malformed)?
                .to_string();
            let plan = value
                .pointer("/data/attributes/variant_name")
                .and_then(|v| v.as_str())
                .unwrap_or("managed")
                .to_string();
            // Lemon Squeezy documents no unique event id; derive a stable one
            // from the subscription id and the event name so retries dedupe.
            let data_id = value
                .pointer("/data/id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let event_id = fallback_event_id
                .map(str::to_string)
                .unwrap_or_else(|| format!("ls:{event_name}:{data_id}"));
            Ok(SubscriptionEvent {
                event_id,
                tenant_id,
                plan,
                action,
            })
        }
        Provider::Paddle => {
            let event_id = value
                .get("event_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or(WebhookError::Malformed)?
                .to_string();
            let event_type = value
                .get("event_type")
                .and_then(|v| v.as_str())
                .ok_or(WebhookError::Malformed)?;
            let action = map_paddle_event(event_type).ok_or(WebhookError::Malformed)?;
            let tenant_id = value
                .pointer("/data/custom_data/tenant_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or(WebhookError::Malformed)?
                .to_string();
            let plan = value
                .pointer("/data/items/0/price/name")
                .and_then(|v| v.as_str())
                .unwrap_or("managed")
                .to_string();
            Ok(SubscriptionEvent {
                event_id,
                tenant_id,
                plan,
                action,
            })
        }
    }
}

fn map_lemon_event(name: &str) -> Option<KeyAction> {
    match name {
        "subscription_created" => Some(KeyAction::Mint),
        "subscription_updated"
        | "subscription_resumed"
        | "subscription_plan_changed"
        | "subscription_unpaused" => Some(KeyAction::Rotate),
        "subscription_cancelled"
        | "subscription_expired"
        | "subscription_paused"
        | "subscription_payment_failed"
        | "subscription_payment_refunded" => Some(KeyAction::Revoke),
        _ => None,
    }
}

fn map_paddle_event(name: &str) -> Option<KeyAction> {
    match name {
        "subscription.created" | "subscription.activated" | "subscription.trialing" => {
            Some(KeyAction::Mint)
        }
        "subscription.updated" | "subscription.resumed" | "subscription.imported" => {
            Some(KeyAction::Rotate)
        }
        "subscription.canceled"
        | "subscription.past_due"
        | "subscription.paused"
        | "transaction.payment_failed"
        | "transaction.canceled" => Some(KeyAction::Revoke),
        _ => None,
    }
}

/// Outcome of applying an event. `key` is present only for mint/rotate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// A new key was minted; the plaintext is returned exactly once.
    Minted { key: String },
    /// The active key was rotated; the new plaintext is returned exactly once.
    Rotated { key: String },
    /// The active key was revoked.
    Revoked,
    /// The event id was already processed; no state changed.
    Duplicate,
}

/// A tenant's stored API-key record (hash only; never plaintext).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredKey {
    pub tenant_id: String,
    pub api_key_hash: String,
    pub tier: String,
}

/// Durable key store + idempotency ledger.
///
/// The Neon/Postgres implementation lives in `mor_webhook_store`; an in-memory
/// implementation backs unit tests. Every method fails closed on error.
#[async_trait::async_trait]
pub trait KeyStore: Send + Sync {
    /// Atomically records the event as processed. Returns `false` if the event
    /// id already exists (idempotent replay).
    async fn record_event(
        &self,
        event_id: &str,
        event_type: &str,
        tenant_id: &str,
    ) -> Result<bool, String>;
    /// Revokes the active key(s) for a tenant. Returns the number revoked.
    async fn revoke_active(&self, tenant_id: &str) -> Result<u64, String>;
    /// Inserts a new active key hash for a tenant, retiring any previous active
    /// key. `tier` is recorded with the key.
    async fn mint_key(&self, tenant_id: &str, api_key_hash: &str, tier: &str)
        -> Result<(), String>;
    /// Returns the currently active key record for a tenant, if any.
    async fn active_key(&self, tenant_id: &str) -> Result<Option<StoredKey>, String>;
    /// Resolves an active key by its SHA-256 hex hash. The managed-auth path
    /// uses this so a key minted by the webhook authenticates against the
    /// durable store without the plaintext ever being mirrored into the
    /// process environment.
    async fn active_key_by_hash(&self, api_key_hash: &str) -> Result<Option<StoredKey>, String>;
}

/// Generates a fresh `cxn_agent_<32-hex>` key and its SHA-256 hex hash.
fn mint_material() -> (String, String) {
    // A random 128-bit key is unguessable; UUID v4 draws from the OS CSPRNG.
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let key = format!("cxn_agent_{}", hex::encode(bytes));
    let hash = hash_key(&key);
    (key, hash)
}

/// SHA-256 hex of a key. The single definition shared by minting and the
/// managed-auth lookup so the two can never drift apart.
pub fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Applies a normalized event to the durable store.
///
/// Order is deliberate: the idempotency record is written first so a crash
/// after provisioning cannot cause a duplicate grant on retry, and a replay of
/// an already-processed event is a no-op.
pub async fn apply_event(
    store: &dyn KeyStore,
    event: &SubscriptionEvent,
) -> Result<ApplyOutcome, String> {
    let is_new = store
        .record_event(
            &event.event_id,
            &format!("{:?}", event.action),
            &event.tenant_id,
        )
        .await?;
    if !is_new {
        return Ok(ApplyOutcome::Duplicate);
    }
    match event.action {
        KeyAction::Mint => {
            let (key, hash) = mint_material();
            store.mint_key(&event.tenant_id, &hash, &event.plan).await?;
            Ok(ApplyOutcome::Minted { key })
        }
        KeyAction::Rotate => {
            let (key, hash) = mint_material();
            store.mint_key(&event.tenant_id, &hash, &event.plan).await?;
            Ok(ApplyOutcome::Rotated { key })
        }
        KeyAction::Revoke => {
            store.revoke_active(&event.tenant_id).await?;
            Ok(ApplyOutcome::Revoked)
        }
    }
}

/// Resolves the configured MoR provider, if any.
pub fn provider_from_env() -> Option<Provider> {
    std::env::var("MOR_PROVIDER")
        .ok()
        .and_then(|v| Provider::parse(&v))
}

/// Shared state for the webhook route: provider + signing secret + store.
#[derive(Clone)]
pub struct MorWebhookState {
    pub provider: Provider,
    pub secret: String,
    pub store: std::sync::Arc<dyn KeyStore>,
}

/// HTTP handler: verify, parse, and apply a MoR subscription webhook.
///
/// The raw body is read as bytes before any JSON parsing so the signature is
/// checked over the exact bytes the provider signed. Any failure is a 4xx and
/// changes no state (fail closed).
pub async fn mor_webhook_handler(
    State(state): State<MorWebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, axum::Json<serde_json::Value>), (StatusCode, String)> {
    let now = now_secs();
    if let Err(err) = verify_signature(state.provider, &state.secret, &headers, &body, now) {
        return Err((StatusCode::UNAUTHORIZED, err.to_string()));
    }
    let event = match parse_event(state.provider, &body, None) {
        Ok(event) => event,
        Err(err) => return Err((StatusCode::BAD_REQUEST, err.to_string())),
    };
    match apply_event(state.store.as_ref(), &event).await {
        Ok(outcome) => Ok((StatusCode::OK, axum::Json(outcome_json(outcome)))),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "webhook processing failed".to_string(),
        )),
    }
}

fn outcome_json(outcome: ApplyOutcome) -> serde_json::Value {
    match outcome {
        ApplyOutcome::Minted { key } => serde_json::json!({"status": "minted", "api_key": key}),
        ApplyOutcome::Rotated { key } => serde_json::json!({"status": "rotated", "api_key": key}),
        ApplyOutcome::Revoked => serde_json::json!({"status": "revoked"}),
        ApplyOutcome::Duplicate => serde_json::json!({"status": "duplicate"}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;

    fn sign_lemon(secret: &str, body: &[u8]) -> String {
        String::from_utf8(hmac_hex(secret, body)).unwrap()
    }

    fn sign_paddle(secret: &str, ts: u64, body: &[u8]) -> String {
        let mut signed = ts.to_string().into_bytes();
        signed.push(b':');
        signed.extend_from_slice(body);
        format!(
            "ts={ts};h1={}",
            String::from_utf8(hmac_hex(secret, &signed)).unwrap()
        )
    }

    fn headers_with(name: &'static str, value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(name, value.parse().unwrap());
        headers
    }

    #[test]
    fn lemon_signature_round_trips_and_rejects_tampering() {
        let body = br#"{"meta":{"event_name":"subscription_created"}}"#;
        let sig = sign_lemon("s3cret", body);
        let headers = headers_with("X-Signature", &sig);
        assert_eq!(
            verify_signature(Provider::LemonSqueezy, "s3cret", &headers, body, 0),
            Ok(())
        );
        assert_eq!(
            verify_signature(Provider::LemonSqueezy, "other", &headers, body, 0),
            Err(WebhookError::InvalidSignature)
        );
        let tampered = br#"{"meta":{"event_name":"subscription_cancelled"}}"#;
        assert_eq!(
            verify_signature(Provider::LemonSqueezy, "s3cret", &headers, tampered, 0),
            Err(WebhookError::InvalidSignature)
        );
        assert_eq!(
            verify_signature(Provider::LemonSqueezy, "s3cret", &HeaderMap::new(), body, 0),
            Err(WebhookError::InvalidSignature)
        );
    }

    #[test]
    fn paddle_signature_binds_timestamp_and_rejects_stale() {
        let body = br#"{"event_id":"evt_1","event_type":"subscription.created"}"#;
        let now = 1_700_000_000u64;
        let sig = sign_paddle("pdl_secret", now, body);
        let headers = headers_with("Paddle-Signature", &sig);
        assert_eq!(
            verify_signature(Provider::Paddle, "pdl_secret", &headers, body, now),
            Ok(())
        );
        assert_eq!(
            verify_signature(Provider::Paddle, "pdl_secret", &headers, body, now + 6),
            Err(WebhookError::StaleTimestamp)
        );
        let tampered = br#"{"event_id":"evt_1","event_type":"subscription.canceled"}"#;
        assert_eq!(
            verify_signature(Provider::Paddle, "pdl_secret", &headers, tampered, now),
            Err(WebhookError::InvalidSignature)
        );
    }

    #[test]
    fn paddle_accepts_any_matching_rotation_signature() {
        let body = br#"{"event_id":"evt_2","event_type":"subscription.updated"}"#;
        let now = 1_700_000_000u64;
        let good = sign_paddle("new_secret", now, body);
        let good_hex = good.split("h1=").nth(1).unwrap().to_string();
        let header = format!("ts={now};h1={};h1={good_hex}", "0".repeat(64));
        let headers = headers_with("Paddle-Signature", &header);
        assert_eq!(
            verify_signature(Provider::Paddle, "new_secret", &headers, body, now),
            Ok(())
        );
    }

    #[test]
    fn empty_secret_fails_closed() {
        let body = br#"{}"#;
        assert_eq!(
            verify_signature(Provider::LemonSqueezy, "", &HeaderMap::new(), body, 0),
            Err(WebhookError::InvalidSignature)
        );
    }

    #[test]
    fn parses_lemon_and_paddle_events() {
        let lemon = br#"{"meta":{"event_name":"subscription_created","custom_data":{"tenant_id":"t1"}},"data":{"id":"sub_9","attributes":{"variant_name":"indie"}}}"#;
        let event = parse_event(Provider::LemonSqueezy, lemon, None).unwrap();
        assert_eq!(event.tenant_id, "t1");
        assert_eq!(event.plan, "indie");
        assert_eq!(event.action, KeyAction::Mint);
        assert_eq!(event.event_id, "ls:subscription_created:sub_9");

        let paddle = br#"{"event_id":"evt_5","event_type":"subscription.canceled","data":{"custom_data":{"tenant_id":"t2"},"items":[{"price":{"name":"business"}}]}}"#;
        let event = parse_event(Provider::Paddle, paddle, None).unwrap();
        assert_eq!(event.tenant_id, "t2");
        assert_eq!(event.plan, "business");
        assert_eq!(event.action, KeyAction::Revoke);
        assert_eq!(event.event_id, "evt_5");
    }

    #[test]
    fn rejects_unknown_event_and_missing_tenant() {
        let unknown =
            br#"{"meta":{"event_name":"order_created","custom_data":{"tenant_id":"t1"}}}"#;
        assert_eq!(
            parse_event(Provider::LemonSqueezy, unknown, None),
            Err(WebhookError::Malformed)
        );
        let no_tenant = br#"{"meta":{"event_name":"subscription_created"}}"#;
        assert_eq!(
            parse_event(Provider::LemonSqueezy, no_tenant, None),
            Err(WebhookError::Malformed)
        );
    }

    #[derive(Default)]
    struct MemStore {
        events: Mutex<HashSet<String>>,
        keys: Mutex<Vec<(String, String, String, bool)>>,
    }

    #[async_trait::async_trait]
    impl KeyStore for MemStore {
        async fn record_event(
            &self,
            event_id: &str,
            _t: &str,
            _tenant: &str,
        ) -> Result<bool, String> {
            Ok(self.events.lock().unwrap().insert(event_id.to_string()))
        }
        async fn revoke_active(&self, tenant_id: &str) -> Result<u64, String> {
            let mut keys = self.keys.lock().unwrap();
            let mut n = 0;
            for k in keys.iter_mut() {
                if k.0 == tenant_id && k.3 {
                    k.3 = false;
                    n += 1;
                }
            }
            Ok(n)
        }
        async fn mint_key(&self, tenant_id: &str, hash: &str, tier: &str) -> Result<(), String> {
            self.revoke_active(tenant_id).await?;
            self.keys.lock().unwrap().push((
                tenant_id.to_string(),
                hash.to_string(),
                tier.to_string(),
                true,
            ));
            Ok(())
        }
        async fn active_key(&self, tenant_id: &str) -> Result<Option<StoredKey>, String> {
            Ok(self
                .keys
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|k| k.0 == tenant_id && k.3)
                .map(|k| StoredKey {
                    tenant_id: k.0.clone(),
                    api_key_hash: k.1.clone(),
                    tier: k.2.clone(),
                }))
        }
        async fn active_key_by_hash(&self, hash: &str) -> Result<Option<StoredKey>, String> {
            Ok(self
                .keys
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|k| k.1 == hash && k.3)
                .map(|k| StoredKey {
                    tenant_id: k.0.clone(),
                    api_key_hash: k.1.clone(),
                    tier: k.2.clone(),
                }))
        }
    }

    #[tokio::test]
    async fn mint_rotate_revoke_lifecycle_and_idempotency() {
        let store = MemStore::default();
        let mint = SubscriptionEvent {
            event_id: "evt_1".into(),
            tenant_id: "t1".into(),
            plan: "indie".into(),
            action: KeyAction::Mint,
        };
        let ApplyOutcome::Minted { key } = apply_event(&store, &mint).await.unwrap() else {
            panic!("expected mint");
        };
        assert!(key.starts_with("cxn_agent_"));
        let active = store.active_key("t1").await.unwrap().unwrap();
        assert_eq!(
            active.api_key_hash,
            hex::encode(Sha256::digest(key.as_bytes()))
        );
        assert_eq!(active.tier, "indie");

        assert_eq!(
            apply_event(&store, &mint).await.unwrap(),
            ApplyOutcome::Duplicate
        );

        let rotate = SubscriptionEvent {
            event_id: "evt_2".into(),
            tenant_id: "t1".into(),
            plan: "business".into(),
            action: KeyAction::Rotate,
        };
        let ApplyOutcome::Rotated { key: key2 } = apply_event(&store, &rotate).await.unwrap()
        else {
            panic!("expected rotate");
        };
        assert_ne!(key, key2);
        assert_eq!(
            store.active_key("t1").await.unwrap().unwrap().tier,
            "business"
        );

        let revoke = SubscriptionEvent {
            event_id: "evt_3".into(),
            tenant_id: "t1".into(),
            plan: "business".into(),
            action: KeyAction::Revoke,
        };
        assert_eq!(
            apply_event(&store, &revoke).await.unwrap(),
            ApplyOutcome::Revoked
        );
        assert!(store.active_key("t1").await.unwrap().is_none());
    }
}
