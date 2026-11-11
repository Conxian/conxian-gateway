//! Lightning-backed managed-gateway subscription billing (Gateway #507).
//!
//! The managed gateway's missing "money in" link: today `tenant.rs` expects
//! `cxn_agent_` keys "minted by the Merchant-of-Record billing webhook", but that
//! webhook was never implemented. This module is the turnkey core:
//!
//! ```text
//! subscribe  -> create Lightning invoice (fail-closed)
//! verify     -> check settlement (fail-closed) -> mint cxn_agent_ key + Fusion JWT
//! ```
//!
//! Settlement is verified against an operator-owned invoice-check URL
//! (`LIGHTNING_INVOICE_CHECK_URL`), mirroring the Nexus #400 remediation. Without
//! a real backend, verification REJECTS — it never trusts an invoice id alone.
//!
//! Key minting and JWT issuance are injected as traits so the orchestration is
//! unit-testable; the gateway adapts `TenantKeyRegistry` (key mint) and `FusionJwt`
//! (token issue) behind those traits at wiring time.

use crate::credit_ledger::{CreditLedger, PrepayQuote, Term};
use axum::extract::Query;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_INVOICE_EXPIRY_SECONDS: u64 = 3600;
pub const DEFAULT_PERIOD_SECONDS: u64 = 30 * 24 * 3600;
/// Prepaid credits carry forward 12 months, matching OpenAI/Anthropic expiry.
pub const CREDIT_EXPIRY_SECONDS: u64 = 365 * 24 * 3600;

/// Sats-denominated subscription amounts. These mirror the only canonicalised
/// sats prices (`lib-conxian-core/src/billing.rs` `nexus::PRO_UPGRADE_SATS` /
/// `ENTERPRISE_UPGRADE_SATS`); no new pricing is invented here.
pub const PLAN_STARTER_AMOUNT_SATS: u64 = 0;
pub const PLAN_INDIE_AMOUNT_SATS: u64 = 37_700; // ~$29 (ADR-009 Indie)
pub const PLAN_BUSINESS_AMOUNT_SATS: u64 = 128_000; // ~$99 (ADR-009 Business)
pub const PLAN_PRO_AMOUNT_SATS: u64 = 100_000; // legacy alias -> Business
pub const PLAN_ENTERPRISE_AMOUNT_SATS: u64 = 1_000_000;

/// A subscription plan. Sats-denominated (Lightning-native), so no fiat->sats
/// conversion is required for the first-revenue lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionPlan {
    pub id: &'static str,
    pub amount_sats: u64,
    pub tier: &'static str,
}

pub const PLAN_PRO: SubscriptionPlan = SubscriptionPlan {
    id: "pro",
    amount_sats: PLAN_PRO_AMOUNT_SATS,
    tier: "pro",
};
pub const PLAN_ENTERPRISE: SubscriptionPlan = SubscriptionPlan {
    id: "enterprise",
    amount_sats: PLAN_ENTERPRISE_AMOUNT_SATS,
    tier: "enterprise",
};

pub const PLAN_STARTER: SubscriptionPlan = SubscriptionPlan {
    id: "starter",
    amount_sats: PLAN_STARTER_AMOUNT_SATS,
    tier: "starter",
};
pub const PLAN_INDIE: SubscriptionPlan = SubscriptionPlan {
    id: "indie",
    amount_sats: PLAN_INDIE_AMOUNT_SATS,
    tier: "indie",
};
pub const PLAN_BUSINESS: SubscriptionPlan = SubscriptionPlan {
    id: "business",
    amount_sats: PLAN_BUSINESS_AMOUNT_SATS,
    tier: "business",
};

pub fn plan_by_id(id: &str) -> Option<SubscriptionPlan> {
    match id {
        "starter" => Some(PLAN_STARTER),
        "indie" => Some(PLAN_INDIE),
        "business" => Some(PLAN_BUSINESS),
        "pro" => Some(PLAN_PRO), // legacy alias
        "enterprise" => Some(PLAN_ENTERPRISE),
        _ => None,
    }
}

/// A Lightning invoice returned by the invoice backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub invoice_id: String,
    pub payment_request: String,
    pub amount_sats: u64,
    pub expires_at: u64,
}

/// The result of a verified settlement: a minted key + JWT + tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionActivation {
    pub api_key: String,
    pub jwt: String,
    pub tier: String,
    pub expires_at: u64,
}

/// A subscription invoice enriched with the commitment quote (term, months,
/// credits). Carries the prepay amount so the invoice is for the full term.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeInvoice {
    pub invoice_id: String,
    pub payment_request: String,
    pub amount_sats: u64,
    pub expires_at: u64,
    pub term: String,
    pub service_months: u16,
    pub paid_months: u16,
    pub total_credit_sats: u64,
    pub bonus_ratio_bps: u64,
}

/// Read-only quote for a plan + term, returned by `/billing/quote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteResponse {
    pub plan: String,
    pub monthly_sats: u64,
    pub term: String,
    pub service_months: u16,
    pub paid_months: u16,
    pub prepay_sats: u64,
    pub total_credit_sats: u64,
    pub bonus_ratio_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub tenant_id: String,
    pub balance_sats: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BillingError {
    NoInvoiceCreateUrl,
    NoInvoiceCheckUrl,
    BackendHttp(u16),
    BackendUnreachable(String),
    MissingSettledIndicator,
    NotSettled,
    AmountMismatch { expected: u64, got: u64 },
    UnknownPlan(String),
    Quote(String),
    InvoiceExpired,
    ActivationUnavailable,
    KeyMint(String),
}

impl fmt::Display for BillingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInvoiceCreateUrl => {
                write!(f, "invoice create URL not configured (fail-closed)")
            }
            Self::NoInvoiceCheckUrl => write!(f, "invoice check URL not configured (fail-closed)"),
            Self::BackendHttp(status) => write!(f, "Lightning backend returned HTTP {status}"),
            Self::BackendUnreachable(detail) => {
                write!(f, "Lightning backend unreachable: {detail}")
            }
            Self::MissingSettledIndicator => {
                write!(
                    f,
                    "invoice-check response lacks an explicit settled indicator (fail-closed)"
                )
            }
            Self::NotSettled => write!(f, "Lightning invoice not settled"),
            Self::AmountMismatch { expected, got } => {
                write!(
                    f,
                    "settled amount mismatch: expected {expected} sat, got {got} sat"
                )
            }
            Self::UnknownPlan(id) => write!(f, "unknown subscription plan: {id}"),
            Self::Quote(detail) => write!(f, "credit quote failed: {detail}"),
            Self::InvoiceExpired => write!(f, "invoice expired"),
            Self::ActivationUnavailable => write!(f, "subscription activation unavailable"),
            Self::KeyMint(detail) => write!(f, "key minting failed: {detail}"),
        }
    }
}

impl std::error::Error for BillingError {}

/// Parses a term label into a `credit_ledger::Term`. Unknown/empty defaults to
/// monthly so an unlabelled subscribe remains valid.
pub fn parse_term(label: Option<&str>) -> Result<Term, String> {
    match label.unwrap_or("monthly") {
        "monthly" => Ok(Term::Monthly),
        "founding" | "founding-6m" | "6m" => Ok(Term::Founding),
        "annual" | "yearly" | "12m" => Ok(Term::Annual),
        "bulk2" | "bulk-2y" | "2y" => Ok(Term::Bulk { years: 2 }),
        "bulk3" | "bulk-3y" | "3y" => Ok(Term::Bulk { years: 3 }),
        other => Err(format!("unknown term: {other}")),
    }
}

/// Persists tenant credit balances. Injected so the orchestration is testable
/// without a database; the gateway adapts this to a durable store later.
pub trait CreditStore: Send + Sync {
    fn grant(&self, tenant_id: &str, quote: &PrepayQuote, expires_at: u64) -> Result<(), String>;
    /// Draws credits for a per-call charge, returning the remaining balance.
    /// Fails closed when the tenant has no ledger or insufficient credit.
    fn draw(&self, tenant_id: &str, amount_sats: u64, now: u64) -> Result<u64, String>;
    fn balance(&self, tenant_id: &str) -> u64;
    /// Grants relay credits directly (no commitment quote), used when a
    /// contributor redeems Genesis Credits for metered relay capacity. Defaults
    /// to fail-closed so a store that does not support it cannot silently
    /// mint unmetered credit.
    fn grant_relay_credits(&self, _tenant_id: &str, _amount_sats: u64) -> Result<u64, String> {
        Err("credit store does not support direct relay-credit grant".to_string())
    }
}

/// Forwards every [`CreditStore`] method through an `Arc`, so a single store
/// instance can be shared by the billing lane and the Genesis Credits
/// redemption sink without duplicating state.
impl<T: CreditStore + ?Sized> CreditStore for std::sync::Arc<T> {
    fn grant(&self, tenant_id: &str, quote: &PrepayQuote, expires_at: u64) -> Result<(), String> {
        (**self).grant(tenant_id, quote, expires_at)
    }
    fn draw(&self, tenant_id: &str, amount_sats: u64, now: u64) -> Result<u64, String> {
        (**self).draw(tenant_id, amount_sats, now)
    }
    fn balance(&self, tenant_id: &str) -> u64 {
        (**self).balance(tenant_id)
    }
    fn grant_relay_credits(&self, tenant_id: &str, amount_sats: u64) -> Result<u64, String> {
        (**self).grant_relay_credits(tenant_id, amount_sats)
    }
}

/// In-memory credit store (single process). Adequate for the first lane; swap
/// for a Neon/durable backend before horizontal scaling.
pub struct InMemoryCreditStore {
    ledgers: std::sync::Mutex<std::collections::HashMap<String, CreditLedger>>,
}

impl InMemoryCreditStore {
    pub fn new() -> Self {
        Self {
            ledgers: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryCreditStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CreditStore for InMemoryCreditStore {
    fn grant(&self, tenant_id: &str, quote: &PrepayQuote, expires_at: u64) -> Result<(), String> {
        let mut map = self.ledgers.lock().map_err(|e| e.to_string())?;
        let ledger = map
            .entry(tenant_id.to_string())
            .or_insert_with(|| CreditLedger::new(tenant_id, 0, expires_at));
        ledger
            .grant(quote.total_credit_sats)
            .map_err(|e| e.to_string())
    }

    fn draw(&self, tenant_id: &str, amount_sats: u64, now: u64) -> Result<u64, String> {
        let mut map = self.ledgers.lock().map_err(|e| e.to_string())?;
        match map.get_mut(tenant_id) {
            Some(ledger) => ledger.draw(amount_sats, now).map_err(|e| e.to_string()),
            None => Err(format!("no credit balance for tenant {tenant_id}")),
        }
    }

    fn balance(&self, tenant_id: &str) -> u64 {
        self.ledgers
            .lock()
            .ok()
            .and_then(|m| m.get(tenant_id).map(|l| l.balance()))
            .unwrap_or(0)
    }

    fn grant_relay_credits(&self, tenant_id: &str, amount_sats: u64) -> Result<u64, String> {
        let mut map = self.ledgers.lock().map_err(|e| e.to_string())?;
        let ledger = map
            .entry(tenant_id.to_string())
            .or_insert_with(|| CreditLedger::new(tenant_id, 0, u64::MAX));
        ledger.grant(amount_sats).map_err(|e| e.to_string())?;
        Ok(ledger.balance())
    }
}

/// Creates and checks Lightning invoices against an operator-owned backend.
#[async_trait::async_trait]
pub trait InvoiceBackend: Send + Sync {
    async fn create_invoice(
        &self,
        amount_sats: u64,
        expiry_seconds: u64,
    ) -> Result<Invoice, BillingError>;
    /// Returns `true` only when the backend confirms settlement for the amount.
    async fn check_settled(&self, invoice_id: &str, amount_sats: u64)
        -> Result<bool, BillingError>;
}

/// Mints a managed API key on activation and records its hash so the managed-auth
/// path can resolve it (gateway adapts the durable key store).
///
/// Async because the durable path persists `SHA-256(key)` to Postgres; the
/// in-memory path ignores it. The plaintext key is returned exactly once and
/// never stored.
#[async_trait::async_trait]
pub trait KeyMinter: Send + Sync {
    async fn mint(
        &self,
        tenant_id: &str,
        tier: &str,
        expires_at: u64,
    ) -> Result<String, BillingError>;
}

/// Issues a short-lived Fusion JWT on activation (gateway adapts `FusionJwt`).
pub trait TokenIssuer: Send + Sync {
    /// Configuration-only check before creating an invoice. The fixed subject
    /// is not a customer identity and never creates an entitlement.
    fn validate_plan(&self, tier: &str) -> Result<(), BillingError> {
        self.validate_activation("billing-plan-preflight", tier)
    }

    /// Check the exact identity/tier against the issuer and verifier contract
    /// before settlement lookup or credit mutations. Never return secret data.
    fn validate_activation(&self, tenant_id: &str, tier: &str) -> Result<(), BillingError>;
    fn issue(&self, tenant_id: &str, tier: &str, expires_at: u64) -> String;
}

/// Fail-closed HTTP invoice backend backed by `reqwest`. lnbits-native: the
/// create/check URLs are lnbits endpoints and the `X-Api-Key` header carries the
/// invoice key.
pub struct HttpInvoiceBackend {
    create_url: Option<String>,
    check_url: Option<String>,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl HttpInvoiceBackend {
    pub fn new(
        create_url: Option<String>,
        check_url: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            create_url,
            check_url,
            api_key,
            client: reqwest::Client::new(),
        }
    }

    pub(crate) fn from_env() -> Self {
        Self::new(
            std::env::var("LIGHTNING_INVOICE_CREATE_URL").ok(),
            std::env::var("LIGHTNING_INVOICE_CHECK_URL").ok(),
            std::env::var("LIGHTNING_INVOICE_API_KEY").ok(),
        )
    }
}

fn read_amount(body: &serde_json::Value) -> Option<u64> {
    // lnbits reports `amount` in millisatoshis (real lnbits top-level; FakeWallet
    // nests it under `details`).
    for path in ["/amount", "/details/amount"] {
        if let Some(v) = body.pointer(path) {
            if let Some(n) = v.as_u64() {
                return Some(n / 1000);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse::<u64>() {
                    return Some(n / 1000);
                }
            }
        }
    }
    // sats-native fallbacks (non-lnbits backends)
    for key in ["amount_sat", "value"] {
        if let Some(v) = body.get(key) {
            if let Some(n) = v.as_u64() {
                return Some(n);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse::<u64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn read_settled(body: &serde_json::Value) -> Option<bool> {
    // Explicit indicators first (lnbits FakeWallet uses top-level `paid`/`status`;
    // real lnbits uses `pending`/`preimage`).
    if let Some(b) = body.get("paid").and_then(|v| v.as_bool()) {
        return Some(b);
    }
    if let Some(b) = body.get("settled").and_then(|v| v.as_bool()) {
        return Some(b);
    }
    if let Some(b) = body.get("pending").and_then(|v| v.as_bool()) {
        return Some(!b);
    }
    if let Some(s) = body.get("status").and_then(|v| v.as_str()) {
        let s = s.to_ascii_lowercase();
        if ["paid", "settled", "succeeded", "complete", "completed"].contains(&s.as_str()) {
            return Some(true);
        }
        if [
            "open",
            "pending",
            "unpaid",
            "expired",
            "cancelled",
            "canceled",
        ]
        .contains(&s.as_str())
        {
            return Some(false);
        }
    }
    // A non-empty preimage confirms settlement (real lnbits / CLN style).
    if let Some(preimage) = body.get("preimage").and_then(|v| v.as_str()) {
        if !preimage.is_empty() {
            return Some(true);
        }
    }
    None
}

#[async_trait::async_trait]
impl InvoiceBackend for HttpInvoiceBackend {
    async fn create_invoice(
        &self,
        amount_sats: u64,
        expiry_seconds: u64,
    ) -> Result<Invoice, BillingError> {
        let url = self
            .create_url
            .as_deref()
            .ok_or(BillingError::NoInvoiceCreateUrl)?;
        let body =
            serde_json::json!({ "out": false, "amount": amount_sats, "expiry": expiry_seconds });
        let mut req = self.client.post(url).json(&body);
        if let Some(key) = self.api_key.as_deref() {
            req = req.header("X-Api-Key", key);
        }
        let res = req
            .send()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        if !res.status().is_success() {
            return Err(BillingError::BackendHttp(res.status().as_u16()));
        }
        let v: serde_json::Value = res
            .json()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        let invoice_id = v
            .get("payment_hash")
            .and_then(|v| v.as_str())
            .or_else(|| v.get("invoice_id").and_then(|v| v.as_str()))
            .or_else(|| v.get("id").and_then(|v| v.as_str()))
            .ok_or(BillingError::MissingSettledIndicator)?
            .to_string();
        let payment_request = v
            .get("payment_request")
            .and_then(|v| v.as_str())
            .or_else(|| v.get("bolt11").and_then(|v| v.as_str()))
            .unwrap_or_default()
            .to_string();
        let expires_at = v
            .get("expires_at")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| now_secs() + expiry_seconds);
        Ok(Invoice {
            invoice_id,
            payment_request,
            amount_sats,
            expires_at,
        })
    }

    async fn check_settled(
        &self,
        invoice_id: &str,
        amount_sats: u64,
    ) -> Result<bool, BillingError> {
        let base = self
            .check_url
            .as_deref()
            .ok_or(BillingError::NoInvoiceCheckUrl)?;
        let url = format!("{}/{}", base.trim_end_matches('/'), invoice_id);
        let mut req = self.client.get(&url);
        if let Some(key) = self.api_key.as_deref() {
            req = req.header("X-Api-Key", key);
        }
        let res = req
            .send()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        if !res.status().is_success() {
            return Err(BillingError::BackendHttp(res.status().as_u16()));
        }
        let v: serde_json::Value = res
            .json()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        let settled = read_settled(&v).ok_or(BillingError::MissingSettledIndicator)?;
        if !settled {
            return Ok(false);
        }
        if let Some(reported) = read_amount(&v) {
            if reported != amount_sats {
                return Err(BillingError::AmountMismatch {
                    expected: amount_sats,
                    got: reported,
                });
            }
        }
        Ok(true)
    }
}

/// Orchestrates subscribe -> verify. Backend, key mint, token issue and credit
/// store are injected so the logic is testable without a real Lightning node.
pub struct LightningSubscriptionService<B, M, T, C> {
    backend: B,
    minter: M,
    issuer: T,
    credits: C,
}

impl<B, M, T, C> LightningSubscriptionService<B, M, T, C>
where
    B: InvoiceBackend,
    M: KeyMinter,
    T: TokenIssuer,
    C: CreditStore,
{
    pub fn new(backend: B, minter: M, issuer: T, credits: C) -> Self {
        Self {
            backend,
            minter,
            issuer,
            credits,
        }
    }

    fn quote(&self, plan: SubscriptionPlan, term: Term) -> Result<PrepayQuote, BillingError> {
        crate::credit_ledger::quote_commitment(plan.id, plan.amount_sats, term)
            .map_err(|e| BillingError::Quote(e.to_string()))
    }

    pub async fn subscribe(
        &self,
        plan: SubscriptionPlan,
        _tenant_id: &str,
        term: Term,
    ) -> Result<SubscribeInvoice, BillingError> {
        self.issuer.validate_plan(plan.tier)?;
        let quote = self.quote(plan, term)?;
        let invoice = self
            .backend
            .create_invoice(quote.prepay_sats, DEFAULT_INVOICE_EXPIRY_SECONDS)
            .await?;
        Ok(SubscribeInvoice {
            invoice_id: invoice.invoice_id,
            payment_request: invoice.payment_request,
            amount_sats: invoice.amount_sats,
            expires_at: invoice.expires_at,
            term: term.label().to_string(),
            service_months: quote.service_months,
            paid_months: quote.paid_months,
            total_credit_sats: quote.total_credit_sats,
            bonus_ratio_bps: quote.bonus_ratio_bps,
        })
    }

    pub async fn verify(
        &self,
        plan: SubscriptionPlan,
        tenant_id: &str,
        invoice_id: &str,
        term: Term,
    ) -> Result<SubscriptionActivation, BillingError> {
        self.issuer.validate_activation(tenant_id, plan.tier)?;
        let quote = self.quote(plan, term)?;
        let settled = self
            .backend
            .check_settled(invoice_id, quote.prepay_sats)
            .await?;
        if !settled {
            return Err(BillingError::NotSettled);
        }
        let expires_at = now_secs() + DEFAULT_PERIOD_SECONDS;
        self.credits
            .grant(tenant_id, &quote, now_secs() + CREDIT_EXPIRY_SECONDS)
            .map_err(BillingError::Quote)?;
        let api_key = self
            .minter
            .mint(tenant_id, plan.tier, expires_at)
            .await
            .map_err(|e| BillingError::KeyMint(e.to_string()))?;
        let jwt = self.issuer.issue(tenant_id, plan.tier, expires_at);
        Ok(SubscriptionActivation {
            api_key,
            jwt,
            tier: plan.tier.to_string(),
            expires_at,
        })
    }

    pub fn balance(&self, tenant_id: &str) -> u64 {
        self.credits.balance(tenant_id)
    }

    pub fn draw(&self, tenant_id: &str, amount_sats: u64, now: u64) -> Result<u64, String> {
        self.credits.draw(tenant_id, amount_sats, now)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Mints an unguessable `cxn_agent_<32-hex>` key on activation and, when a
/// durable store is configured, records its `SHA-256` hash so the managed-auth
/// path (`ManagedAuth::authenticate_bearer`) can resolve it immediately.
///
/// Without the store the key is returned but not registered, so it will not
/// authenticate — that path is only for tests and explicitly out-of-band keys.
pub struct StoreKeyMinter {
    store: Option<std::sync::Arc<dyn crate::mor_webhook::KeyStore>>,
}

impl StoreKeyMinter {
    pub fn new(store: Option<std::sync::Arc<dyn crate::mor_webhook::KeyStore>>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl KeyMinter for StoreKeyMinter {
    async fn mint(
        &self,
        tenant_id: &str,
        tier: &str,
        _expires_at: u64,
    ) -> Result<String, BillingError> {
        // Shared with the MoR webhook so the key format and hash can never drift.
        let (key, hash) = crate::mor_webhook::mint_material();
        if let Some(store) = self.store.as_ref() {
            store
                .mint_key(tenant_id, &hash, tier)
                .await
                .map_err(BillingError::KeyMint)?;
        }
        Ok(key)
    }
}

/// Issues Fusion HS256 JWTs on activation via `tenant::FusionJwt`.
pub struct FusionTokenIssuer {
    fusion: crate::tenant::FusionJwt,
}

impl FusionTokenIssuer {
    pub(crate) fn from_env() -> Self {
        let secret = std::env::var("FUSION_JWT_SECRET").ok();
        Self {
            fusion: crate::tenant::FusionJwt::new(secret),
        }
    }
}

impl TokenIssuer for FusionTokenIssuer {
    fn validate_activation(&self, tenant_id: &str, tier: &str) -> Result<(), BillingError> {
        // Exercise the same signer and verifier used by managed requests.
        // This token is local preflight data and is never returned or logged.
        let candidate = self.fusion.issue(tenant_id, tier, 60);
        self.fusion
            .verify(&candidate)
            .map(|_| ())
            .map_err(|_| BillingError::ActivationUnavailable)
    }

    fn issue(&self, tenant_id: &str, tier: &str, expires_at: u64) -> String {
        let ttl = expires_at.saturating_sub(now_secs()).max(60);
        self.fusion.issue(tenant_id, tier, ttl)
    }
}

/// Concrete billing service used by the HTTP handlers (fail-closed invoice
/// backend + durable key store + shared in-memory credit store).
type BillingService = LightningSubscriptionService<
    HttpInvoiceBackend,
    StoreKeyMinter,
    FusionTokenIssuer,
    std::sync::Arc<InMemoryCreditStore>,
>;

/// The single process-wide metered credit store. Shared by the billing lane
/// (which grants on activation and draws per call) and the Genesis Credits
/// redemption sink (which grants relay credits), so a redeemed credit is
/// immediately spendable through the same balance.
fn shared_credit_store() -> &'static std::sync::Arc<InMemoryCreditStore> {
    static STORE: std::sync::OnceLock<std::sync::Arc<InMemoryCreditStore>> =
        std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Arc::new(InMemoryCreditStore::new()))
}

/// The shared metered credit store as a trait object, for callers that need to
/// grant relay credits (the Genesis Credits redemption sink).
pub(crate) fn shared_credit_sink() -> std::sync::Arc<dyn CreditStore> {
    shared_credit_store().clone()
}

fn billing_service() -> &'static std::sync::Arc<BillingService> {
    static SVC: std::sync::OnceLock<std::sync::Arc<BillingService>> = std::sync::OnceLock::new();
    SVC.get_or_init(|| {
        let backend = HttpInvoiceBackend::from_env();
        let minter = StoreKeyMinter::new(crate::routes::mor_key_store_from_env());
        let issuer = FusionTokenIssuer::from_env();
        let credits = shared_credit_store().clone();
        std::sync::Arc::new(LightningSubscriptionService::new(
            backend, minter, issuer, credits,
        ))
    })
}

#[derive(Deserialize)]
pub struct SubscribeRequest {
    pub plan: String,
    #[serde(default)]
    pub term: Option<String>,
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub plan: String,
    pub tenant_id: String,
    pub invoice_id: String,
    #[serde(default)]
    pub term: Option<String>,
}

#[derive(Deserialize)]
pub struct BalanceQuery {
    pub tenant_id: String,
}

fn parse_req_term(term: Option<&str>) -> Result<Term, (axum::http::StatusCode, String)> {
    parse_term(term).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))
}

pub(crate) async fn quote_handler(
    axum::Json(req): axum::Json<SubscribeRequest>,
) -> Result<axum::Json<QuoteResponse>, (axum::http::StatusCode, String)> {
    let plan = plan_by_id(&req.plan).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            format!("unknown plan: {}", req.plan),
        )
    })?;
    let term = parse_req_term(req.term.as_deref())?;
    let quote = crate::credit_ledger::quote_commitment(plan.id, plan.amount_sats, term)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(axum::Json(QuoteResponse {
        plan: plan.id.to_string(),
        monthly_sats: plan.amount_sats,
        term: term.label().to_string(),
        service_months: quote.service_months,
        paid_months: quote.paid_months,
        prepay_sats: quote.prepay_sats,
        total_credit_sats: quote.total_credit_sats,
        bonus_ratio_bps: quote.bonus_ratio_bps,
    }))
}

pub(crate) async fn subscribe_handler(
    axum::Json(req): axum::Json<SubscribeRequest>,
) -> Result<axum::Json<SubscribeInvoice>, (axum::http::StatusCode, String)> {
    let plan = plan_by_id(&req.plan).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            format!("unknown plan: {}", req.plan),
        )
    })?;
    let term = parse_req_term(req.term.as_deref())?;
    let invoice = billing_service()
        .subscribe(plan, "", term)
        .await
        .map_err(|e| (axum::http::StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
    Ok(axum::Json(invoice))
}

pub(crate) async fn verify_handler(
    axum::Json(req): axum::Json<VerifyRequest>,
) -> Result<axum::Json<SubscriptionActivation>, (axum::http::StatusCode, String)> {
    let plan = plan_by_id(&req.plan).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            format!("unknown plan: {}", req.plan),
        )
    })?;
    let term = parse_req_term(req.term.as_deref())?;
    let activation = billing_service()
        .verify(plan, &req.tenant_id, &req.invoice_id, term)
        .await
        .map_err(|e| {
            let status = if e == BillingError::ActivationUnavailable {
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            } else {
                axum::http::StatusCode::PAYMENT_REQUIRED
            };
            (status, e.to_string())
        })?;
    Ok(axum::Json(activation))
}

pub(crate) async fn balance_handler(
    Query(q): Query<BalanceQuery>,
) -> Result<axum::Json<BalanceResponse>, (axum::http::StatusCode, String)> {
    let balance = billing_service().balance(&q.tenant_id);
    Ok(axum::Json(BalanceResponse {
        tenant_id: q.tenant_id,
        balance_sats: balance,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoBackendCalls;

    /// Backend that behaves like a real, unconfigured HTTP backend: it fails
    /// closed without touching the network. Used to assert the preflight lets a
    /// valid activation through to invoice creation.
    struct UnconfiguredBackend;

    #[async_trait::async_trait]
    impl InvoiceBackend for UnconfiguredBackend {
        async fn create_invoice(&self, _: u64, _: u64) -> Result<Invoice, BillingError> {
            Err(BillingError::NoInvoiceCreateUrl)
        }

        async fn check_settled(&self, _: &str, _: u64) -> Result<bool, BillingError> {
            Err(BillingError::NoInvoiceCheckUrl)
        }
    }

    #[async_trait::async_trait]
    impl InvoiceBackend for NoBackendCalls {
        async fn create_invoice(&self, _: u64, _: u64) -> Result<Invoice, BillingError> {
            panic!("activation preflight must not create invoices");
        }

        async fn check_settled(&self, _: &str, _: u64) -> Result<bool, BillingError> {
            panic!("invalid activation must fail before backend lookup");
        }
    }

    #[tokio::test]
    async fn invalid_production_issuer_never_grants_credit_or_contacts_backend() {
        for (secret, tenant, plan) in [
            (None, "tenant-42", PLAN_ENTERPRISE),
            (Some(""), "tenant-42", PLAN_ENTERPRISE),
            (Some("fixture-secret"), "", PLAN_ENTERPRISE),
            (Some("fixture-secret"), "tenant\n42", PLAN_ENTERPRISE),
        ] {
            let service = LightningSubscriptionService::new(
                NoBackendCalls,
                FixedMinter,
                FusionTokenIssuer {
                    fusion: crate::tenant::FusionJwt::new(secret.map(str::to_owned)),
                },
                InMemoryCreditStore::new(),
            );
            assert_eq!(
                service
                    .verify(plan, tenant, "fixture-invoice", Term::Monthly)
                    .await
                    .unwrap_err(),
                BillingError::ActivationUnavailable
            );
            assert_eq!(service.balance(tenant), 0);
        }
    }

    #[tokio::test]
    async fn valid_issuer_and_plan_pass_activation_preflight() {
        // A valid secret + real plan tier must pass the activation preflight and
        // reach invoice creation; an unconfigured backend then fails closed with
        // NoInvoiceCreateUrl, not ActivationUnavailable. Regression guard for the
        // plan-tier allowlist that previously rejected every subscription tier.
        let service = LightningSubscriptionService::new(
            UnconfiguredBackend,
            FixedMinter,
            FusionTokenIssuer {
                fusion: crate::tenant::FusionJwt::new(Some("fixture-secret".into())),
            },
            InMemoryCreditStore::new(),
        );
        assert_eq!(
            service
                .subscribe(PLAN_INDIE, "", Term::Monthly)
                .await
                .unwrap_err(),
            BillingError::NoInvoiceCreateUrl
        );
    }

    #[tokio::test]
    async fn invalid_production_plan_never_creates_an_invoice() {
        for (secret, plan) in [(None, PLAN_ENTERPRISE), (Some(""), PLAN_ENTERPRISE)] {
            let service = LightningSubscriptionService::new(
                NoBackendCalls,
                FixedMinter,
                FusionTokenIssuer {
                    fusion: crate::tenant::FusionJwt::new(secret.map(str::to_owned)),
                },
                InMemoryCreditStore::new(),
            );
            assert_eq!(
                service
                    .subscribe(plan, "", Term::Monthly)
                    .await
                    .unwrap_err(),
                BillingError::ActivationUnavailable
            );
        }
    }

    #[tokio::test]
    async fn invalid_http_activation_returns_unavailable_without_configuration_details() {
        let error = verify_handler(axum::Json(VerifyRequest {
            plan: "enterprise".into(),
            tenant_id: String::new(),
            invoice_id: "fixture-never-looked-up".into(),
            term: Some("monthly".into()),
        }))
        .await
        .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.1, "subscription activation unavailable");
    }

    #[test]
    fn production_token_issuer_obeys_managed_auth_lifetime_contract() {
        let issuer = FusionTokenIssuer {
            fusion: crate::tenant::FusionJwt::new(Some("billing-fixture-secret".into())),
        };
        let token = issuer.issue(
            "tenant-42",
            "enterprise",
            now_secs() + DEFAULT_PERIOD_SECONDS,
        );
        let tenant = issuer
            .fusion
            .verify(&token)
            .expect("issued token accepted by managed auth");
        assert_eq!(tenant.tenant_id, "tenant-42");
        assert_eq!(tenant.tier, "enterprise");
    }

    struct MockBackend {
        settled: bool,
        amount: u64,
    }

    #[async_trait::async_trait]
    impl InvoiceBackend for MockBackend {
        async fn create_invoice(
            &self,
            amount_sats: u64,
            expiry_seconds: u64,
        ) -> Result<Invoice, BillingError> {
            Ok(Invoice {
                invoice_id: "inv_1".into(),
                payment_request: "lnbc...".into(),
                amount_sats,
                expires_at: now_secs() + expiry_seconds,
            })
        }

        async fn check_settled(
            &self,
            _invoice_id: &str,
            _amount_sats: u64,
        ) -> Result<bool, BillingError> {
            if self.amount != _amount_sats {
                return Err(BillingError::AmountMismatch {
                    expected: _amount_sats,
                    got: self.amount,
                });
            }
            Ok(self.settled)
        }
    }

    struct FixedMinter;
    #[async_trait::async_trait]
    impl KeyMinter for FixedMinter {
        async fn mint(
            &self,
            tenant_id: &str,
            _tier: &str,
            _expires_at: u64,
        ) -> Result<String, BillingError> {
            Ok(format!("cxn_agent_{tenant_id}"))
        }
    }

    struct FixedIssuer;
    impl TokenIssuer for FixedIssuer {
        fn validate_activation(&self, _tenant_id: &str, _tier: &str) -> Result<(), BillingError> {
            Ok(())
        }

        fn issue(&self, tenant_id: &str, tier: &str, _expires_at: u64) -> String {
            format!("jwt.{tenant_id}.{tier}")
        }
    }

    fn svc(
        settled: bool,
        amount: u64,
    ) -> LightningSubscriptionService<MockBackend, FixedMinter, FixedIssuer, InMemoryCreditStore>
    {
        LightningSubscriptionService::new(
            MockBackend { settled, amount },
            FixedMinter,
            FixedIssuer,
            InMemoryCreditStore::new(),
        )
    }

    #[tokio::test]
    async fn subscribe_returns_invoice() {
        let s = svc(false, PLAN_PRO_AMOUNT_SATS);
        let inv = s
            .subscribe(PLAN_PRO, "tenant-1", Term::Monthly)
            .await
            .unwrap();
        assert_eq!(inv.invoice_id, "inv_1");
        assert_eq!(inv.amount_sats, PLAN_PRO_AMOUNT_SATS);
        assert_eq!(inv.term, "monthly");
    }

    #[tokio::test]
    async fn subscribe_annual_invoices_prepay_and_credits() {
        let s = svc(false, 0);
        let inv = s.subscribe(PLAN_INDIE, "t", Term::Annual).await.unwrap();
        assert_eq!(inv.amount_sats, PLAN_INDIE_AMOUNT_SATS * 10);
        assert_eq!(inv.term, "annual");
        assert_eq!(inv.total_credit_sats, PLAN_INDIE_AMOUNT_SATS * 15);
        assert_eq!(inv.bonus_ratio_bps, 15_000);
    }

    #[tokio::test]
    async fn verify_settled_mints_key_and_jwt() {
        let s = svc(true, PLAN_PRO_AMOUNT_SATS);
        let act = s
            .verify(PLAN_PRO, "tenant-1", "inv_1", Term::Monthly)
            .await
            .unwrap();
        assert_eq!(act.api_key, "cxn_agent_tenant-1");
        assert_eq!(act.jwt, "jwt.tenant-1.pro");
        assert_eq!(act.tier, "pro");
        // Monthly grants exactly one month of credit.
        assert_eq!(s.balance("tenant-1"), PLAN_PRO_AMOUNT_SATS);
    }

    #[tokio::test]
    async fn verify_annual_grants_full_credit_balance() {
        let annual_prepay = PLAN_INDIE_AMOUNT_SATS * 10;
        let s = svc(true, annual_prepay);
        let act = s
            .verify(PLAN_INDIE, "tenant-1", "inv_1", Term::Annual)
            .await
            .unwrap();
        assert_eq!(act.tier, "indie");
        // 1.5x of prepay = 15 months of list value in sats.
        assert_eq!(s.balance("tenant-1"), PLAN_INDIE_AMOUNT_SATS * 15);
    }

    #[tokio::test]
    async fn verify_unsettled_fails_closed() {
        let s = svc(false, PLAN_PRO_AMOUNT_SATS);
        let err = s
            .verify(PLAN_PRO, "tenant-1", "inv_1", Term::Monthly)
            .await
            .unwrap_err();
        assert_eq!(err, BillingError::NotSettled);
    }

    #[tokio::test]
    async fn verify_amount_mismatch_fails_closed() {
        let s = svc(true, PLAN_PRO_AMOUNT_SATS - 1);
        let err = s
            .verify(PLAN_PRO, "tenant-1", "inv_1", Term::Monthly)
            .await
            .unwrap_err();
        assert!(matches!(err, BillingError::AmountMismatch { .. }));
    }

    #[test]
    fn parse_term_maps_labels() {
        assert_eq!(parse_term(None).unwrap(), Term::Monthly);
        assert_eq!(parse_term(Some("annual")).unwrap(), Term::Annual);
        assert_eq!(
            parse_term(Some("bulk-2y")).unwrap(),
            Term::Bulk { years: 2 }
        );
        assert_eq!(parse_term(Some("3y")).unwrap(), Term::Bulk { years: 3 });
        assert!(parse_term(Some("weekly")).is_err());
    }

    #[test]
    fn in_memory_credit_store_grants_and_reads() {
        let store = InMemoryCreditStore::new();
        let q =
            crate::credit_ledger::quote_commitment("indie", PLAN_INDIE_AMOUNT_SATS, Term::Annual)
                .unwrap();
        store.grant("t1", &q, 99).unwrap();
        assert_eq!(store.balance("t1"), q.total_credit_sats);
        assert_eq!(store.balance("unknown"), 0);
        // Draw-down returns the remaining balance and fails closed on shortfall.
        let remaining = store.draw("t1", 1_000, 1).unwrap();
        assert_eq!(remaining, q.total_credit_sats - 1_000);
        assert!(store.draw("t1", u64::MAX, 1).is_err());
        assert!(store.draw("unknown", 1, 1).is_err());
    }

    #[test]
    fn plan_catalog_matches_canonical_sats() {
        assert_eq!(PLAN_PRO_AMOUNT_SATS, 100_000);
        assert_eq!(PLAN_ENTERPRISE_AMOUNT_SATS, 1_000_000);
        assert_eq!(plan_by_id("pro"), Some(PLAN_PRO));
        assert_eq!(plan_by_id("enterprise"), Some(PLAN_ENTERPRISE));
        assert_eq!(plan_by_id("nope"), None);
    }

    #[tokio::test]
    async fn http_backend_fails_closed_without_urls() {
        let b = HttpInvoiceBackend::new(None, None, None);
        assert!(matches!(
            b.create_invoice(1000, 3600).await,
            Err(BillingError::NoInvoiceCreateUrl)
        ));
        assert!(matches!(
            b.check_settled("inv_1", 1000).await,
            Err(BillingError::NoInvoiceCheckUrl)
        ));
    }

    #[test]
    fn read_settled_handles_fakewallet_and_real_lnbits_shapes() {
        // FakeWallet unpaid: paid=false + status=pending + preimage=null.
        let fw = serde_json::json!({"paid": false, "status": "pending", "preimage": null});
        assert_eq!(read_settled(&fw), Some(false));
        // FakeWallet settled: paid=true.
        let fw_paid = serde_json::json!({"paid": true, "status": "paid", "preimage": null});
        assert_eq!(read_settled(&fw_paid), Some(true));
        // Real lnbits: pending=false => settled; pending=true => unsettled.
        assert_eq!(
            read_settled(&serde_json::json!({"pending": false})),
            Some(true)
        );
        assert_eq!(
            read_settled(&serde_json::json!({"pending": true})),
            Some(false)
        );
        // A null preimage must NOT count as settled (regression guard).
        let real_pre = serde_json::json!({"preimage": "abc123"});
        assert_eq!(read_settled(&real_pre), Some(true));
        assert_eq!(read_settled(&serde_json::json!({"preimage": null})), None);
    }

    #[test]
    fn read_amount_handles_msat_and_details() {
        // FakeWallet nests amount under `details` (msat).
        assert_eq!(
            read_amount(&serde_json::json!({"details": {"amount": 100000000}})),
            Some(100000)
        );
        // Real lnbits top-level `amount` (msat).
        assert_eq!(
            read_amount(&serde_json::json!({"amount": 100000000})),
            Some(100000)
        );
    }

    /// Records every `mint_key` call so the test can assert the durable
    /// registration that makes a minted key authenticable.
    #[derive(Default)]
    struct RecordingStore {
        minted: std::sync::Mutex<Vec<(String, String, String)>>,
    }

    #[async_trait::async_trait]
    impl crate::mor_webhook::KeyStore for RecordingStore {
        async fn apply_event_atomic(
            &self,
            _e: &crate::mor_webhook::SubscriptionEvent,
        ) -> Result<crate::mor_webhook::ApplyOutcome, String> {
            Err("not used".into())
        }
        async fn record_event(&self, _e: &str, _t: &str, _tenant: &str) -> Result<bool, String> {
            Ok(true)
        }
        async fn revoke_active(&self, _tenant: &str) -> Result<u64, String> {
            Ok(0)
        }
        async fn mint_key(
            &self,
            tenant_id: &str,
            api_key_hash: &str,
            tier: &str,
        ) -> Result<(), String> {
            self.minted.lock().unwrap().push((
                tenant_id.to_string(),
                api_key_hash.to_string(),
                tier.to_string(),
            ));
            Ok(())
        }
        async fn active_key(
            &self,
            _tenant: &str,
        ) -> Result<Option<crate::mor_webhook::StoredKey>, String> {
            Ok(None)
        }
        async fn active_key_by_hash(
            &self,
            _hash: &str,
        ) -> Result<Option<crate::mor_webhook::StoredKey>, String> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn store_minter_mints_unguessable_key_and_registers_its_hash() {
        let store = std::sync::Arc::new(RecordingStore::default());
        let minter = StoreKeyMinter::new(Some(store.clone()));
        let key = minter.mint("tenant-42", "indie", u64::MAX).await.unwrap();

        // Unguessable shape, not the old forgeable `cxn_agent_<tenant>`.
        assert!(key.starts_with("cxn_agent_"));
        assert_ne!(key, "cxn_agent_tenant-42");
        assert_eq!(key.len(), "cxn_agent_".len() + 32);

        // The SHA-256 hash of exactly this key is what the store received, so
        // `ManagedAuth::authenticate_bearer` can resolve it.
        let recorded = store.minted.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "tenant-42");
        assert_eq!(recorded[0].1, crate::mor_webhook::hash_key(&key));
        assert_eq!(recorded[0].2, "indie");
    }

    #[tokio::test]
    async fn store_minter_without_store_still_returns_a_key() {
        let minter = StoreKeyMinter::new(None);
        let key = minter.mint("tenant-9", "indie", u64::MAX).await.unwrap();
        assert!(key.starts_with("cxn_agent_"));
        assert_eq!(key.len(), "cxn_agent_".len() + 32);
    }
}
