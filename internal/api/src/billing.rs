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

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_INVOICE_EXPIRY_SECONDS: u64 = 3600;
pub const DEFAULT_PERIOD_SECONDS: u64 = 30 * 24 * 3600;

/// Sats-denominated subscription amounts. These mirror the only canonicalised
/// sats prices (`lib-conxian-core/src/billing.rs` `nexus::PRO_UPGRADE_SATS` /
/// `ENTERPRISE_UPGRADE_SATS`); no new pricing is invented here.
pub const PLAN_PRO_AMOUNT_SATS: u64 = 100_000;
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

pub fn plan_by_id(id: &str) -> Option<SubscriptionPlan> {
    match id {
        "pro" => Some(PLAN_PRO),
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
    InvoiceExpired,
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
            Self::InvoiceExpired => write!(f, "invoice expired"),
        }
    }
}

impl std::error::Error for BillingError {}

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

/// Mints a managed API key on activation (gateway adapts `TenantKeyRegistry`).
pub trait KeyMinter: Send + Sync {
    fn mint(&self, tenant_id: &str, tier: &str, expires_at: u64) -> String;
}

/// Issues a short-lived Fusion JWT on activation (gateway adapts `FusionJwt`).
pub trait TokenIssuer: Send + Sync {
    fn issue(&self, tenant_id: &str, tier: &str, expires_at: u64) -> String;
}

/// Fail-closed HTTP invoice backend backed by `reqwest`.
pub struct HttpInvoiceBackend {
    create_url: Option<String>,
    check_url: Option<String>,
    client: reqwest::Client,
}

impl HttpInvoiceBackend {
    pub fn new(create_url: Option<String>, check_url: Option<String>) -> Self {
        Self {
            create_url,
            check_url,
            client: reqwest::Client::new(),
        }
    }
}

fn read_amount(body: &serde_json::Value) -> Option<u64> {
    for key in ["amount_sat", "value", "amount"] {
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
    // millisatoshi form
    for key in ["amount_msat", "value_msat"] {
        if let Some(v) = body.get(key).and_then(|v| v.as_str()) {
            if let Ok(n) = v.parse::<u64>() {
                return Some(n / 1000);
            }
        }
    }
    None
}

fn read_settled(body: &serde_json::Value) -> Option<bool> {
    if let Some(b) = body.get("settled").and_then(|v| v.as_bool()) {
        return Some(b);
    }
    if let Some(b) = body.get("paid").and_then(|v| v.as_bool()) {
        return Some(b);
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
    None
}

#[async_trait::async_trait]
impl InvoiceBackend for HttpInvoiceBackend {
    async fn create_invoice(
        &self,
        amount_sats: u64,
        expiry_seconds: u64,
    ) -> Result<Invoice, BillingError> {
        let base = self
            .create_url
            .as_deref()
            .ok_or(BillingError::NoInvoiceCreateUrl)?;
        let url = format!("{}/invoice", base.trim_end_matches('/'));
        let body =
            serde_json::json!({ "amount_sats": amount_sats, "expiry_seconds": expiry_seconds });
        let res = self
            .client
            .post(&url)
            .json(&body)
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
            .get("invoice_id")
            .and_then(|v| v.as_str())
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
        let res = self
            .client
            .get(&url)
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

/// Orchestrates subscribe -> verify. Backend, key mint and token issue are
/// injected so the logic is testable without a real Lightning node.
pub struct LightningSubscriptionService<B, M, T> {
    backend: B,
    minter: M,
    issuer: T,
}

impl<B, M, T> LightningSubscriptionService<B, M, T>
where
    B: InvoiceBackend,
    M: KeyMinter,
    T: TokenIssuer,
{
    pub fn new(backend: B, minter: M, issuer: T) -> Self {
        Self {
            backend,
            minter,
            issuer,
        }
    }

    pub async fn subscribe(
        &self,
        plan: SubscriptionPlan,
        _tenant_id: &str,
    ) -> Result<Invoice, BillingError> {
        self.backend
            .create_invoice(plan.amount_sats, DEFAULT_INVOICE_EXPIRY_SECONDS)
            .await
    }

    pub async fn verify(
        &self,
        plan: SubscriptionPlan,
        tenant_id: &str,
        invoice_id: &str,
    ) -> Result<SubscriptionActivation, BillingError> {
        let settled = self
            .backend
            .check_settled(invoice_id, plan.amount_sats)
            .await?;
        if !settled {
            return Err(BillingError::NotSettled);
        }
        let expires_at = now_secs() + DEFAULT_PERIOD_SECONDS;
        let api_key = self.minter.mint(tenant_id, plan.tier, expires_at);
        let jwt = self.issuer.issue(tenant_id, plan.tier, expires_at);
        Ok(SubscriptionActivation {
            api_key,
            jwt,
            tier: plan.tier.to_string(),
            expires_at,
        })
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    impl KeyMinter for FixedMinter {
        fn mint(&self, tenant_id: &str, _tier: &str, _expires_at: u64) -> String {
            format!("cxn_agent_{tenant_id}")
        }
    }

    struct FixedIssuer;
    impl TokenIssuer for FixedIssuer {
        fn issue(&self, tenant_id: &str, tier: &str, _expires_at: u64) -> String {
            format!("jwt.{tenant_id}.{tier}")
        }
    }

    fn svc(
        settled: bool,
        amount: u64,
    ) -> LightningSubscriptionService<MockBackend, FixedMinter, FixedIssuer> {
        LightningSubscriptionService::new(MockBackend { settled, amount }, FixedMinter, FixedIssuer)
    }

    #[tokio::test]
    async fn subscribe_returns_invoice() {
        let s = svc(false, PLAN_PRO_AMOUNT_SATS);
        let inv = s.subscribe(PLAN_PRO, "tenant-1").await.unwrap();
        assert_eq!(inv.invoice_id, "inv_1");
        assert_eq!(inv.amount_sats, PLAN_PRO_AMOUNT_SATS);
    }

    #[tokio::test]
    async fn verify_settled_mints_key_and_jwt() {
        let s = svc(true, PLAN_PRO_AMOUNT_SATS);
        let act = s.verify(PLAN_PRO, "tenant-1", "inv_1").await.unwrap();
        assert_eq!(act.api_key, "cxn_agent_tenant-1");
        assert_eq!(act.jwt, "jwt.tenant-1.pro");
        assert_eq!(act.tier, "pro");
    }

    #[tokio::test]
    async fn verify_unsettled_fails_closed() {
        let s = svc(false, PLAN_PRO_AMOUNT_SATS);
        let err = s.verify(PLAN_PRO, "tenant-1", "inv_1").await.unwrap_err();
        assert_eq!(err, BillingError::NotSettled);
    }

    #[tokio::test]
    async fn verify_amount_mismatch_fails_closed() {
        let s = svc(true, PLAN_PRO_AMOUNT_SATS - 1);
        let err = s.verify(PLAN_PRO, "tenant-1", "inv_1").await.unwrap_err();
        assert!(matches!(err, BillingError::AmountMismatch { .. }));
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
        let b = HttpInvoiceBackend::new(None, None);
        assert!(matches!(
            b.create_invoice(1000, 3600).await,
            Err(BillingError::NoInvoiceCreateUrl)
        ));
        assert!(matches!(
            b.check_settled("inv_1", 1000).await,
            Err(BillingError::NoInvoiceCheckUrl)
        ));
    }
}
