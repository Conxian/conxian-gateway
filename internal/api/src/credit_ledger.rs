//! Prepaid credit ledger with a declining founding-tenant bonus schedule.
//!
//! `billing::LightningSubscriptionService` collects a recurring monthly fee.
//! This module adds the prepaid-commitment mechanics: a tenant prepays a term
//! up front and receives usage credits. Credits — not a discounted monthly
//! price — are granted so list price stays intact for reporting and valuation.
//!
//! Two independent levers combine:
//!   * a one-time **founding bonus** on the first six months (2.0x -> 1.0x), and
//!   * a **term discount** expressed as months free (annual, multi-year bulk).
//!
//! Both are bounded by a hard 1.5x credit-to-prepay ceiling, so no commitment
//! can grant more than one and a half times what the tenant pays.
//!
//! Pure by design: no network, no persistence, no clock. Callers supply the
//! prepaid amount and the current time; the module only computes grants and
//! tracks a balance.

use std::fmt;

pub const BPS_DENOMINATOR: u64 = 10_000;

/// Length of the founding bonus window, in months.
pub const FOUNDING_MONTHS: u16 = 6;

/// Declining bonus multiplier per founding month, in basis points. Month 1 is
/// 2.0x and each month decays to list price (1.0x) by month 6.
pub const BONUS_MULTIPLIER_BPS: [u32; FOUNDING_MONTHS as usize] =
    [20_000, 18_000, 16_000, 14_000, 12_000, 10_000];

/// Credits granted may never exceed 1.5x the prepaid amount.
pub const MAX_BONUS_RATIO_BPS: u64 = 15_000;

/// Longest supported bulk commitment, in years.
pub const MAX_BULK_YEARS: u8 = 3;

/// A prepaid commitment shape: how many months of service it buys and how many
/// of those months the tenant is charged for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term {
    /// List price, month to month, no commitment and no bonus.
    Monthly,
    /// Six-month founding commitment: 1.5x credits, declining to list.
    Founding,
    /// Twelve months of service for the price of ten (two months free).
    Annual,
    /// Multi-year bulk: deeper months-free, same 1.5x ceiling.
    Bulk { years: u8 },
}

impl Term {
    /// Months of service the commitment covers.
    pub fn service_months(self) -> u16 {
        match self {
            Self::Monthly => 1,
            Self::Founding => 6,
            Self::Annual => 12,
            Self::Bulk { years } => u16::from(years) * 12,
        }
    }

    /// Months the tenant actually pays for.
    pub fn paid_months(self) -> u16 {
        match self {
            Self::Monthly => 1,
            Self::Founding => 6,
            Self::Annual => 10,
            // Benchmarked against 2026 SaaS norms: annual 15-25% off, then
            // +5-10% per additional year. Annual = 2 free (16.7%); 2y = 6 free
            // (25%); 3y = 10 free (27.8%). All sit exactly at the 1.5x ceiling.
            Self::Bulk { years } => {
                let years = u16::from(years);
                years * 9 - (years - 2)
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::Founding => "founding-6m",
            Self::Annual => "annual",
            Self::Bulk { .. } => "bulk",
        }
    }
}

/// One month's credit grant within a commitment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreditGrant {
    /// 1-based index of the service month this grant covers.
    pub month_index: u16,
    pub amount_sats: u64,
}

/// A fully computed commitment: what the tenant pays and what credits the
/// ledger should receive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepayQuote {
    pub plan_id: &'static str,
    pub monthly_sats: u64,
    pub service_months: u16,
    pub paid_months: u16,
    pub prepay_sats: u64,
    pub effective_monthly_sats: u64,
    pub grants: Vec<CreditGrant>,
    pub total_credit_sats: u64,
    /// `total_credit_sats / prepay_sats` in basis points (10_000 == 1.0x).
    pub bonus_ratio_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreditError {
    ZeroMonths,
    TooManyMonths { max: u16 },
    UnsupportedBulkTerm { years: u8, max: u8 },
    Overflow,
    InsufficientCredit { required: u64, available: u64 },
    Expired,
}

impl fmt::Display for CreditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroMonths => write!(f, "commitment must cover at least one month"),
            Self::TooManyMonths { max } => {
                write!(
                    f,
                    "commitment exceeds the supported maximum of {max} months"
                )
            }
            Self::UnsupportedBulkTerm { years, max } => {
                write!(f, "bulk term of {years} years is unsupported (max {max})")
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

fn multiplier_bps_for_month(term: Term, month: u16) -> u64 {
    // A month-to-month subscription never earns the founding bonus.
    if term == Term::Monthly {
        return BPS_DENOMINATOR;
    }
    if month <= FOUNDING_MONTHS {
        u64::from(BONUS_MULTIPLIER_BPS[(month - 1) as usize])
    } else {
        BPS_DENOMINATOR
    }
}

/// Computes the credit grants for a commitment of `term` at `monthly_sats` per
/// month. The founding bonus applies once, on the first six service months; the
/// remaining months grant list value. The total is scaled down proportionally
/// if it would exceed the 1.5x ceiling.
pub fn quote_commitment(
    plan_id: &'static str,
    monthly_sats: u64,
    term: Term,
) -> Result<PrepayQuote, CreditError> {
    if let Term::Bulk { years } = term {
        if !(2..=MAX_BULK_YEARS).contains(&years) {
            return Err(CreditError::UnsupportedBulkTerm {
                years,
                max: MAX_BULK_YEARS,
            });
        }
    }

    let service_months = term.service_months();
    let paid_months = term.paid_months();
    if service_months == 0 || paid_months == 0 {
        return Err(CreditError::ZeroMonths);
    }
    if paid_months > service_months {
        return Err(CreditError::TooManyMonths {
            max: service_months,
        });
    }

    let mut grants = Vec::with_capacity(service_months as usize);
    let mut total_credit_sats: u64 = 0;
    for month in 1..=service_months {
        let amount = monthly_sats
            .checked_mul(multiplier_bps_for_month(term, month))
            .ok_or(CreditError::Overflow)?
            / BPS_DENOMINATOR;
        total_credit_sats = total_credit_sats
            .checked_add(amount)
            .ok_or(CreditError::Overflow)?;
        grants.push(CreditGrant {
            month_index: month,
            amount_sats: amount,
        });
    }

    let prepay_sats = monthly_sats
        .checked_mul(u64::from(paid_months))
        .ok_or(CreditError::Overflow)?;

    let cap = prepay_sats
        .checked_mul(MAX_BONUS_RATIO_BPS)
        .ok_or(CreditError::Overflow)?
        / BPS_DENOMINATOR;
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

    let effective_monthly_sats = prepay_sats / u64::from(service_months);
    let bonus_ratio_bps = total_credit_sats
        .checked_mul(BPS_DENOMINATOR)
        .ok_or(CreditError::Overflow)?
        .checked_div(prepay_sats)
        .unwrap_or(BPS_DENOMINATOR);

    Ok(PrepayQuote {
        plan_id,
        monthly_sats,
        service_months,
        paid_months,
        prepay_sats,
        effective_monthly_sats,
        grants,
        total_credit_sats,
        bonus_ratio_bps,
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

    /// Builds a ledger from a quote. `expires_at` is the carry-forward deadline.
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
    fn founding_six_month_grants_decline_to_list() {
        let q = quote_commitment("indie", INDIE_SATS, Term::Founding).unwrap();
        assert_eq!(q.service_months, 6);
        assert_eq!(q.paid_months, 6);
        assert_eq!(q.prepay_sats, INDIE_SATS * 6);
        let amounts: Vec<u64> = q.grants.iter().map(|g| g.amount_sats).collect();
        assert_eq!(
            amounts,
            vec![75_400, 67_860, 60_320, 52_780, 45_240, 37_700]
        );
        assert_eq!(q.total_credit_sats, 339_300);
        assert_eq!(q.bonus_ratio_bps, 15_000, "founding is exactly 1.5x");
        assert_eq!(q.grants.last().unwrap().amount_sats, INDIE_SATS);
    }

    #[test]
    fn annual_two_months_free_is_one_point_five_x() {
        let q = quote_commitment("indie", INDIE_SATS, Term::Annual).unwrap();
        assert_eq!(q.service_months, 12);
        assert_eq!(q.paid_months, 10);
        assert_eq!(q.prepay_sats, INDIE_SATS * 10);
        // Founding six (339_300) + six list months (226_200).
        assert_eq!(q.total_credit_sats, 565_500);
        assert_eq!(
            q.bonus_ratio_bps, 15_000,
            "annual also hits the 1.5x ceiling"
        );
        assert_eq!(q.effective_monthly_sats, INDIE_SATS * 10 / 12);
    }

    #[test]
    fn bulk_terms_deepen_the_discount_and_stay_under_cap() {
        let two_year = quote_commitment("indie", INDIE_SATS, Term::Bulk { years: 2 }).unwrap();
        assert_eq!(two_year.service_months, 24);
        assert_eq!(two_year.paid_months, 18);
        assert!(two_year.bonus_ratio_bps <= MAX_BONUS_RATIO_BPS);

        let three_year = quote_commitment("indie", INDIE_SATS, Term::Bulk { years: 3 }).unwrap();
        assert_eq!(three_year.service_months, 36);
        assert_eq!(three_year.paid_months, 26);
        assert!(three_year.bonus_ratio_bps <= MAX_BONUS_RATIO_BPS);

        // Longer bulk terms are cheaper per month.
        assert!(three_year.effective_monthly_sats < two_year.effective_monthly_sats);
        assert!(two_year.effective_monthly_sats < INDIE_SATS);
        // And grant more absolute credit than annual.
        let annual = quote_commitment("indie", INDIE_SATS, Term::Annual).unwrap();
        assert!(two_year.total_credit_sats > annual.total_credit_sats);
    }

    #[test]
    fn term_discounts_sit_within_industry_benchmarks() {
        // 2026 SaaS norms: annual 15-25% off list, multi-year adds ~5-10% per
        // extra year. Keep every committed term inside a 15-30% band and
        // monotonically deeper with term length.
        let terms = [
            Term::Annual,
            Term::Bulk { years: 2 },
            Term::Bulk { years: 3 },
        ];
        let mut last = 0;
        for term in terms {
            let q = quote_commitment("indie", INDIE_SATS, term).unwrap();
            let discount_bps = u64::from(q.service_months - q.paid_months) * BPS_DENOMINATOR
                / u64::from(q.service_months);
            // 100% == 10_000 bps, so 15% == 1_500 bps and 30% == 3_000 bps.
            assert!(
                (1_500..=3_000).contains(&discount_bps),
                "{term:?} discount {discount_bps}bps outside the 15-30% benchmark band"
            );
            assert!(discount_bps >= last, "{term:?} discount must not decrease");
            last = discount_bps;
        }
    }

    #[test]
    fn every_term_respects_the_one_point_five_x_ceiling() {
        for term in [
            Term::Founding,
            Term::Annual,
            Term::Bulk { years: 2 },
            Term::Bulk { years: 3 },
        ] {
            let q = quote_commitment("business", BUSINESS_SATS, term).unwrap();
            assert!(
                q.bonus_ratio_bps <= MAX_BONUS_RATIO_BPS,
                "{term:?} ratio {} exceeds cap",
                q.bonus_ratio_bps
            );
        }
    }

    #[test]
    fn monthly_is_list_price_with_no_bonus() {
        let q = quote_commitment("indie", INDIE_SATS, Term::Monthly).unwrap();
        assert_eq!(q.service_months, 1);
        assert_eq!(q.prepay_sats, INDIE_SATS);
        assert_eq!(q.total_credit_sats, INDIE_SATS);
        assert_eq!(q.bonus_ratio_bps, 10_000);
    }

    #[test]
    fn unsupported_bulk_terms_are_rejected() {
        assert_eq!(
            quote_commitment("indie", INDIE_SATS, Term::Bulk { years: 1 }).unwrap_err(),
            CreditError::UnsupportedBulkTerm { years: 1, max: 3 }
        );
        assert_eq!(
            quote_commitment("indie", INDIE_SATS, Term::Bulk { years: 4 }).unwrap_err(),
            CreditError::UnsupportedBulkTerm { years: 4, max: 3 }
        );
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
        let q = quote_commitment("indie", INDIE_SATS, Term::Founding).unwrap();
        let l = CreditLedger::from_quote("t1", &q, 99_999);
        assert_eq!(l.balance(), 339_300);
        assert_eq!(l.tenant_id(), "t1");
    }
}
