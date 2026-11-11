//! Genesis Credits — the utility-only contribution ledger.
//!
//! Implements the safe gamified capital-formation mechanic (see
//! `conxian-business/docs/CAPITAL_FORMATION_MODEL_2026_11_11.md`). Credits are
//! **earned** for verified work, are **non-transferable**, are **never
//! purchasable**, and are redeemable only for platform services (relay credits,
//! rate-limit uplift, onboarding, discounts). They carry no monetary, equity, or
//! profit-sharing value and no expectation of profit from others' efforts, so
//! they are a loyalty ledger, not a security.
//!
//! This module is deliberately pure and deterministic: it holds the accrual and
//! redemption rules. Persistence and HTTP wiring live with the caller.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Credits expire this long after they are earned, so the ledger is a loyalty
/// programme and not a store of speculative value.
pub const CREDIT_TTL_SECONDS: u64 = 365 * 24 * 60 * 60;

/// A cap on credits that can be minted per contributor per period. Bounds the
/// liability a single actor can accrue and discourages farming.
pub const MAX_CREDITS_PER_PERIOD: u64 = 20_000;

/// The verified contribution types that earn credits. Only a maintainer-confirmed
/// verification may mint — the incentive attaches to a verified outcome, never to
/// a claim (Proof Guild rule, Business #1843).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Contribution {
    /// A disproof of a real bug, confirmed.
    VerifiedDisproof,
    /// A confirmed reproduction of a reported issue.
    VerifiedReproduction,
    /// A reviewed and merged non-trivial pull request.
    MergedPullRequest,
    /// A shipped SDK integration.
    SdkIntegration,
    /// A security-audit contribution.
    SecurityAudit,
    /// A qualified enterprise pilot referral.
    EnterpriseReferral,
}

impl Contribution {
    /// Credits earned for this contribution type. Values mirror the redemption
    /// mapping in the capital-formation model.
    pub fn credits(self) -> u64 {
        match self {
            Contribution::VerifiedDisproof => 500,
            Contribution::VerifiedReproduction => 250,
            Contribution::MergedPullRequest => 100,
            Contribution::SdkIntegration => 300,
            Contribution::SecurityAudit => 1_000,
            Contribution::EnterpriseReferral => 2_000,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Contribution::VerifiedDisproof => "verified-disproof",
            Contribution::VerifiedReproduction => "verified-reproduction",
            Contribution::MergedPullRequest => "merged-pull-request",
            Contribution::SdkIntegration => "sdk-integration",
            Contribution::SecurityAudit => "security-audit",
            Contribution::EnterpriseReferral => "enterprise-referral",
        }
    }
}

/// What a credit balance can be redeemed for. Utility only — never cash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Redemption {
    /// Convert credits into metered relay credits.
    RelayCredits,
    /// A managed-tier fee discount for one month.
    TierDiscount,
    /// A rate-limit tier uplift for 90 days.
    RateLimitUplift,
    /// A guided-onboarding slot.
    OnboardingSlot,
}

impl Redemption {
    /// Credit cost of a redemption.
    pub fn cost(self) -> u64 {
        match self {
            Redemption::RelayCredits => 100,
            Redemption::TierDiscount => 500,
            Redemption::RateLimitUplift => 300,
            Redemption::OnboardingSlot => 1_000,
        }
    }

    /// Relay credits minted, when the redemption is `RelayCredits`.
    pub fn relay_credits(self) -> u64 {
        match self {
            Redemption::RelayCredits => 100_000,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreditLedgerError {
    /// The per-period cap would be exceeded by this award.
    PeriodCapExceeded { would_be: u64, cap: u64 },
    /// The balance is too low for the requested redemption.
    InsufficientCredits { required: u64, available: u64 },
    /// Arithmetic overflow.
    Overflow,
}

/// A single dated award, retained for an auditable history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditAward {
    pub contribution: String,
    pub credits: u64,
    pub awarded_at: u64,
    pub expires_at: u64,
    /// True once expiry has been applied to the live balance. Retained in the
    /// history so repeated reads or redemption attempts cannot deduct it twice.
    #[serde(default)]
    pub expiry_applied: bool,
}

/// The per-contributor Genesis Credits ledger. Non-transferable by construction:
/// credits are held against one contributor id and there is no transfer method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionLedger {
    contributor: String,
    balance: u64,
    awards: Vec<CreditAward>,
    period_minted: u64,
    period_started_at: u64,
}

impl ContributionLedger {
    /// Period length for the cap (30 days).
    pub const PERIOD_SECONDS: u64 = 30 * 24 * 60 * 60;

    pub fn new(contributor: impl Into<String>, now: u64) -> Self {
        Self {
            contributor: contributor.into(),
            balance: 0,
            awards: Vec::new(),
            period_minted: 0,
            period_started_at: now,
        }
    }

    /// Rehydrates a ledger from persisted state. Used by durable stores; the
    /// public constructors remain the only way to create a *new* ledger, so a
    /// caller cannot forge a balance.
    pub fn restore(
        contributor: impl Into<String>,
        balance: u64,
        awards: Vec<CreditAward>,
        period_minted: u64,
        period_started_at: u64,
    ) -> Self {
        Self {
            contributor: contributor.into(),
            balance,
            awards,
            period_minted,
            period_started_at,
        }
    }

    pub fn period_minted(&self) -> u64 {
        self.period_minted
    }

    pub fn period_started_at(&self) -> u64 {
        self.period_started_at
    }

    pub fn contributor(&self) -> &str {
        &self.contributor
    }

    pub fn balance(&self) -> u64 {
        self.balance
    }

    pub fn awards(&self) -> &[CreditAward] {
        &self.awards
    }

    /// Rolls the period window if it has elapsed, resetting the minted counter.
    fn roll_period(&mut self, now: u64) {
        if now.saturating_sub(self.period_started_at) >= Self::PERIOD_SECONDS {
            self.period_started_at = now;
            self.period_minted = 0;
        }
    }

    /// Awards credits for a verified contribution. Fails closed when the
    /// per-period cap would be exceeded, so the liability stays bounded.
    pub fn award(
        &mut self,
        contribution: Contribution,
        now: u64,
    ) -> Result<u64, CreditLedgerError> {
        self.roll_period(now);
        let amount = contribution.credits();
        let would_be = self
            .period_minted
            .checked_add(amount)
            .ok_or(CreditLedgerError::Overflow)?;
        if would_be > MAX_CREDITS_PER_PERIOD {
            return Err(CreditLedgerError::PeriodCapExceeded {
                would_be,
                cap: MAX_CREDITS_PER_PERIOD,
            });
        }
        self.balance = self
            .balance
            .checked_add(amount)
            .ok_or(CreditLedgerError::Overflow)?;
        self.period_minted = would_be;
        self.awards.push(CreditAward {
            contribution: contribution.as_str().to_string(),
            credits: amount,
            awarded_at: now,
            expires_at: now + CREDIT_TTL_SECONDS,
            expiry_applied: false,
        });
        Ok(self.balance)
    }

    /// Redeems credits for a platform utility. Fails closed on insufficient
    /// credit; returns the relay credits minted when applicable.
    pub fn redeem(
        &mut self,
        redemption: Redemption,
        now: u64,
    ) -> Result<RedemptionReceipt, CreditLedgerError> {
        self.prune_expired(now);
        let cost = redemption.cost();
        if cost > self.balance {
            return Err(CreditLedgerError::InsufficientCredits {
                required: cost,
                available: self.balance,
            });
        }
        self.balance -= cost;
        Ok(RedemptionReceipt {
            redemption,
            credits_spent: cost,
            relay_credits: redemption.relay_credits(),
            remaining: self.balance,
            redeemed_at: now,
        })
    }

    /// Applies each expiry exactly once while retaining the award history. A
    /// marked award is never deducted again, so repeated redemption attempts
    /// (or a reload) cannot double-count the same expiry.
    fn prune_expired(&mut self, now: u64) {
        let mut expired = 0u64;
        for award in &mut self.awards {
            if !award.expiry_applied && now >= award.expires_at {
                expired = expired.saturating_add(award.credits);
                award.expiry_applied = true;
            }
        }
        self.balance = self.balance.saturating_sub(expired.min(self.balance));
    }

    /// Aggregate view for the API surface.
    pub fn summary(&self, now: u64) -> ContributionSummary {
        let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
        for a in &self.awards {
            *by_type.entry(a.contribution.clone()).or_insert(0) += a.credits;
        }
        ContributionSummary {
            contributor: self.contributor.clone(),
            balance: self.balance,
            total_earned: self.awards.iter().map(|a| a.credits).sum(),
            period_minted: self.period_minted,
            period_cap: MAX_CREDITS_PER_PERIOD,
            awards: self.awards.len() as u64,
            by_type,
            generated_at: now,
        }
    }
}

/// A record of a completed redemption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionReceipt {
    pub redemption: Redemption,
    pub credits_spent: u64,
    pub relay_credits: u64,
    pub remaining: u64,
    pub redeemed_at: u64,
}

/// The machine-readable summary returned by the contribution API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionSummary {
    pub contributor: String,
    pub balance: u64,
    pub total_earned: u64,
    pub period_minted: u64,
    pub period_cap: u64,
    pub awards: u64,
    pub by_type: BTreeMap<String, u64>,
    pub generated_at: u64,
}

/// A durable record of one applied redemption, keyed by an idempotency key so a
/// retried request returns the same receipt instead of spending twice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedemptionRecord {
    pub redemption: String,
    pub credits_spent: u64,
    pub relay_credits: u64,
    pub remaining: u64,
    pub redeemed_at: u64,
}

/// Outcome of an award or redemption applied against a durable store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome<T> {
    /// The mutation was applied for the first time.
    Applied(T),
    /// The idempotency key had already been applied; the stored result is
    /// returned and no credit moved.
    Duplicate(T),
}

/// Persists per-contributor Genesis Credits ledgers. Injected so the accrual
/// rules stay unit-testable without a database; the gateway adapts this to a
/// durable Neon/Postgres store at wiring time.
///
/// The mutation methods are **atomic and idempotent**: they take an idempotency
/// key and apply the whole read-modify-write inside one serialized store
/// transaction, so concurrent awards cannot lose updates or bypass the
/// per-period cap, and a retried request cannot mint or spend twice. Every
/// method fails closed by returning `Err` rather than silently succeeding.
#[async_trait::async_trait]
pub trait ContributionStore: Send + Sync {
    async fn get(&self, contributor: &str) -> Result<Option<ContributionLedger>, String>;

    /// Atomically awards `contribution` to `contributor`, serialized against
    /// concurrent awards so the per-period cap is enforced exactly. Repeating
    /// the call with the same `idempotency_key` returns the prior result and
    /// mints nothing further.
    async fn award(
        &self,
        contributor: &str,
        contribution: Contribution,
        idempotency_key: &str,
        now: u64,
    ) -> Result<ApplyOutcome<ContributionSummary>, String>;

    /// Atomically spends `redemption` for `contributor`. The debit is applied
    /// to the durable ledger, and the relay-credit grant is handed to `grant`,
    /// which is keyed by `idempotency_key` so an outbox retry cannot
    /// double-grant. Repeating the call with the same `idempotency_key` returns
    /// the prior receipt and moves no credit.
    async fn redeem(
        &self,
        contributor: &str,
        redemption: Redemption,
        idempotency_key: &str,
        now: u64,
        grant: &(dyn for<'a> Fn(u64, &'a str) -> Result<(), String> + Send + Sync),
    ) -> Result<ApplyOutcome<RedemptionReceipt>, String>;
}

/// In-memory store used by unit tests and as the process-local fallback. It is
/// deliberately not durable: the production lane mounts the routes with a
/// durable store so a balance survives a restart.
///
/// Every mutation runs under one mutex, so the award/redemption critical section
/// (cap check, expiry application, debit, idempotency record, and the relay
/// grant) is atomic within the process.
#[derive(Default)]
pub struct InMemoryContributionStore {
    inner: Mutex<InMemoryState>,
}

#[derive(Default)]
struct InMemoryState {
    ledgers: BTreeMap<String, ContributionLedger>,
    award_keys: BTreeMap<String, ContributionSummary>,
    redeem_keys: BTreeMap<String, RedemptionReceipt>,
}

impl InMemoryContributionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl ContributionStore for InMemoryContributionStore {
    async fn get(&self, contributor: &str) -> Result<Option<ContributionLedger>, String> {
        let state = self.inner.lock().map_err(|e| e.to_string())?;
        Ok(state.ledgers.get(contributor).cloned())
    }

    async fn award(
        &self,
        contributor: &str,
        contribution: Contribution,
        idempotency_key: &str,
        now: u64,
    ) -> Result<ApplyOutcome<ContributionSummary>, String> {
        if idempotency_key.is_empty() {
            return Err("idempotency key must not be empty".to_string());
        }
        let mut state = self.inner.lock().map_err(|e| e.to_string())?;
        if let Some(prior) = state.award_keys.get(idempotency_key) {
            return Ok(ApplyOutcome::Duplicate(prior.clone()));
        }
        let ledger = state
            .ledgers
            .entry(contributor.to_string())
            .or_insert_with(|| ContributionLedger::new(contributor, now));
        ledger
            .award(contribution, now)
            .map_err(|e| format!("{e:?}"))?;
        let summary = ledger.summary(now);
        state
            .award_keys
            .insert(idempotency_key.to_string(), summary.clone());
        Ok(ApplyOutcome::Applied(summary))
    }

    async fn redeem(
        &self,
        contributor: &str,
        redemption: Redemption,
        idempotency_key: &str,
        now: u64,
        grant: &(dyn for<'a> Fn(u64, &'a str) -> Result<(), String> + Send + Sync),
    ) -> Result<ApplyOutcome<RedemptionReceipt>, String> {
        if idempotency_key.is_empty() {
            return Err("idempotency key must not be empty".to_string());
        }
        let mut state = self.inner.lock().map_err(|e| e.to_string())?;
        if let Some(prior) = state.redeem_keys.get(idempotency_key) {
            return Ok(ApplyOutcome::Duplicate(prior.clone()));
        }
        let ledger = state
            .ledgers
            .entry(contributor.to_string())
            .or_insert_with(|| ContributionLedger::new(contributor, now));
        // Apply the debit to a working copy; the grant runs before the debit is
        // recorded so a failed grant leaves the ledger unchanged.
        let mut working = ledger.clone();
        let receipt = working
            .redeem(redemption, now)
            .map_err(|e| format!("{e:?}"))?;
        if receipt.relay_credits > 0 {
            grant(receipt.relay_credits, idempotency_key)?;
        }
        *ledger = working;
        state
            .redeem_keys
            .insert(idempotency_key.to_string(), receipt.clone());
        Ok(ApplyOutcome::Applied(receipt))
    }
}

/// The Genesis Credits API state. Gated by a single operator token
/// (`CONTRIBUTION_LEDGER_TOKEN`): the operator confirms the verified work that
/// earns a credit, so only a maintainer-confirmed award may mint.
#[derive(Clone)]
pub struct ContributionApi {
    store: std::sync::Arc<dyn ContributionStore>,
    /// Where redeemed relay credits land: the same metered credit store the
    /// billing lane draws from, so a redemption is spendable immediately. When
    /// absent, a relay-credit redemption fails closed rather than burning
    /// contribution credits for capacity that was never minted.
    relay_sink: Option<std::sync::Arc<dyn crate::billing::CreditStore>>,
    operator_token: String,
}

impl ContributionApi {
    /// Builds the API from the environment, or `None` when the operator token is
    /// unset — the routes are then not mounted (fail closed), mirroring the
    /// onboarding and MoR-webhook lanes.
    pub fn from_env(
        store: std::sync::Arc<dyn ContributionStore>,
        relay_sink: Option<std::sync::Arc<dyn crate::billing::CreditStore>>,
    ) -> Option<Self> {
        let operator_token = std::env::var("CONTRIBUTION_LEDGER_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())?;
        Some(Self {
            store,
            relay_sink,
            operator_token,
        })
    }

    pub fn token_ok(&self, presented: Option<&str>) -> bool {
        use subtle::ConstantTimeEq;
        match presented {
            Some(token) => {
                token
                    .as_bytes()
                    .ct_eq(self.operator_token.as_bytes())
                    .unwrap_u8()
                    == 1
            }
            None => false,
        }
    }

    /// Grants redeemed relay credits into the metered credit store. Fails
    /// closed when no sink is configured. The `idempotency_key` makes the grant
    /// a no-op on an outbox retry.
    fn grant_relay_credits(
        &self,
        tenant_id: &str,
        amount_sats: u64,
        idempotency_key: &str,
    ) -> Result<u64, String> {
        match &self.relay_sink {
            Some(sink) => sink.grant_relay_credits(tenant_id, amount_sats, idempotency_key),
            None => Err("no relay-credit sink configured".to_string()),
        }
    }

    /// Loads the ledger for a contributor, or a fresh zero-balance ledger when
    /// none exists yet (so a summary is always well-formed).
    async fn load(&self, contributor: &str, now: u64) -> Result<ContributionLedger, String> {
        Ok(self
            .store
            .get(contributor)
            .await?
            .unwrap_or_else(|| ContributionLedger::new(contributor, now)))
    }
}

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parses a contribution type from its wire label (inverse of
/// [`Contribution::as_str`]).
pub fn parse_contribution(label: &str) -> Option<Contribution> {
    Some(match label {
        "verified-disproof" => Contribution::VerifiedDisproof,
        "verified-reproduction" => Contribution::VerifiedReproduction,
        "merged-pull-request" => Contribution::MergedPullRequest,
        "sdk-integration" => Contribution::SdkIntegration,
        "security-audit" => Contribution::SecurityAudit,
        "enterprise-referral" => Contribution::EnterpriseReferral,
        _ => return None,
    })
}

/// Parses a redemption type from its wire label (inverse of the serde names).
pub fn parse_redemption(label: &str) -> Option<Redemption> {
    Some(match label {
        "relay-credits" => Redemption::RelayCredits,
        "tier-discount" => Redemption::TierDiscount,
        "rate-limit-uplift" => Redemption::RateLimitUplift,
        "onboarding-slot" => Redemption::OnboardingSlot,
        _ => return None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwardRequest {
    pub contributor: String,
    pub contribution: String,
    /// Client-supplied idempotency key. A retry with the same key returns the
    /// prior result and mints nothing further.
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemRequest {
    pub contributor: String,
    pub redemption: String,
    /// Client-supplied idempotency key. A retry with the same key returns the
    /// prior receipt and spends nothing further.
    pub idempotency_key: String,
}

/// API error tuple, matching the crate-wide `(StatusCode, String)` convention.
type ApiError = (axum::http::StatusCode, String);

fn bearer(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
}

fn authorize(api: &ContributionApi, headers: &axum::http::HeaderMap) -> Result<(), ApiError> {
    if api.token_ok(bearer(headers)) {
        Ok(())
    } else {
        Err((
            axum::http::StatusCode::UNAUTHORIZED,
            "invalid contribution-ledger operator token".to_string(),
        ))
    }
}

/// `POST /v1/contributions/award` — mint credits for a maintainer-confirmed
/// contribution. Operator-token gated: the incentive attaches to a verified
/// outcome, never to a self-reported claim. The award is applied atomically and
/// idempotently by the store, so concurrent awards cannot bypass the cap and a
/// retry cannot mint twice.
pub async fn award_handler(
    axum::extract::State(api): axum::extract::State<ContributionApi>,
    headers: axum::http::HeaderMap,
    axum::Json(req): axum::Json<AwardRequest>,
) -> Result<axum::Json<ContributionSummary>, ApiError> {
    authorize(&api, &headers)?;
    let contribution = parse_contribution(&req.contribution).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            format!("unknown contribution: {}", req.contribution),
        )
    })?;
    if req.contributor.trim().is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "contributor must not be empty".to_string(),
        ));
    }
    if req.idempotency_key.trim().is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "idempotency_key must not be empty".to_string(),
        ));
    }
    let now = now_secs();
    let outcome = api
        .store
        .award(&req.contributor, contribution, &req.idempotency_key, now)
        .await
        .map_err(|e| {
            let status = if e.contains("PeriodCapExceeded") {
                axum::http::StatusCode::CONFLICT
            } else {
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            };
            (status, e)
        })?;
    Ok(axum::Json(match outcome {
        ApplyOutcome::Applied(summary) | ApplyOutcome::Duplicate(summary) => summary,
    }))
}

/// `GET /v1/contributions/{contributor}` — the machine-readable summary.
pub async fn summary_handler(
    axum::extract::State(api): axum::extract::State<ContributionApi>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(contributor): axum::extract::Path<String>,
) -> Result<axum::Json<ContributionSummary>, ApiError> {
    authorize(&api, &headers)?;
    let now = now_secs();
    let ledger = api
        .load(&contributor, now)
        .await
        .map_err(|e| (axum::http::StatusCode::SERVICE_UNAVAILABLE, e))?;
    Ok(axum::Json(ledger.summary(now)))
}

/// `POST /v1/contributions/redeem` — spend credits on a platform utility.
///
/// The debit and the relay-credit grant are applied inside one serialized,
/// idempotent store transaction, so a failed grant leaves the balance unchanged
/// and a retry cannot double-spend or double-grant.
pub async fn redeem_handler(
    axum::extract::State(api): axum::extract::State<ContributionApi>,
    headers: axum::http::HeaderMap,
    axum::Json(req): axum::Json<RedeemRequest>,
) -> Result<axum::Json<RedemptionReceipt>, ApiError> {
    authorize(&api, &headers)?;
    let redemption = parse_redemption(&req.redemption).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            format!("unknown redemption: {}", req.redemption),
        )
    })?;
    if req.contributor.trim().is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "contributor must not be empty".to_string(),
        ));
    }
    if req.idempotency_key.trim().is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "idempotency_key must not be empty".to_string(),
        ));
    }
    let now = now_secs();
    // The grant closure is keyed by the request's idempotency key, so an outbox
    // replay cannot double-grant. It fails closed when no sink is configured, so
    // a relay redemption without a sink never burns the contribution credits.
    let grant = |relay_credits: u64, key: &str| -> Result<(), String> {
        api.grant_relay_credits(&req.contributor, relay_credits, key)
            .map(|_| ())
    };
    let outcome = api
        .store
        .redeem(
            &req.contributor,
            redemption,
            &req.idempotency_key,
            now,
            &grant,
        )
        .await
        .map_err(|e| {
            let status = if e.contains("InsufficientCredits") {
                axum::http::StatusCode::PAYMENT_REQUIRED
            } else if e.contains("PeriodCapExceeded") {
                axum::http::StatusCode::CONFLICT
            } else {
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            };
            (status, e)
        })?;
    Ok(axum::Json(match outcome {
        ApplyOutcome::Applied(receipt) | ApplyOutcome::Duplicate(receipt) => receipt,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn award_mints_credits_and_records_history() {
        let mut l = ContributionLedger::new("c-1", 0);
        assert_eq!(l.award(Contribution::VerifiedDisproof, 0).unwrap(), 500);
        assert_eq!(l.award(Contribution::MergedPullRequest, 0).unwrap(), 600);
        assert_eq!(l.balance(), 600);
        assert_eq!(l.awards().len(), 2);
        let s = l.summary(0);
        assert_eq!(s.total_earned, 600);
        assert_eq!(s.by_type.get("verified-disproof"), Some(&500));
    }

    #[test]
    fn period_cap_fails_closed() {
        let mut l = ContributionLedger::new("c-2", 0);
        for _ in 0..10 {
            l.award(Contribution::EnterpriseReferral, 0).unwrap();
        }
        assert_eq!(l.balance(), MAX_CREDITS_PER_PERIOD);
        match l.award(Contribution::EnterpriseReferral, 0) {
            Err(CreditLedgerError::PeriodCapExceeded { would_be, cap }) => {
                assert_eq!(cap, MAX_CREDITS_PER_PERIOD);
                assert!(would_be > cap);
            }
            other => panic!("expected PeriodCapExceeded, got {other:?}"),
        }
    }

    #[test]
    fn period_roll_resets_the_cap() {
        let mut l = ContributionLedger::new("c-3", 0);
        for _ in 0..10 {
            l.award(Contribution::EnterpriseReferral, 0).unwrap();
        }
        assert!(l.award(Contribution::EnterpriseReferral, 0).is_err());
        let next = ContributionLedger::PERIOD_SECONDS + 1;
        assert!(l.award(Contribution::EnterpriseReferral, next).is_ok());
    }

    #[test]
    fn redeem_relay_credits_returns_metered_credits() {
        let mut l = ContributionLedger::new("c-4", 0);
        l.award(Contribution::VerifiedDisproof, 0).unwrap();
        let r = l.redeem(Redemption::RelayCredits, 0).unwrap();
        assert_eq!(r.credits_spent, 100);
        assert_eq!(r.relay_credits, 100_000);
        assert_eq!(r.remaining, 400);
    }

    #[test]
    fn redeem_fails_closed_on_insufficient_credits() {
        let mut l = ContributionLedger::new("c-5", 0);
        l.award(Contribution::MergedPullRequest, 0).unwrap();
        match l.redeem(Redemption::OnboardingSlot, 0) {
            Err(CreditLedgerError::InsufficientCredits {
                required,
                available,
            }) => {
                assert_eq!(required, 1_000);
                assert_eq!(available, 100);
            }
            other => panic!("expected InsufficientCredits, got {other:?}"),
        }
        assert_eq!(l.balance(), 100);
    }

    #[test]
    fn expired_awards_stop_counting_toward_the_balance() {
        let mut l = ContributionLedger::new("c-6", 0);
        l.award(Contribution::VerifiedDisproof, 0).unwrap();
        let later = CREDIT_TTL_SECONDS + 1;
        match l.redeem(Redemption::RelayCredits, later) {
            Err(CreditLedgerError::InsufficientCredits { available, .. }) => {
                assert_eq!(available, 0)
            }
            other => panic!("expected InsufficientCredits, got {other:?}"),
        }
    }

    #[test]
    fn credits_are_non_transferable_by_construction() {
        // There is deliberately no `transfer` method; the only mutation paths are
        // award (verified work) and redeem (platform utility).
        let l = ContributionLedger::new("c-7", 0);
        assert_eq!(l.contributor(), "c-7");
        let s = l.summary(0);
        assert_eq!(s.contributor, "c-7");
    }

    #[test]
    fn expired_awards_are_applied_only_once() {
        let mut l = ContributionLedger::new("c-expiry-once", 0);
        l.award(Contribution::VerifiedDisproof, 0).unwrap();
        l.award(Contribution::SecurityAudit, CREDIT_TTL_SECONDS)
            .unwrap();
        let later = CREDIT_TTL_SECONDS + 1;
        let first = l.redeem(Redemption::RelayCredits, later).unwrap();
        assert_eq!(first.remaining, 900);
        let second = l.redeem(Redemption::RelayCredits, later).unwrap();
        assert_eq!(second.remaining, 800);
    }

    fn api(token: &str) -> ContributionApi {
        ContributionApi {
            store: std::sync::Arc::new(InMemoryContributionStore::new()),
            relay_sink: None,
            operator_token: token.to_string(),
        }
    }

    #[tokio::test]
    async fn award_handler_mints_and_persists_for_the_operator() {
        let api = api("op-secret");
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer op-secret".parse().unwrap(),
        );
        let summary = award_handler(
            axum::extract::State(api.clone()),
            headers.clone(),
            axum::Json(AwardRequest {
                contributor: "alice".into(),
                contribution: "security-audit".into(),
                idempotency_key: "k-alice".into(),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(summary.balance, 1_000);
        // The persisted ledger survives a fresh load.
        let reloaded = summary_handler(
            axum::extract::State(api),
            headers,
            axum::extract::Path("alice".to_string()),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(reloaded.balance, 1_000);
        assert_eq!(reloaded.by_type.get("security-audit"), Some(&1_000));
    }

    #[tokio::test]
    async fn award_handler_rejects_a_missing_or_wrong_token() {
        let api = api("op-secret");
        let err = award_handler(
            axum::extract::State(api.clone()),
            axum::http::HeaderMap::new(),
            axum::Json(AwardRequest {
                contributor: "alice".into(),
                contribution: "security-audit".into(),
                idempotency_key: "k-alice".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::UNAUTHORIZED);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer wrong".parse().unwrap(),
        );
        let err = award_handler(
            axum::extract::State(api),
            headers,
            axum::Json(AwardRequest {
                contributor: "alice".into(),
                contribution: "security-audit".into(),
                idempotency_key: "k-alice".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn award_handler_rejects_an_unknown_contribution() {
        let api = api("op-secret");
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer op-secret".parse().unwrap(),
        );
        let err = award_handler(
            axum::extract::State(api),
            headers,
            axum::Json(AwardRequest {
                contributor: "alice".into(),
                contribution: "free-money".into(),
                idempotency_key: "k-alice-free".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn redeem_handler_fails_closed_without_a_relay_sink() {
        let api = api("op-secret");
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer op-secret".parse().unwrap(),
        );
        let _ = award_handler(
            axum::extract::State(api.clone()),
            headers.clone(),
            axum::Json(AwardRequest {
                contributor: "bob".into(),
                contribution: "verified-disproof".into(),
                idempotency_key: "k-bob".into(),
            }),
        )
        .await
        .expect("award succeeds");
        // No sink configured -> the relay-credit redemption must not burn the
        // contribution credits.
        let err = redeem_handler(
            axum::extract::State(api.clone()),
            headers.clone(),
            axum::Json(RedeemRequest {
                contributor: "bob".into(),
                redemption: "relay-credits".into(),
                idempotency_key: "r-bob".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::SERVICE_UNAVAILABLE);
        let after = summary_handler(
            axum::extract::State(api),
            headers,
            axum::extract::Path("bob".to_string()),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(after.balance, 500, "credits were not burned");
    }

    #[tokio::test]
    async fn redeem_handler_grants_relay_credits_into_the_sink() {
        let sink = std::sync::Arc::new(crate::billing::InMemoryCreditStore::new());
        let api = ContributionApi {
            store: std::sync::Arc::new(InMemoryContributionStore::new()),
            relay_sink: Some(sink.clone()),
            operator_token: "op-secret".to_string(),
        };
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer op-secret".parse().unwrap(),
        );
        let _ = award_handler(
            axum::extract::State(api.clone()),
            headers.clone(),
            axum::Json(AwardRequest {
                contributor: "carol".into(),
                contribution: "verified-disproof".into(),
                idempotency_key: "k-carol".into(),
            }),
        )
        .await
        .expect("award succeeds");
        let receipt = redeem_handler(
            axum::extract::State(api),
            headers,
            axum::Json(RedeemRequest {
                contributor: "carol".into(),
                redemption: "relay-credits".into(),
                idempotency_key: "r-carol".into(),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(receipt.relay_credits, 100_000);
        assert_eq!(receipt.remaining, 400);
        use crate::billing::CreditStore;
        assert_eq!(sink.balance("carol"), 100_000);
    }

    #[tokio::test]
    async fn award_is_idempotent_on_the_key() {
        let store = InMemoryContributionStore::new();
        let first = store
            .award("dave", Contribution::SecurityAudit, "k-1", 0)
            .await
            .unwrap();
        let second = store
            .award("dave", Contribution::SecurityAudit, "k-1", 0)
            .await
            .unwrap();
        assert!(matches!(first, ApplyOutcome::Applied(_)));
        assert!(matches!(second, ApplyOutcome::Duplicate(_)));
        let ledger = store.get("dave").await.unwrap().unwrap();
        assert_eq!(ledger.balance(), 1_000, "a retry did not mint twice");
    }

    #[tokio::test]
    async fn redeem_is_idempotent_on_the_key() {
        let store = InMemoryContributionStore::new();
        let sink = crate::billing::InMemoryCreditStore::new();
        let grant = |amount: u64, key: &str| {
            use crate::billing::CreditStore;
            sink.grant_relay_credits("erin", amount, key).map(|_| ())
        };
        store
            .award("erin", Contribution::VerifiedDisproof, "a-1", 0)
            .await
            .unwrap();
        let first = store
            .redeem("erin", Redemption::RelayCredits, "r-1", 0, &grant)
            .await
            .unwrap();
        let second = store
            .redeem("erin", Redemption::RelayCredits, "r-1", 0, &grant)
            .await
            .unwrap();
        assert!(matches!(first, ApplyOutcome::Applied(_)));
        assert!(matches!(second, ApplyOutcome::Duplicate(_)));
        let ledger = store.get("erin").await.unwrap().unwrap();
        assert_eq!(ledger.balance(), 400, "a retry did not spend twice");
        use crate::billing::CreditStore;
        assert_eq!(
            sink.balance("erin"),
            100_000,
            "a retry did not double-grant relay credits"
        );
    }

    #[tokio::test]
    async fn concurrent_awards_cannot_bypass_the_period_cap() {
        let store = std::sync::Arc::new(InMemoryContributionStore::new());
        // Each award is 2_000 credits and the cap is 20_000, so exactly ten may
        // succeed. Fire twelve concurrently and assert the cap held.
        let mut handles = Vec::new();
        for i in 0..12 {
            let store = store.clone();
            handles.push(tokio::spawn(async move {
                store
                    .award(
                        "frank",
                        Contribution::EnterpriseReferral,
                        &format!("cap-{i}"),
                        0,
                    )
                    .await
            }));
        }
        let mut applied = 0;
        for h in handles {
            if matches!(h.await.unwrap(), Ok(ApplyOutcome::Applied(_))) {
                applied += 1;
            }
        }
        assert_eq!(applied, 10, "the cap admitted exactly ten awards");
        let ledger = store.get("frank").await.unwrap().unwrap();
        assert_eq!(ledger.balance(), MAX_CREDITS_PER_PERIOD);
    }

    #[tokio::test]
    async fn redeem_requires_a_non_empty_idempotency_key() {
        let store = InMemoryContributionStore::new();
        store
            .award("gina", Contribution::VerifiedDisproof, "a-g", 0)
            .await
            .unwrap();
        let grant = |_: u64, _: &str| Ok(());
        let err = store
            .redeem("gina", Redemption::RelayCredits, "", 0, &grant)
            .await
            .unwrap_err();
        assert!(err.contains("idempotency key"));
        let ledger = store.get("gina").await.unwrap().unwrap();
        assert_eq!(ledger.balance(), 500, "a keyless redeem moved no credit");
    }
}
