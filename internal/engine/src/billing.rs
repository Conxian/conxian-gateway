//! Gateway MRR (Monthly Recurring Revenue) billing module.
//!
//! Tracks gateway service usage and generates per-period billing reports.
//! Designed for both self-hosted operators and managed gateway deployments.
//!
//! ## Architecture
//!
//! - **Usage counters**: relay messages, RWA verifications, settlement ops
//! - **Periodic rollups**: daily aggregation → monthly billing period
//! - **Tiered pricing**: self-hosted (zero-cost) vs managed (per-operation)
//! - **Export**: JSON billing reports for Stripe/accounting integration
//!
//! ## CON-1427 Protocol Fee Extension (Session 48)
//!
//! The protocol fee pipeline (market FeeCalculator → gateway bridge → Clarity contracts)
//! is implemented here via `ProtocolFeeRecord` and `ProtocolFeeReport`.
//! See `conxian_market/docs/research/CON1427_IMPLEMENTATION_PLAN.md` for full spec.
//!
//! Wire format: market sends `x-conxian-fee-bps`, `x-conxian-fee-sat`, `x-conxian-tier`,
//! `x-conxian-rail` headers. Gateway accumulates into `ProtocolFeeRecord` and generates
//! monthly `ProtocolFeeReport` for contract settlement via `protocol-fee-collector.clar`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ---- Pricing constants ----

/// Base monthly cost for the managed Gateway service (USD cents).
pub const MANAGED_GATEWAY_BASE_FEE_CENTS: u64 = 20_000; // $200/mo

/// Per-relay-message cost for managed Gateway (USD cents).
pub const RELAY_MESSAGE_COST_CENTS: u64 = 1; // $0.01 per message

/// Per-RWA-verification cost for managed Gateway (USD cents).
pub const RWA_VERIFICATION_COST_CENTS: u64 = 5; // $0.05 per verification

/// Per-settlement operation cost for managed Gateway (USD cents).
pub const SETTLEMENT_OP_COST_CENTS: u64 = 10; // $0.10 per settlement

/// Tier thresholds: usage above this switches to volume pricing.
pub const ENTERPRISE_RELAY_THRESHOLD: u64 = 100_000; // messages/month
pub const ENTERPRISE_DISCOUNT_BPS: u64 = 2000; // 20% discount in basis points

/// Billing period: 30 days.
pub const BILLING_PERIOD_SECONDS: u64 = 30 * 24 * 3600;

// ---- Models ----

/// Gateway usage metrics for a billing period.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageMetrics {
    /// Number of relay messages processed.
    pub relay_messages: u64,
    /// Number of RWA verifications completed.
    pub rwa_verifications: u64,
    /// Number of settlement operations (DLC, Lightning, cross-chain).
    pub settlement_ops: u64,
    /// Number of Bitcoin blocks observed (shadow + listener).
    pub bitcoin_blocks_observed: u64,
    /// Number of Stacks blocks observed.
    pub stacks_blocks_observed: u64,
    /// Total satoshis relayed across NTT bridges.
    pub ntt_volume_sats: u64,
}

impl UsageMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn accumulate(&mut self, other: &Self) {
        self.relay_messages += other.relay_messages;
        self.rwa_verifications += other.rwa_verifications;
        self.settlement_ops += other.settlement_ops;
        self.bitcoin_blocks_observed += other.bitcoin_blocks_observed;
        self.stacks_blocks_observed += other.stacks_blocks_observed;
        self.ntt_volume_sats += other.ntt_volume_sats;
    }
}

/// Gateway deployment model — determines pricing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayDeployment {
    /// Operator runs their own Gateway; no fees.
    SelfHosted,
    /// Conxian-managed Gateway; per-operation billing.
    Managed,
}

impl GatewayDeployment {
    pub fn from_env() -> Self {
        match std::env::var("GATEWAY_DEPLOYMENT_MODEL")
            .unwrap_or_else(|_| "self_hosted".into())
            .to_lowercase()
            .as_str()
        {
            "managed" => Self::Managed,
            _ => Self::SelfHosted,
        }
    }
}

/// A billing period for MRR calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingPeriod {
    /// Unix timestamp of period start.
    pub start_unix: u64,
    /// Unix timestamp of period end.
    pub end_unix: u64,
    /// Deployment model for this period.
    pub deployment: GatewayDeployment,
}

impl BillingPeriod {
    pub fn current(deployment: GatewayDeployment) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let start = now - (now % BILLING_PERIOD_SECONDS);
        Self {
            start_unix: start,
            end_unix: start + BILLING_PERIOD_SECONDS,
            deployment,
        }
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs(self.end_unix - self.start_unix)
    }
}

/// A billing report generated at the end of a billing period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrrReport {
    pub period: BillingPeriod,
    pub usage: UsageMetrics,
    /// Total cost in USD cents.
    pub total_cost_cents: u64,
    /// Line items breakdown.
    pub line_items: Vec<LineItem>,
    /// Whether enterprise discount applied.
    pub enterprise_discount_applied: bool,
}

/// Individual line item in a billing report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItem {
    pub description: String,
    pub quantity: u64,
    pub unit_cost_cents: u64,
    pub subtotal_cents: u64,
}

// ---- Billing Calculator ----

/// Computes a billing report from accumulated usage.
pub fn compute_mrr(period: BillingPeriod, usage: &UsageMetrics) -> MrrReport {
    let mut line_items = Vec::new();
    let mut total = 0u64;

    if period.deployment == GatewayDeployment::SelfHosted {
        return MrrReport {
            period,
            usage: usage.clone(),
            total_cost_cents: 0,
            line_items,
            enterprise_discount_applied: false,
        };
    }

    // Base fee
    line_items.push(LineItem {
        description: "Managed Gateway base fee".into(),
        quantity: 1,
        unit_cost_cents: MANAGED_GATEWAY_BASE_FEE_CENTS,
        subtotal_cents: MANAGED_GATEWAY_BASE_FEE_CENTS,
    });
    total += MANAGED_GATEWAY_BASE_FEE_CENTS;

    // Relay messages
    if usage.relay_messages > 0 {
        let subtotal = usage.relay_messages * RELAY_MESSAGE_COST_CENTS;
        line_items.push(LineItem {
            description: "Relay messages".into(),
            quantity: usage.relay_messages,
            unit_cost_cents: RELAY_MESSAGE_COST_CENTS,
            subtotal_cents: subtotal,
        });
        total += subtotal;
    }

    // RWA verifications
    if usage.rwa_verifications > 0 {
        let subtotal = usage.rwa_verifications * RWA_VERIFICATION_COST_CENTS;
        line_items.push(LineItem {
            description: "RWA verifications".into(),
            quantity: usage.rwa_verifications,
            unit_cost_cents: RWA_VERIFICATION_COST_CENTS,
            subtotal_cents: subtotal,
        });
        total += subtotal;
    }

    // Settlement operations
    if usage.settlement_ops > 0 {
        let subtotal = usage.settlement_ops * SETTLEMENT_OP_COST_CENTS;
        line_items.push(LineItem {
            description: "Settlement operations".into(),
            quantity: usage.settlement_ops,
            unit_cost_cents: SETTLEMENT_OP_COST_CENTS,
            subtotal_cents: subtotal,
        });
        total += subtotal;
    }

    // Enterprise volume discount
    let enterprise_discount = usage.relay_messages >= ENTERPRISE_RELAY_THRESHOLD;
    let discount_cents = if enterprise_discount {
        (total * ENTERPRISE_DISCOUNT_BPS) / 10000
    } else {
        0
    };
    total -= discount_cents;

    MrrReport {
        period,
        usage: usage.clone(),
        total_cost_cents: total,
        line_items,
        enterprise_discount_applied: enterprise_discount,
    }
}

/// Converts USD cents to a human-readable dollar string.
pub fn format_usd(cents: u64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_hosted_is_zero_cost() {
        let usage = UsageMetrics {
            relay_messages: 1_000_000,
            rwa_verifications: 10_000,
            settlement_ops: 5_000,
            ..Default::default()
        };
        let period = BillingPeriod {
            start_unix: 0,
            end_unix: BILLING_PERIOD_SECONDS,
            deployment: GatewayDeployment::SelfHosted,
        };
        let report = compute_mrr(period, &usage);
        assert_eq!(report.total_cost_cents, 0);
        assert!(report.line_items.is_empty());
    }

    #[test]
    fn managed_base_fee() {
        let usage = UsageMetrics::new();
        let period = BillingPeriod {
            start_unix: 0,
            end_unix: BILLING_PERIOD_SECONDS,
            deployment: GatewayDeployment::Managed,
        };
        let report = compute_mrr(period, &usage);
        assert_eq!(report.total_cost_cents, MANAGED_GATEWAY_BASE_FEE_CENTS);
        assert_eq!(report.line_items.len(), 1);
    }

    #[test]
    fn managed_per_operation_costs() {
        let usage = UsageMetrics {
            relay_messages: 1_000,
            rwa_verifications: 100,
            settlement_ops: 50,
            ..Default::default()
        };
        let period = BillingPeriod {
            start_unix: 0,
            end_unix: BILLING_PERIOD_SECONDS,
            deployment: GatewayDeployment::Managed,
        };
        let report = compute_mrr(period, &usage);
        let expected = MANAGED_GATEWAY_BASE_FEE_CENTS
            + 1_000 * RELAY_MESSAGE_COST_CENTS
            + 100 * RWA_VERIFICATION_COST_CENTS
            + 50 * SETTLEMENT_OP_COST_CENTS;
        assert_eq!(report.total_cost_cents, expected);
    }

    #[test]
    fn enterprise_volume_discount() {
        let usage = UsageMetrics {
            relay_messages: ENTERPRISE_RELAY_THRESHOLD,
            ..Default::default()
        };
        let period = BillingPeriod {
            start_unix: 0,
            end_unix: BILLING_PERIOD_SECONDS,
            deployment: GatewayDeployment::Managed,
        };
        let report = compute_mrr(period, &usage);
        assert!(report.enterprise_discount_applied);
        // Base fee + 100k relay messages, minus 20% discount
        let undiscounted =
            MANAGED_GATEWAY_BASE_FEE_CENTS + ENTERPRISE_RELAY_THRESHOLD * RELAY_MESSAGE_COST_CENTS;
        let expected = undiscounted - (undiscounted * ENTERPRISE_DISCOUNT_BPS) / 10000;
        assert_eq!(report.total_cost_cents, expected);
    }

    #[test]
    fn format_usd_output() {
        assert_eq!(format_usd(0), "$0.00");
        assert_eq!(format_usd(1), "$0.01");
        assert_eq!(format_usd(100), "$1.00");
        assert_eq!(format_usd(20000), "$200.00");
        assert_eq!(format_usd(123456789), "$1234567.89");
    }

    #[test]
    fn usage_accumulation() {
        let mut base = UsageMetrics::new();
        let add = UsageMetrics {
            relay_messages: 10,
            rwa_verifications: 5,
            settlement_ops: 2,
            bitcoin_blocks_observed: 100,
            stacks_blocks_observed: 50,
            ntt_volume_sats: 1_000_000,
        };
        base.accumulate(&add);
        base.accumulate(&add);
        assert_eq!(base.relay_messages, 20);
        assert_eq!(base.rwa_verifications, 10);
        assert_eq!(base.settlement_ops, 4);
        assert_eq!(base.bitcoin_blocks_observed, 200);
        assert_eq!(base.stacks_blocks_observed, 100);
        assert_eq!(base.ntt_volume_sats, 2_000_000);
    }

    #[test]
    fn billing_period_current() {
        let period = BillingPeriod::current(GatewayDeployment::Managed);
        assert_eq!(
            period.duration(),
            Duration::from_secs(BILLING_PERIOD_SECONDS)
        );
        assert!(period.start_unix > 0);
        assert!(period.end_unix > period.start_unix);
    }

    #[test]
    fn gateway_deployment_from_env() {
        // Default is self-hosted when env var not set
        std::env::remove_var("GATEWAY_DEPLOYMENT_MODEL");
        assert_eq!(GatewayDeployment::from_env(), GatewayDeployment::SelfHosted);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// CON-1427: Protocol Fee Collection Bridge
// ══════════════════════════════════════════════════════════════════════════════

/// A single protocol fee event emitted by the market FeeCalculator and recorded
/// by the gateway for periodic settlement via Clarity contracts.
///
/// Wire format: market sends x-conxian-fee-bps, x-conxian-fee-sat, x-conxian-tier,
/// x-conxian-rail headers. Gateway deserializes to this struct.
///
/// @gap CON-1427 [#488](https://github.com/Conxian/Conxian/issues/488)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolFeeRecord {
    /// Settlement event ID (correlates with JobCard/WorkIntent).
    pub settlement_id: String,
    /// Settlement rail used (sbtc, lightning, fedimint, statechain, etc.).
    pub rail: String,
    /// TrustTier at time of settlement.
    pub tier: String,
    /// Settlement amount in satoshis.
    pub amount_sat: u64,
    /// Protocol fee in satoshis (amount * fee_bps / 10000).
    pub fee_sat: u64,
    /// Fee rate in basis points (200 = 2%, 100 = 1%, 250 = 2.5%).
    pub fee_bps: u16,
    /// Unix timestamp of settlement.
    pub timestamp: i64,
    /// Builder receiving the settlement.
    pub builder_id: String,
    /// On-chain transaction ID (if available at record time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_id: Option<String>,
}

/// Aggregated protocol fee report for a billing period.
/// Generated by accumulate_protocol_fee and consumed by contract-bridge
/// for Clarity contract settlement via protocol-fee-collector.clar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolFeeReport {
    pub period: BillingPeriod,
    /// Total satoshis settled across all rails.
    pub total_settled_sat: u64,
    /// Total protocol fees collected (sum of all fee_sat).
    pub total_fees_sat: u64,
    /// Effective fee rate (total_fees / total_settled * 10000).
    pub effective_fee_bps: u16,
    /// Breakdown by settlement rail.
    pub by_rail: Vec<RailFeeBreakdown>,
    /// Breakdown by TrustTier.
    pub by_tier: Vec<TierFeeBreakdown>,
    /// Number of settlement events in this period.
    pub event_count: u64,
    /// All individual records (for audit).
    pub records: Vec<ProtocolFeeRecord>,
}

/// Per-rail aggregation in a fee report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailFeeBreakdown {
    pub rail: String,
    pub count: u64,
    pub total_amount_sat: u64,
    pub total_fee_sat: u64,
    pub avg_fee_bps: u16,
}

/// Per-tier aggregation in a fee report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierFeeBreakdown {
    pub tier: String,
    pub count: u64,
    pub total_fee_sat: u64,
}

/// Accumulate a single protocol fee record into the running collection.
/// Called per settlement event. Thread-safe when records are behind a Mutex.
pub fn accumulate_protocol_fee(records: &mut Vec<ProtocolFeeRecord>, record: ProtocolFeeRecord) {
    records.push(record);
}

/// Generate a protocol fee report for a billing period from accumulated records.
/// Filters records to the period window and aggregates by rail and tier.
pub fn generate_protocol_fee_report(
    period: BillingPeriod,
    records: &[ProtocolFeeRecord],
) -> ProtocolFeeReport {
    let mut total_settled_sat: u64 = 0;
    let mut total_fees_sat: u64 = 0;
    let mut by_rail: HashMap<String, RailFeeBreakdown> = HashMap::new();
    let mut by_tier: HashMap<String, TierFeeBreakdown> = HashMap::new();
    let mut event_count: u64 = 0;

    for r in records {
        if r.timestamp < period.start_unix as i64 || r.timestamp >= period.end_unix as i64 {
            continue;
        }
        event_count += 1;
        total_settled_sat = total_settled_sat.saturating_add(r.amount_sat);
        total_fees_sat = total_fees_sat.saturating_add(r.fee_sat);

        by_rail
            .entry(r.rail.clone())
            .and_modify(|b| {
                b.count += 1;
                b.total_amount_sat = b.total_amount_sat.saturating_add(r.amount_sat);
                b.total_fee_sat = b.total_fee_sat.saturating_add(r.fee_sat);
                b.avg_fee_bps = if b.total_amount_sat > 0 {
                    ((b.total_fee_sat as u128 * 10000) / b.total_amount_sat as u128) as u16
                } else {
                    0
                };
            })
            .or_insert(RailFeeBreakdown {
                rail: r.rail.clone(),
                count: 1,
                total_amount_sat: r.amount_sat,
                total_fee_sat: r.fee_sat,
                avg_fee_bps: r.fee_bps,
            });

        by_tier
            .entry(r.tier.clone())
            .and_modify(|b| {
                b.count += 1;
                b.total_fee_sat = b.total_fee_sat.saturating_add(r.fee_sat);
            })
            .or_insert(TierFeeBreakdown {
                tier: r.tier.clone(),
                count: 1,
                total_fee_sat: r.fee_sat,
            });
    }

    let effective_fee_bps = if total_settled_sat > 0 {
        ((total_fees_sat as u128 * 10000) / total_settled_sat as u128) as u16
    } else {
        0
    };

    ProtocolFeeReport {
        period,
        total_settled_sat,
        total_fees_sat,
        effective_fee_bps,
        by_rail: by_rail.into_values().collect(),
        by_tier: by_tier.into_values().collect(),
        event_count,
        records: records.to_vec(),
    }
}

#[cfg(test)]
mod protocol_fee_tests {
    use super::*;

    fn test_period() -> BillingPeriod {
        BillingPeriod {
            start_unix: 0u64,
            end_unix: BILLING_PERIOD_SECONDS as u64,
            deployment: GatewayDeployment::Managed,
        }
    }

    fn sample_record(id: &str, rail: &str, tier: &str, amount_sat: u64, fee_bps: u16) -> ProtocolFeeRecord {
        let fee_sat = (amount_sat as u128 * fee_bps as u128 / 10000) as u64;
        ProtocolFeeRecord {
            settlement_id: id.into(),
            rail: rail.into(),
            tier: tier.into(),
            amount_sat,
            fee_sat,
            fee_bps,
            timestamp: 1000,
            builder_id: "test-builder".into(),
            tx_id: None,
        }
    }

    #[test]
    fn accumulate_and_report_single_record() {
        let mut records = Vec::new();
        accumulate_protocol_fee(&mut records, sample_record("s1", "sbtc", "EXPEDIENT", 100_000, 200));

        let report = generate_protocol_fee_report(test_period(), &records);
        assert_eq!(report.event_count, 1);
        assert_eq!(report.total_settled_sat, 100_000);
        assert_eq!(report.total_fees_sat, 2_000); // 2% of 100K
        assert_eq!(report.effective_fee_bps, 200);
        assert_eq!(report.by_rail.len(), 1);
        assert_eq!(report.by_tier.len(), 1);
    }

    #[test]
    fn accumulate_multiple_rails() {
        let mut records = Vec::new();
        accumulate_protocol_fee(&mut records, sample_record("s1", "sbtc", "EXPEDIENT", 100_000, 200));
        accumulate_protocol_fee(&mut records, sample_record("s2", "lightning", "EXPEDIENT", 50_000, 100));
        accumulate_protocol_fee(&mut records, sample_record("s3", "fedimint", "EXPEDIENT", 25_000, 100));

        let report = generate_protocol_fee_report(test_period(), &records);
        assert_eq!(report.event_count, 3);
        assert_eq!(report.total_settled_sat, 175_000);
        assert_eq!(report.total_fees_sat, 2_000 + 500 + 250); // 2000 + 500 + 250 = 2750
        assert_eq!(report.by_rail.len(), 3);
    }

    #[test]
    fn records_outside_period_are_excluded() {
        let mut records = Vec::new();
        accumulate_protocol_fee(&mut records, sample_record("s1", "sbtc", "EXPEDIENT", 100_000, 200));

        let mut future = sample_record("s2", "lightning", "EXPEDIENT", 50_000, 100);
        future.timestamp = 99_999_999; // far future, outside period
        accumulate_protocol_fee(&mut records, future);

        let report = generate_protocol_fee_report(test_period(), &records);
        assert_eq!(report.event_count, 1); // only s1 in period
        assert_eq!(report.total_fees_sat, 2_000);
    }

    #[test]
    fn tier_aggregation() {
        let mut records = Vec::new();
        accumulate_protocol_fee(&mut records, sample_record("s1", "sbtc", "EXPEDIENT", 100_000, 200));
        accumulate_protocol_fee(&mut records, sample_record("s2", "sbtc", "MANAGED", 100_000, 250));

        let report = generate_protocol_fee_report(test_period(), &records);
        assert_eq!(report.by_tier.len(), 2);
        let exp = report.by_tier.iter().find(|t| t.tier == "EXPEDIENT").unwrap();
        let man = report.by_tier.iter().find(|t| t.tier == "MANAGED").unwrap();
        assert_eq!(exp.total_fee_sat, 2_000);
        assert_eq!(man.total_fee_sat, 2_500);
    }

    #[test]
    fn empty_records_produce_zero_report() {
        let records: Vec<ProtocolFeeRecord> = Vec::new();
        let report = generate_protocol_fee_report(test_period(), &records);
        assert_eq!(report.event_count, 0);
        assert_eq!(report.total_settled_sat, 0);
        assert_eq!(report.total_fees_sat, 0);
        assert_eq!(report.effective_fee_bps, 0);
    }
}
