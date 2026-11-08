//! Self-serve onboarding API (Gateway #713).
//!
//! Implements the design in `conxian-business/docs/operations/SELF_SERVE_CLOUD_PROVISIONING_2026_11_05.md`
//! §5.1–§5.3: an idempotent, resumable onboarding lane plus a machine-readable
//! limits surface, so an autonomous client can provision a tenant and budget
//! before it provisions, instead of discovering limits through a support ticket.
//!
//! Design rules enforced here:
//!   * **Idempotent** — a client-supplied idempotency key makes a retry return
//!     the existing receipt rather than provisioning a second tenant.
//!   * **Resumable** — each step records its own state; a failure does not
//!     discard earlier steps, and `POST /v1/tenants/{id}/deploy` resumes.
//!   * **Fail-closed on entitlement** — the entitlement row is written before a
//!     key is minted, so a partial provision can never yield a
//!     working-but-unmetered endpoint.
//!   * **Receipt is the contract** — the receipt names the endpoint, the key
//!     id (never the secret), the quota and the state.
//!
//! The durable store lives behind [`OnboardingStore`] (Neon/Postgres in
//! `onboarding_store`, in-memory for tests). Key minting reuses the MoR
//! [`KeyStore`] so a key issued here authenticates through the same managed-auth
//! path as a webhook-minted key — there is no second key store.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

use crate::mor_webhook::{mint_material, KeyStore};
use crate::tenant::TenantContext;

/// The ordered provisioning steps. Each is independently observable and
/// resumable, matching design §5.1.
pub const ONBOARDING_STEPS: [&str; 4] = ["entitlement", "endpoint", "key", "activation"];

/// Lifecycle of an onboarding tenant (design §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingState {
    Requested,
    Provisioning,
    Active,
    Failed,
    Suspended,
    Terminated,
}

impl OnboardingState {
    pub fn as_str(self) -> &'static str {
        match self {
            OnboardingState::Requested => "requested",
            OnboardingState::Provisioning => "provisioning",
            OnboardingState::Active => "active",
            OnboardingState::Failed => "failed",
            OnboardingState::Suspended => "suspended",
            OnboardingState::Terminated => "terminated",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(OnboardingState::Requested),
            "provisioning" => Some(OnboardingState::Provisioning),
            "active" => Some(OnboardingState::Active),
            "failed" => Some(OnboardingState::Failed),
            "suspended" => Some(OnboardingState::Suspended),
            "terminated" => Some(OnboardingState::Terminated),
            _ => None,
        }
    }
}

/// A single provisioning step's recorded state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    pub name: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl StepRecord {
    fn pending(name: &str) -> Self {
        Self {
            name: name.to_string(),
            state: "pending".to_string(),
            detail: None,
        }
    }

    fn done(&mut self) {
        self.state = "done".to_string();
        self.detail = None;
    }

    fn failed(&mut self, detail: impl Into<String>) {
        self.state = "failed".to_string();
        self.detail = Some(detail.into());
    }

    fn skipped(&mut self, detail: impl Into<String>) {
        self.state = "skipped".to_string();
        self.detail = Some(detail.into());
    }
}

/// The durable tenant onboarding record. `key_id` is a public identifier for
/// the active key (a hash prefix) — never the key itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantRecord {
    pub tenant_id: String,
    pub idempotency_key: String,
    pub state: String,
    pub tier: String,
    pub plan: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub steps: Vec<StepRecord>,
}

impl TenantRecord {
    fn new(tenant_id: String, idempotency_key: String, tier: &str) -> Self {
        let now = now_secs();
        Self {
            tenant_id,
            idempotency_key,
            state: OnboardingState::Requested.as_str().to_string(),
            tier: tier.to_string(),
            plan: tier.to_string(),
            endpoint_url: None,
            key_id: None,
            created_at: now,
            updated_at: now,
            steps: ONBOARDING_STEPS
                .iter()
                .map(|s| StepRecord::pending(s))
                .collect(),
        }
    }

    fn step_mut(&mut self, name: &str) -> Option<&mut StepRecord> {
        self.steps.iter_mut().find(|s| s.name == name)
    }

    fn touch(&mut self) {
        self.updated_at = now_secs();
    }

    fn step_done(&self, name: &str) -> bool {
        self.steps
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.state == "done")
            .unwrap_or(false)
    }
}

/// Published, machine-readable limits for a tier. These are configuration, not
/// observed telemetry — the point of the `limits` surface is that a client can
/// read them before it provisions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantLimits {
    pub tier: String,
    pub requests_per_minute: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_requests: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trial_days: Option<u32>,
    pub always_free: bool,
    pub per_call_satoshi: u128,
}

/// The published limits catalog. `sandbox` is the no-card free lane; paid tiers
/// follow the subscription plans in [`crate::billing`] and charge the canonical
/// x402 per-call amount, so the advertised price cannot drift from the charged
/// amount.
pub fn limits_for_tier(tier: &str) -> Option<TenantLimits> {
    let per_call = crate::x402::X402_PER_CALL_SATOSHI;
    let limits = match tier {
        "sandbox" => TenantLimits {
            tier: "sandbox".to_string(),
            requests_per_minute: 60,
            monthly_requests: Some(10_000),
            trial_days: Some(14),
            always_free: true,
            per_call_satoshi: 0,
        },
        "starter" => TenantLimits {
            tier: "starter".to_string(),
            requests_per_minute: 120,
            monthly_requests: None,
            trial_days: None,
            always_free: false,
            per_call_satoshi: per_call,
        },
        "indie" => TenantLimits {
            tier: "indie".to_string(),
            requests_per_minute: 300,
            monthly_requests: None,
            trial_days: None,
            always_free: false,
            per_call_satoshi: per_call,
        },
        "business" | "pro" => TenantLimits {
            tier: tier.to_string(),
            requests_per_minute: 600,
            monthly_requests: None,
            trial_days: None,
            always_free: false,
            per_call_satoshi: per_call,
        },
        "enterprise" => TenantLimits {
            tier: "enterprise".to_string(),
            requests_per_minute: 1_200,
            monthly_requests: None,
            trial_days: None,
            always_free: false,
            per_call_satoshi: per_call,
        },
        _ => return None,
    };
    Some(limits)
}

/// Durable onboarding store. Every method fails closed on error.
#[async_trait]
pub trait OnboardingStore: Send + Sync {
    /// Returns the record for an idempotency key, if one exists. This is the
    /// idempotency gate: a repeat `POST /v1/onboard` with the same key returns
    /// the existing receipt instead of provisioning again.
    async fn find_by_idempotency(&self, key: &str) -> Result<Option<TenantRecord>, String>;
    /// Returns a tenant record by id.
    async fn get(&self, tenant_id: &str) -> Result<Option<TenantRecord>, String>;
    /// Inserts or updates a record.
    async fn save(&self, record: &TenantRecord) -> Result<(), String>;
}

/// In-memory store used by unit tests and as a fallback when no database is
/// configured. It is deliberately not durable: the production lane mounts the
/// routes only when a durable store is present.
#[derive(Default)]
pub struct InMemoryOnboardingStore {
    tenants: Mutex<HashMap<String, TenantRecord>>,
    idempotency: Mutex<HashMap<String, String>>,
}

impl InMemoryOnboardingStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl OnboardingStore for InMemoryOnboardingStore {
    async fn find_by_idempotency(&self, key: &str) -> Result<Option<TenantRecord>, String> {
        let idem = self.idempotency.lock().unwrap();
        let Some(tenant_id) = idem.get(key) else {
            return Ok(None);
        };
        Ok(self.tenants.lock().unwrap().get(tenant_id).cloned())
    }

    async fn get(&self, tenant_id: &str) -> Result<Option<TenantRecord>, String> {
        Ok(self.tenants.lock().unwrap().get(tenant_id).cloned())
    }

    async fn save(&self, record: &TenantRecord) -> Result<(), String> {
        self.idempotency
            .lock()
            .unwrap()
            .insert(record.idempotency_key.clone(), record.tenant_id.clone());
        self.tenants
            .lock()
            .unwrap()
            .insert(record.tenant_id.clone(), record.clone());
        Ok(())
    }
}

/// A request to provision a tenant.
#[derive(Clone, Debug, Deserialize)]
pub struct OnboardRequest {
    /// Client-supplied idempotency key. A retry with the same key returns the
    /// existing receipt.
    pub idempotency_key: String,
    /// Requested tier. Defaults to the no-card `sandbox` lane.
    #[serde(default)]
    pub tier: Option<String>,
}

/// A successful provisioning outcome. `api_key` is the plaintext key, present
/// exactly once (on first provision or rotation) and never stored.
#[derive(Clone, Debug)]
pub struct OnboardOutcome {
    pub record: TenantRecord,
    pub api_key: Option<String>,
}

/// Errors surfaced by the onboarding service, mapped to HTTP status by the
/// handlers.
#[derive(Debug)]
pub enum OnboardError {
    /// A malformed or unsupported request (unknown tier, empty idempotency key).
    BadRequest(String),
    /// The requested tenant does not exist.
    NotFound(String),
    /// A durable step failed; the record is left resumable.
    Step(String),
}

impl OnboardError {
    fn into_error(self) -> ApiError {
        let (code, msg) = match self {
            OnboardError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            OnboardError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            OnboardError::Step(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
        };
        (code, Json(json!({ "error": msg })))
    }
}

/// The handler error type: a small `(status, body)` tuple, matching the
/// convention in [`crate::handlers`]. Kept small deliberately so
/// `result_large_err` does not fire on the handlers.
pub type ApiError = (StatusCode, Json<Value>);

/// The onboarding orchestrator. It sequences the durable steps behind one
/// idempotent call and reuses the MoR [`KeyStore`] to mint tenant keys.
#[derive(Clone)]
pub struct OnboardingService {
    store: Arc<dyn OnboardingStore>,
    key_store: Option<Arc<dyn KeyStore>>,
    /// Public base for the managed agent surface; the tenant endpoint is
    /// derived from it so the receipt names a real host.
    public_base: String,
}

impl OnboardingService {
    pub fn new(
        store: Arc<dyn OnboardingStore>,
        key_store: Option<Arc<dyn KeyStore>>,
        public_base: impl Into<String>,
    ) -> Self {
        Self {
            store,
            key_store,
            public_base: public_base.into(),
        }
    }

    /// Builds a service from the environment. `GATEWAY_PUBLIC_BASE_URL` names
    /// the managed host; it defaults to the canonical commercial base.
    pub fn from_env(store: Arc<dyn OnboardingStore>, key_store: Option<Arc<dyn KeyStore>>) -> Self {
        let public_base = std::env::var("GATEWAY_PUBLIC_BASE_URL")
            .ok()
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| crate::discovery::API_BASE_URL.to_string());
        Self::new(store, key_store, public_base)
    }

    pub fn store(&self) -> &Arc<dyn OnboardingStore> {
        &self.store
    }

    /// Idempotent, resumable provisioning (design §5.1).
    ///
    /// Order is deliberate: the entitlement row is persisted before any key is
    /// minted, so a crash between the two cannot leave a working-but-unmetered
    /// endpoint. A repeat with the same idempotency key returns the existing
    /// receipt without re-provisioning.
    pub async fn provision(&self, req: OnboardRequest) -> Result<OnboardOutcome, OnboardError> {
        let idempotency_key = req.idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(OnboardError::BadRequest(
                "idempotency_key must be non-empty".to_string(),
            ));
        }
        let tier = req
            .tier
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("sandbox")
            .to_string();
        if limits_for_tier(&tier).is_none() {
            return Err(OnboardError::BadRequest(format!("unknown tier {tier:?}")));
        }

        // Idempotency gate: a seen key returns the existing receipt unchanged.
        if let Some(existing) = self
            .store
            .find_by_idempotency(&idempotency_key)
            .await
            .map_err(OnboardError::Step)?
        {
            return Ok(OnboardOutcome {
                record: existing,
                api_key: None,
            });
        }

        let tenant_id = format!("t_{}", hex::encode(uuid::Uuid::new_v4().into_bytes()));
        let mut record = TenantRecord::new(tenant_id, idempotency_key, &tier);
        self.store.save(&record).await.map_err(OnboardError::Step)?;

        record.state = OnboardingState::Provisioning.as_str().to_string();
        record.touch();
        self.store.save(&record).await.map_err(OnboardError::Step)?;

        // Step 1: entitlement (persisted first — the fail-closed ordering).
        if let Some(step) = record.step_mut("entitlement") {
            step.done();
        }
        record.touch();
        self.store.save(&record).await.map_err(OnboardError::Step)?;

        // Step 2: endpoint.
        if let Some(step) = record.step_mut("endpoint") {
            step.done();
        }
        record.endpoint_url = Some(self.tenant_endpoint(&record.tenant_id));
        record.touch();
        self.store.save(&record).await.map_err(OnboardError::Step)?;

        // Step 3: key (only reachable because the entitlement row exists).
        let api_key = self.mint_key_step(&mut record).await?;

        // Step 4: activation.
        if let Some(step) = record.step_mut("activation") {
            step.done();
        }
        record.state = OnboardingState::Active.as_str().to_string();
        record.touch();
        self.store.save(&record).await.map_err(OnboardError::Step)?;

        Ok(OnboardOutcome { record, api_key })
    }

    /// Resumes a partially-provisioned tenant (design §5.1). Re-runs from the
    /// first step that is not `done`; a fully-active tenant is returned as-is.
    pub async fn resume(&self, tenant_id: &str) -> Result<OnboardOutcome, OnboardError> {
        let mut record = self
            .store
            .get(tenant_id)
            .await
            .map_err(OnboardError::Step)?
            .ok_or_else(|| OnboardError::NotFound(tenant_id.to_string()))?;

        if record.state == OnboardingState::Terminated.as_str() {
            return Err(OnboardError::BadRequest("tenant is terminated".to_string()));
        }

        if !record.step_done("entitlement") {
            if let Some(step) = record.step_mut("entitlement") {
                step.done();
            }
            record.touch();
            self.store.save(&record).await.map_err(OnboardError::Step)?;
        }
        if !record.step_done("endpoint") {
            if let Some(step) = record.step_mut("endpoint") {
                step.done();
            }
            record.endpoint_url = Some(self.tenant_endpoint(&record.tenant_id));
            record.touch();
            self.store.save(&record).await.map_err(OnboardError::Step)?;
        }

        let api_key = if record.step_done("key") {
            None
        } else {
            self.mint_key_step(&mut record).await?
        };

        if !record.step_done("activation") {
            if let Some(step) = record.step_mut("activation") {
                step.done();
            }
        }
        record.state = OnboardingState::Active.as_str().to_string();
        record.touch();
        self.store.save(&record).await.map_err(OnboardError::Step)?;

        Ok(OnboardOutcome { record, api_key })
    }

    /// Rotates a tenant's active key, returning the new plaintext exactly once.
    pub async fn rotate_key(&self, tenant_id: &str) -> Result<OnboardOutcome, OnboardError> {
        let mut record = self
            .store
            .get(tenant_id)
            .await
            .map_err(OnboardError::Step)?
            .ok_or_else(|| OnboardError::NotFound(tenant_id.to_string()))?;
        if record.state == OnboardingState::Terminated.as_str() {
            return Err(OnboardError::BadRequest("tenant is terminated".to_string()));
        }
        let api_key = self.mint_key_step(&mut record).await?;
        Ok(OnboardOutcome { record, api_key })
    }

    /// Revokes the tenant's active key and marks the tenant terminated.
    pub async fn revoke_key(&self, tenant_id: &str, key_id: &str) -> Result<(), OnboardError> {
        let mut record = self
            .store
            .get(tenant_id)
            .await
            .map_err(OnboardError::Step)?
            .ok_or_else(|| OnboardError::NotFound(tenant_id.to_string()))?;
        // The key id is the hash prefix published in the receipt; a mismatch
        // must not revoke a key the caller did not name.
        if let Some(active) = record.key_id.as_deref() {
            if active != key_id {
                return Err(OnboardError::NotFound(format!(
                    "key {key_id} not active for tenant"
                )));
            }
        }
        if let Some(ks) = &self.key_store {
            ks.revoke_active(tenant_id)
                .await
                .map_err(OnboardError::Step)?;
        }
        record.key_id = None;
        record.state = OnboardingState::Terminated.as_str().to_string();
        record.touch();
        self.store.save(&record).await.map_err(OnboardError::Step)?;
        Ok(())
    }

    /// Returns the published limits for a tenant's tier, or `None` if the
    /// tenant is unknown.
    pub async fn limits(&self, tenant_id: &str) -> Result<Option<TenantLimits>, String> {
        Ok(self
            .store
            .get(tenant_id)
            .await?
            .and_then(|r| limits_for_tier(&r.tier)))
    }

    pub async fn get(&self, tenant_id: &str) -> Result<Option<TenantRecord>, String> {
        self.store.get(tenant_id).await
    }

    fn tenant_endpoint(&self, tenant_id: &str) -> String {
        format!("{}/api/v1/agent?tenant={tenant_id}", self.public_base)
    }

    /// Mints a key through the shared [`KeyStore`] and records the public key id
    /// (a hash prefix). A mint failure marks the step failed and leaves the
    /// record resumable rather than discarding the entitlement.
    async fn mint_key_step(
        &self,
        record: &mut TenantRecord,
    ) -> Result<Option<String>, OnboardError> {
        let Some(ks) = self.key_store.clone() else {
            if let Some(step) = record.step_mut("key") {
                step.skipped("no durable key store configured");
            }
            record.touch();
            self.store.save(record).await.map_err(OnboardError::Step)?;
            return Ok(None);
        };

        let (key, hash) = mint_material();
        if let Err(err) = ks.mint_key(&record.tenant_id, &hash, &record.tier).await {
            if let Some(step) = record.step_mut("key") {
                step.failed(err.clone());
            }
            record.state = OnboardingState::Failed.as_str().to_string();
            record.touch();
            let _ = self.store.save(record).await;
            return Err(OnboardError::Step(format!("key mint failed: {err}")));
        }
        if let Some(step) = record.step_mut("key") {
            step.done();
        }
        // Publish a stable, non-secret identifier: the first 12 hex chars of the
        // key hash. The plaintext is returned to the caller and never stored.
        record.key_id = Some(hash[..12].to_string());
        record.touch();
        self.store.save(record).await.map_err(OnboardError::Step)?;
        Ok(Some(key))
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Shared state for the onboarding routes.
#[derive(Clone)]
pub struct OnboardingApi {
    pub service: Arc<OnboardingService>,
    /// Bearer required to call `POST /v1/onboard`. A shared sandbox/onboarding
    /// token — this is the "no card to evaluate" entry credential.
    pub onboarding_token: String,
}

impl OnboardingApi {
    /// Builds the API state from the environment, or `None` when the onboarding
    /// token is unset (the routes are then not mounted, mirroring the MoR
    /// webhook). A durable store is required in production; the caller supplies
    /// it.
    pub fn from_env(service: OnboardingService) -> Option<Self> {
        let onboarding_token = std::env::var("ONBOARDING_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())?;
        Some(Self {
            service: Arc::new(service),
            onboarding_token,
        })
    }

    fn token_ok(&self, headers: &header::HeaderMap) -> bool {
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));
        match presented {
            Some(token) => {
                token
                    .as_bytes()
                    .ct_eq(self.onboarding_token.as_bytes())
                    .unwrap_u8()
                    == 1
            }
            None => false,
        }
    }
}

fn tenant_matches(ctx: &TenantContext, tenant_id: &str) -> bool {
    ctx.tenant_id == tenant_id
}

/// `POST /v1/onboard` — provision a tenant. Gated by the onboarding token, not
/// a tenant key (the caller does not have one yet).
pub async fn post_onboard(
    State(api): State<OnboardingApi>,
    headers: header::HeaderMap,
    Json(req): Json<OnboardRequest>,
) -> Result<Json<Value>, ApiError> {
    if !api.token_ok(&headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid onboarding token"})),
        ));
    }
    match api.service.provision(req).await {
        Ok(outcome) => Ok(Json(receipt_json(&outcome))),
        Err(e) => Err(e.into_error()),
    }
}

/// `GET /v1/onboard/{tenant_id}` — current onboarding state.
///
/// Gated by the onboarding token (not a tenant key): a tenant that failed at
/// the key step has no key yet, so status and resume must be reachable without
/// one. The tenant id is the identifier; the operator holds the token.
pub async fn get_onboard(
    State(api): State<OnboardingApi>,
    headers: header::HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !api.token_ok(&headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid onboarding token"})),
        ));
    }
    match api.service.get(&tenant_id).await {
        Ok(Some(record)) => Ok(Json(record_json(&record))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown tenant"})),
        )),
        Err(e) => Err((StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": e})))),
    }
}

/// `GET /v1/tenants/{tenant_id}/limits` — published, machine-readable limits.
pub async fn get_limits(
    State(api): State<OnboardingApi>,
    Extension(ctx): Extension<TenantContext>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantLimits>, ApiError> {
    if !tenant_matches(&ctx, &tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "tenant mismatch"})),
        ));
    }
    match api.service.limits(&tenant_id).await {
        Ok(Some(limits)) => Ok(Json(limits)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown tenant"})),
        )),
        Err(e) => Err((StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": e})))),
    }
}

/// `POST /v1/tenants/{tenant_id}/keys` — rotate the active key.
pub async fn rotate_key(
    State(api): State<OnboardingApi>,
    Extension(ctx): Extension<TenantContext>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !tenant_matches(&ctx, &tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "tenant mismatch"})),
        ));
    }
    match api.service.rotate_key(&tenant_id).await {
        Ok(outcome) => Ok(Json(receipt_json(&outcome))),
        Err(e) => Err(e.into_error()),
    }
}

/// `DELETE /v1/tenants/{tenant_id}/keys/{key_id}` — revoke the active key.
pub async fn revoke_key(
    State(api): State<OnboardingApi>,
    Extension(ctx): Extension<TenantContext>,
    Path((tenant_id, key_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    if !tenant_matches(&ctx, &tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "tenant mismatch"})),
        ));
    }
    match api.service.revoke_key(&tenant_id, &key_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(e.into_error()),
    }
}

/// `POST /v1/tenants/{tenant_id}/deploy` — resume/re-run provisioning.
///
/// Gated by the onboarding token, like `POST /v1/onboard`: resume is a
/// provisioning operation, and a tenant that failed at the key step has no key.
pub async fn post_deploy(
    State(api): State<OnboardingApi>,
    headers: header::HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !api.token_ok(&headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid onboarding token"})),
        ));
    }
    match api.service.resume(&tenant_id).await {
        Ok(outcome) => Ok(Json(receipt_json(&outcome))),
        Err(e) => Err(e.into_error()),
    }
}

/// `GET /v1/tenants/{tenant_id}/usage` — quota for the current period.
///
/// Monthly usage metering is wired to the billing ledger (#1591); until that
/// meter is live this reports the published quota with `metered: false` rather
/// than inventing a number.
pub async fn get_usage(
    State(api): State<OnboardingApi>,
    Extension(ctx): Extension<TenantContext>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !tenant_matches(&ctx, &tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "tenant mismatch"})),
        ));
    }
    match api.service.limits(&tenant_id).await {
        Ok(Some(limits)) => Ok(Json(json!({
            "tenant_id": tenant_id,
            "tier": limits.tier,
            "metered": false,
            "metering_note": "monthly usage metering is wired to the billing ledger; this reports the published quota",
            "quota": {
                "requests_per_minute": limits.requests_per_minute,
                "monthly_requests": limits.monthly_requests,
                "per_call_satoshi": limits.per_call_satoshi
            }
        }))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown tenant"})),
        )),
        Err(e) => Err((StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": e})))),
    }
}

/// The receipt: the machine-readable contract an agent stores. It names the
/// endpoint, the key id (never the secret), the quota and the state.
fn receipt_json(outcome: &OnboardOutcome) -> Value {
    let mut v = record_json(&outcome.record);
    if let Some(key) = &outcome.api_key {
        // Plaintext returned exactly once.
        v["api_key"] = json!(key);
    }
    v
}

fn record_json(record: &TenantRecord) -> Value {
    json!({
        "tenant_id": record.tenant_id,
        "state": record.state,
        "tier": record.tier,
        "plan": record.plan,
        "endpoint_url": record.endpoint_url,
        "key_id": record.key_id,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
        "steps": record.steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mor_webhook::{hash_key, ApplyOutcome, StoredKey, SubscriptionEvent};
    use std::sync::Mutex as StdMutex;

    /// Minimal in-memory KeyStore for exercising the onboarding service without
    /// a database. It records minted hashes so a minted key can be verified.
    #[derive(Default)]
    struct MemKeyStore {
        keys: StdMutex<HashMap<String, StoredKey>>,
        minted: StdMutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl KeyStore for MemKeyStore {
        async fn apply_event_atomic(&self, _e: &SubscriptionEvent) -> Result<ApplyOutcome, String> {
            Err("not used".to_string())
        }
        async fn record_event(&self, _a: &str, _b: &str, _c: &str) -> Result<bool, String> {
            Ok(true)
        }
        async fn revoke_active(&self, tenant_id: &str) -> Result<u64, String> {
            let mut keys = self.keys.lock().unwrap();
            let before = keys.len();
            keys.retain(|_, v| v.tenant_id != tenant_id);
            Ok((before - keys.len()) as u64)
        }
        async fn mint_key(
            &self,
            tenant_id: &str,
            api_key_hash: &str,
            tier: &str,
        ) -> Result<(), String> {
            // Mirror PgKeyStore: retire any existing active key for the tenant
            // before inserting the new one, so a rotation leaves exactly one
            // active key.
            let mut keys = self.keys.lock().unwrap();
            keys.retain(|_, v| v.tenant_id != tenant_id);
            keys.insert(
                api_key_hash.to_string(),
                StoredKey {
                    tenant_id: tenant_id.to_string(),
                    api_key_hash: api_key_hash.to_string(),
                    tier: tier.to_string(),
                },
            );
            drop(keys);
            self.minted
                .lock()
                .unwrap()
                .push((tenant_id.to_string(), api_key_hash.to_string()));
            Ok(())
        }
        async fn active_key(&self, tenant_id: &str) -> Result<Option<StoredKey>, String> {
            Ok(self
                .keys
                .lock()
                .unwrap()
                .values()
                .find(|k| k.tenant_id == tenant_id)
                .cloned())
        }
        async fn active_key_by_hash(&self, hash: &str) -> Result<Option<StoredKey>, String> {
            Ok(self.keys.lock().unwrap().get(hash).cloned())
        }
    }

    fn service_with(
        store: Arc<dyn OnboardingStore>,
        ks: Option<Arc<dyn KeyStore>>,
    ) -> OnboardingService {
        OnboardingService::new(store, ks, "https://api.example.test")
    }

    #[tokio::test]
    async fn provision_returns_active_receipt_and_minted_key() {
        let ks = Arc::new(MemKeyStore::default());
        let svc = service_with(Arc::new(InMemoryOnboardingStore::new()), Some(ks.clone()));
        let out = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-1".to_string(),
                tier: None,
            })
            .await
            .expect("provision");
        assert_eq!(out.record.state, "active");
        assert_eq!(out.record.tier, "sandbox");
        let key = out.api_key.expect("plaintext key on first provision");
        assert!(key.starts_with("cxn_agent_"));
        assert!(out.record.key_id.is_some());
        assert!(out
            .record
            .endpoint_url
            .as_deref()
            .unwrap()
            .contains(&out.record.tenant_id));
        // The minted key is resolvable in the shared store by its hash.
        let found = ks.active_key_by_hash(&hash_key(&key)).await.unwrap();
        assert_eq!(found.unwrap().tenant_id, out.record.tenant_id);
        // Every step is done.
        assert!(out.record.steps.iter().all(|s| s.state == "done"));
    }

    #[tokio::test]
    async fn provision_is_idempotent_on_repeat_key() {
        let svc = service_with(
            Arc::new(InMemoryOnboardingStore::new()),
            Some(Arc::new(MemKeyStore::default())),
        );
        let first = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-x".to_string(),
                tier: None,
            })
            .await
            .unwrap();
        let second = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-x".to_string(),
                tier: None,
            })
            .await
            .unwrap();
        // Same tenant, and no second plaintext key is handed out.
        assert_eq!(first.record.tenant_id, second.record.tenant_id);
        assert!(second.api_key.is_none());
    }

    #[tokio::test]
    async fn provision_rejects_unknown_tier() {
        let svc = service_with(Arc::new(InMemoryOnboardingStore::new()), None);
        let err = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-2".to_string(),
                tier: Some("platinum".to_string()),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, OnboardError::BadRequest(_)));
    }

    #[tokio::test]
    async fn provision_rejects_empty_idempotency_key() {
        let svc = service_with(Arc::new(InMemoryOnboardingStore::new()), None);
        let err = svc
            .provision(OnboardRequest {
                idempotency_key: "  ".to_string(),
                tier: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, OnboardError::BadRequest(_)));
    }

    #[tokio::test]
    async fn key_mint_failure_is_resumable_and_fails_closed() {
        /// A key store whose mint always fails.
        struct FailingKeyStore;
        #[async_trait]
        impl KeyStore for FailingKeyStore {
            async fn apply_event_atomic(
                &self,
                _e: &SubscriptionEvent,
            ) -> Result<ApplyOutcome, String> {
                Err("no".to_string())
            }
            async fn record_event(&self, _a: &str, _b: &str, _c: &str) -> Result<bool, String> {
                Ok(true)
            }
            async fn revoke_active(&self, _t: &str) -> Result<u64, String> {
                Ok(0)
            }
            async fn mint_key(&self, _t: &str, _h: &str, _tier: &str) -> Result<(), String> {
                Err("database unavailable".to_string())
            }
            async fn active_key(&self, _t: &str) -> Result<Option<StoredKey>, String> {
                Ok(None)
            }
            async fn active_key_by_hash(&self, _h: &str) -> Result<Option<StoredKey>, String> {
                Ok(None)
            }
        }

        let store = Arc::new(InMemoryOnboardingStore::new());
        let svc = service_with(store.clone(), Some(Arc::new(FailingKeyStore)));
        let err = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-f".to_string(),
                tier: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, OnboardError::Step(_)));

        // The record is persisted and resumable: entitlement is done, key failed.
        let rec = store
            .find_by_idempotency("idem-f")
            .await
            .unwrap()
            .expect("record persisted despite key failure");
        assert_eq!(rec.state, "failed");
        assert!(rec.step_done("entitlement"));
        assert!(!rec.step_done("key"));
    }

    #[tokio::test]
    async fn rotate_key_issues_a_new_key_and_revokes_the_old() {
        let ks = Arc::new(MemKeyStore::default());
        let svc = service_with(Arc::new(InMemoryOnboardingStore::new()), Some(ks.clone()));
        let out = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-r".to_string(),
                tier: None,
            })
            .await
            .unwrap();
        let first_key = out.api_key.unwrap();
        let rotated = svc.rotate_key(&out.record.tenant_id).await.unwrap();
        let new_key = rotated.api_key.unwrap();
        assert_ne!(first_key, new_key);
        // The old key no longer resolves; the new one does.
        assert!(ks
            .active_key_by_hash(&hash_key(&first_key))
            .await
            .unwrap()
            .is_none());
        assert!(ks
            .active_key_by_hash(&hash_key(&new_key))
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn revoke_key_terminates_the_tenant() {
        let ks = Arc::new(MemKeyStore::default());
        let svc = service_with(Arc::new(InMemoryOnboardingStore::new()), Some(ks.clone()));
        let out = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-v".to_string(),
                tier: None,
            })
            .await
            .unwrap();
        let key_id = out.record.key_id.clone().unwrap();
        svc.revoke_key(&out.record.tenant_id, &key_id)
            .await
            .unwrap();
        let rec = svc.get(&out.record.tenant_id).await.unwrap().unwrap();
        assert_eq!(rec.state, "terminated");
        assert!(rec.key_id.is_none());
    }

    #[tokio::test]
    async fn revoke_rejects_wrong_key_id() {
        let svc = service_with(
            Arc::new(InMemoryOnboardingStore::new()),
            Some(Arc::new(MemKeyStore::default())),
        );
        let out = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-w".to_string(),
                tier: None,
            })
            .await
            .unwrap();
        let err = svc
            .revoke_key(&out.record.tenant_id, "deadbeefdead")
            .await
            .unwrap_err();
        assert!(matches!(err, OnboardError::NotFound(_)));
    }

    #[tokio::test]
    async fn limits_are_published_for_every_known_tier() {
        for tier in [
            "sandbox",
            "starter",
            "indie",
            "business",
            "pro",
            "enterprise",
        ] {
            let l =
                limits_for_tier(tier).unwrap_or_else(|| panic!("tier {tier} must publish limits"));
            assert!(l.requests_per_minute > 0);
        }
        assert!(limits_for_tier("nonsense").is_none());
        // The sandbox lane is the no-card free lane.
        assert!(limits_for_tier("sandbox").unwrap().always_free);
    }

    #[tokio::test]
    async fn limits_reflect_the_tenant_tier() {
        let svc = service_with(Arc::new(InMemoryOnboardingStore::new()), None);
        let out = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-l".to_string(),
                tier: Some("business".to_string()),
            })
            .await
            .unwrap();
        let limits = svc.limits(&out.record.tenant_id).await.unwrap().unwrap();
        assert_eq!(limits.tier, "business");
    }

    #[tokio::test]
    async fn resume_completes_a_partial_tenant() {
        let ks = Arc::new(MemKeyStore::default());
        let store = Arc::new(InMemoryOnboardingStore::new());
        let svc = service_with(store.clone(), Some(ks));
        let out = svc
            .provision(OnboardRequest {
                idempotency_key: "idem-res".to_string(),
                tier: None,
            })
            .await
            .unwrap();
        // Simulate a record that stalled before activation.
        let mut rec = store.get(&out.record.tenant_id).await.unwrap().unwrap();
        rec.state = "provisioning".to_string();
        if let Some(s) = rec.step_mut("activation") {
            s.state = "pending".to_string();
        }
        store.save(&rec).await.unwrap();
        let resumed = svc.resume(&out.record.tenant_id).await.unwrap();
        assert_eq!(resumed.record.state, "active");
        assert!(resumed.record.step_done("activation"));
    }

    #[tokio::test]
    async fn provisioning_routes_require_the_onboarding_token_not_a_tenant_key() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::routing::{get, post};
        use tower::ServiceExt;

        // A minimal router mirroring routes.rs: operator routes carry no
        // managed-auth layer; tenant routes do. This pins the split so a failed
        // (key-less) tenant can still read status and resume with the token.
        let api = OnboardingApi {
            service: Arc::new(service_with(
                Arc::new(InMemoryOnboardingStore::new()),
                Some(Arc::new(MemKeyStore::default())),
            )),
            onboarding_token: "onboarding-secret".to_string(),
        };
        let operator = axum::Router::new()
            .route("/onboard", post(post_onboard))
            .route("/onboard/{tenant_id}", get(get_onboard))
            .route("/tenants/{tenant_id}/deploy", post(post_deploy))
            .with_state(api.clone());
        let app = operator;

        // No token: rejected.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/onboard/t_abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // Onboarding token: accepted (404 here only because t_abc does not exist).
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/onboard/t_abc")
                    .header(header::AUTHORIZATION, "Bearer onboarding-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
