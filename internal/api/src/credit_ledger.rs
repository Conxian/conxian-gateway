//! Prepaid credit ledger with a declining founding-tenant bonus schedule.
//!
//! `billing::LightningSubscriptionService` collects a recurring monthly fee.
//! This module adds the prepaid-commitment mechanics: a tenant may prepay a
//! number of months up front and receive front-loaded, declining bonus credits
//! that normalise to list price once the founding window ends.
//!
//! Pure by design: no network, no persistence, no clock. Callers supply the
//! prepaid amount and the current time; the module only computes grants and
//! tracks a balance. This keeps the pricing arithmetic unit-testable and stops
//! the ledger from silently depending on host time during verification.

use std::fmt;

pub const BPS_DENOMINATOR: u32 = 10_000;

/// Length of the founding window, in months.
pub const FOUNDING_MONTHS: u8 = 6;

/// Declining bonus multiplier per founding month, in basis points. Month 1 is
/// 2.0x and each month decays to list price (1.0x) by month 6.
pub const BONUS_MULTIPLIER_BPS: [u32; FOUNDING_MONTHS as usize] =
    [20_000, 18_000, 16_000, 14_000, 12_000, 10_000];

/// Bonus granted may never exceed 1.5x the prepaid amount.
pub const MAX_BONUS_RATIO_BPS: u32 = 15_000;

/// One month's credit grant within a prepaid commitment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreditGrant {
    /// 1-based index of the founding month this grant covers.
    pub month_index: u8,
    pub amount_sats: u64,
}

/// A fully computed prepaid commitment: what the tenant pays and what credits
/// the ledger should receive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepayQuote {
    pub plan_id: &'static str,
    pub monthly_sats: u64,
    pub months: u8,
    pub prepay_sats: u64,
    pub grants: Vec<CreditGrant>,
    pub total_credit_sats: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreditError {
    ZeroMonths,
    TooManyMonths { max: u8 },
    Overflow,
    InsufficientCredit { required: u64, available: u64 },
    Expired,
}

impl fmt::Display for CreditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroMonths => write!(f, "prepay months must be at least 1"),
            Self::TooManyMonths { max } => {
                write!(f, "prepay months exceed founding window of {max}")
            }
            Self::Overflow => write!(f, "credit arithmetic overflow"),
            Self::InsufficientCredit {
                required,
                available,
            } => write!(
                f,
                "insufficient credit: required {required} sat, available {available} sat"
            ),
            Self::Expired => write!(f, "credit ledger expired"),
        }
    }
}

impl std::error::Error for CreditError {}

/// Computes the credit grants for a prepaid commitment of `months` at
/// `monthly_sats` per month. Months beyond the founding window are rejected:
/// the declining bonus only applies inside the window, and list price applies
/// afterwards with no grant.
pub fn quote_prepay(
    plan_id: &'static str,
    monthly_sats: u64,
    months: u8,
) -> Result<PrepayQuote, CreditError> {
    if months == 0 {
        return Err(CreditError::ZeroMonths);
    }
    if months > FOUNDING_MONTHS {
        return Err(CreditError::TooManyMonths {
            max: FOUNDING_MONTHS,
        });
    }

    let mut grants = Vec::with_capacity(months as usize);
    let mut total_credit_sats: u64 = 0;
    for month in 1..=months {
        let multiplier_bps = BONUS_MULTIPLIER_BPS[(month - 1) as usize];
        let amount = monthly_sats
            .checked_mul(multiplier_bps as u64)
            .ok_or(CreditError::Overflow)?
            / BPS_DENOMINATOR as u64;
        total_credit_sats = total_credit_sats
            .checked_add(amount)
            .ok_or(CreditError::Overflow)?;
        grants.push(CreditGrant {
            month_index: month,
            amount_sats: amount,
        });
    }

    let prepay_sats = monthly_sats
        .checked_mul(months as u64)
        .ok_or(CreditError::Overflow)?;

    // The granted bonus must stay within the contractual 1.5x cap. A partial
    // commitment skips the cheap tail months, so the front-loaded multipliers
    // can exceed the cap; scale every grant down proportionally to honour it.
    let cap = prepay_sats
        .checked_mul(MAX_BONUS_RATIO_BPS as u64)
        .ok_or(CreditError::Overflow)?
        / BPS_DENOMINATOR as u64;
    if total_credit_sats > cap {
        let mut scaled_total: u64 = 0;
        for grant in grants.iter_mut() {
            grant.amount_sats = grant
                .amount_sats
                .checked_mul(cap)
                .ok_or(CreditError::Overflow)?
                / total_credit_sats;
            scaled_total = scaled_total
                .checked_add(grant.amount_sats)
                .ok_or(CreditError::Overflow)?;
        }
        total_credit_sats = scaled_total;
    }

    Ok(PrepayQuote {
        plan_id,
        monthly_sats,
        months,
        prepay_sats,
        grants,
        total_credit_sats,
    })
}

/// A tenant's credit balance. Callers own persistence and pass `now` explicitly
/// so expiry is deterministic in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditLedger {
    tenant_id: String,
    balance_sats: u64,
    expires_at: u64,
}

impl CreditLedger {
    pub fn new(tenant_id: impl Into<String>, initial_sats: u64, expires_at: u64) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            balance_sats: initial_sats,
            expires_at,
        }
    }

    /// Builds a ledger from a prepaid quote. `expires_at` is the carry-forward
    /// deadline (12 months in the commercial offer).
    pub fn from_quote(tenant_id: impl Into<String>, quote: &PrepayQuote, expires_at: u64) -> Self {
        Self::new(tenant_id, quote.total_credit_sats, expires_at)
    }

    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    pub fn balance(&self) -> u64 {
        self.balance_sats
    }

    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }

    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }

    /// Adds credit (e.g. a top-up or a fresh commitment).
    pub fn grant(&mut self, amount_sats: u64) -> Result<(), CreditError> {
        self.balance_sats = self
            .balance_sats
            .checked_add(amount_sats)
            .ok_or(CreditError::Overflow)?;
        Ok(())
    }

    /// Draws `amount_sats` for a call and returns the remaining balance. Fails
    /// closed on expiry or insufficient credit; it never goes negative.
    pub fn draw(&mut self, amount_sats: u64, now: u64) -> Result<u64, CreditError> {
        if self.is_expired(now) {
            return Err(CreditError::Expired);
        }
        if amount_sats > self.balance_sats {
            return Err(CreditError::InsufficientCredit {
                required: amount_sats,
                available: self.balance_sats,
            });
        }
        self.balance_sats -= amount_sats;
        Ok(self.balance_sats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sats mirrors of the ADR-009 tiers used by billing.rs.
    const INDIE_SATS: u64 = 37_700;
    const BUSINESS_SATS: u64 = 128_000;

    #[test]
    fn indie_six_month_quote_matches_offer() {
        let q = quote_prepay("indie", INDIE_SATS, 6).unwrap();
        assert_eq!(q.prepay_sats, INDIE_SATS * 6);
        let amounts: Vec<u64> = q.grants.iter().map(|g| g.amount_sats).collect();
        assert_eq!(
            amounts,
            vec![75_400, 67_860, 60_320, 52_780, 45_240, 37_700]
        );
        assert_eq!(q.total_credit_sats, 339_300);
    }

    #[test]
    fn business_six_month_quote_is_one_point_five_times() {
        let q = quote_prepay("business", BUSINESS_SATS, 6).unwrap();
        assert_eq!(q.prepay_sats, BUSINESS_SATS * 6);
        // 1.5x of prepay by construction.
        assert_eq!(q.total_credit_sats, (BUSINESS_SATS * 6 * 3) / 2);
    }

    #[test]
    fn multiplier_decays_monotonically_to_list() {
        let q = quote_prepay("indie", INDIE_SATS, 6).unwrap();
        let amounts: Vec<u64> = q.grants.iter().map(|g| g.amount_sats).collect();
        assert!(
            amounts.windows(2).all(|w| w[0] >= w[1]),
            "grant schedule must not increase: {amounts:?}"
        );
        assert_eq!(*amounts.last().unwrap(), INDIE_SATS, "month 6 == list");
    }

    #[test]
    fn zero_and_over_window_months_are_rejected() {
        assert_eq!(
            quote_prepay("indie", INDIE_SATS, 0).unwrap_err(),
            CreditError::ZeroMonths
        );
        assert_eq!(
            quote_prepay("indie", INDIE_SATS, 7).unwrap_err(),
            CreditError::TooManyMonths { max: 6 }
        );
    }

    #[test]
    fn partial_commitment_respects_bonus_cap() {
        let q = quote_prepay("indie", INDIE_SATS, 3).unwrap();
        assert_eq!(q.prepay_sats, INDIE_SATS * 3);
        // Front-loaded multipliers on a short window would exceed 1.5x; the cap
        // scales the grants back so the total still honours the limit.
        let cap = q.prepay_sats * MAX_BONUS_RATIO_BPS as u64 / BPS_DENOMINATOR as u64;
        assert!(
            q.total_credit_sats <= cap,
            "total {} exceeds cap {}",
            q.total_credit_sats,
            cap
        );
        // Still front-loaded: earlier months grant at least as much as later ones.
        let amounts: Vec<u64> = q.grants.iter().map(|g| g.amount_sats).collect();
        assert!(amounts.windows(2).all(|w| w[0] >= w[1]), "{amounts:?}");
    }

    #[test]
    fn draw_reduces_balance_and_fails_closed_on_shortfall() {
        let mut l = CreditLedger::new("t1", 1_000, 10_000);
        assert_eq!(l.draw(400, 1).unwrap(), 600);
        let err = l.draw(10_000, 1).unwrap_err();
        assert_eq!(
            err,
            CreditError::InsufficientCredit {
                required: 10_000,
                available: 600
            }
        );
        assert_eq!(l.balance(), 600, "failed draw must not mutate balance");
    }

    #[test]
    fn expired_ledger_rejects_draws() {
        let mut l = CreditLedger::new("t1", 1_000, 100);
        assert!(l.is_expired(100));
        assert_eq!(l.draw(1, 100).unwrap_err(), CreditError::Expired);
    }

    #[test]
    fn from_quote_sets_total_credit_as_balance() {
        let q = quote_prepay("indie", INDIE_SATS, 6).unwrap();
        let l = CreditLedger::from_quote("t1", &q, 99_999);
        assert_eq!(l.balance(), 339_300);
        assert_eq!(l.tenant_id(), "t1");
    }
}
