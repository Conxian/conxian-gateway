use conxian_core::{FeeBumpStrategy, MempoolTxStatus, TrackedMempoolTx};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

pub const MEMPOOL_TELEMETRY_SCHEMA_VERSION: u8 = 2;
pub const MEMPOOL_TELEMETRY_SCOPE: &str = "gateway_tracked_transactions";
pub const NETWORK_MEMPOOL_OBSERVATION: &str = "not_configured";
const EMPTY_SEMANTICS: &str =
    "empty means no Gateway-tracked transactions are persisted; it does not mean the network mempool is empty";
const ATTEMPT_SEMANTICS: &str =
    "sum of the current persisted bump_attempts fields; historical per-attempt strategy totals are not represented";
const STRATEGY_SEMANTICS: &str =
    "counts of the current persisted last_bump_strategy fields; each record contributes at most one observation";
const LAST_UPDATED_SEMANTICS: &str =
    "latest persisted last_evaluated_at or last_bump_at across tracked records; response time is not used";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MempoolTelemetryAvailability {
    Available,
    Empty,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MempoolStatusCounts {
    pub pending: u64,
    pub stuck: u64,
    #[serde(rename = "bump_broadcasted")]
    pub bump_broadcasted: u64,
    pub bump_outcome_unknown: u64,
    #[serde(rename = "guardrail_rejected")]
    pub guardrail_rejected: u64,
    pub confirmed: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MempoolStrategyCounts {
    pub rbf: u64,
    pub cpfp: u64,
}

/// A bounded view of the Gateway's persisted transaction tracking state.
///
/// This is deliberately not a node or network mempool observation. The
/// strategy counts describe only the current `last_bump_strategy` field on
/// each record, and the attempt total describes only the current persisted
/// `bump_attempts` fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MempoolTelemetryResponse {
    pub schema_version: u8,
    pub scope: String,
    pub network_mempool_observation: String,
    pub availability: MempoolTelemetryAvailability,
    pub empty_semantics: String,
    pub tracked_transaction_count: u64,
    pub status_counts: MempoolStatusCounts,
    pub replaceable_tracked_total: u64,
    pub cpfp_capable_tracked_total: u64,
    pub bump_attempts_current_total: u64,
    pub records_with_bump_attempts: u64,
    pub last_bump_strategy_counts: MempoolStrategyCounts,
    pub attempt_semantics: String,
    pub strategy_semantics: String,
    pub last_updated_at: Option<u64>,
    pub last_updated_semantics: String,
}

pub fn aggregate_tracked_mempool_transactions(
    tracked_transactions: &[TrackedMempoolTx],
) -> MempoolTelemetryResponse {
    let mut status_counts = MempoolStatusCounts::default();
    let mut last_bump_strategy_counts = MempoolStrategyCounts::default();
    let mut replaceable_tracked_total = 0;
    let mut cpfp_capable_tracked_total = 0;
    let mut bump_attempts_current_total = 0;
    let mut records_with_bump_attempts = 0;
    let mut last_updated_at: Option<u64> = None;

    for tracked in tracked_transactions {
        match tracked.status {
            MempoolTxStatus::Pending => status_counts.pending += 1,
            MempoolTxStatus::Stuck => status_counts.stuck += 1,
            MempoolTxStatus::BumpBroadcasted => status_counts.bump_broadcasted += 1,
            MempoolTxStatus::BumpOutcomeUnknown => status_counts.bump_outcome_unknown += 1,
            MempoolTxStatus::GuardrailRejected => status_counts.guardrail_rejected += 1,
            MempoolTxStatus::Confirmed => status_counts.confirmed += 1,
        }

        if tracked.replaceable {
            replaceable_tracked_total += 1;
        }
        if tracked.cpfp_eligible {
            cpfp_capable_tracked_total += 1;
        }

        bump_attempts_current_total += u64::from(tracked.bump_attempts);
        if tracked.bump_attempts > 0 {
            records_with_bump_attempts += 1;
        }

        match tracked.last_bump_strategy {
            Some(FeeBumpStrategy::Rbf) => last_bump_strategy_counts.rbf += 1,
            Some(FeeBumpStrategy::Cpfp) => last_bump_strategy_counts.cpfp += 1,
            None => {}
        }

        for timestamp in [tracked.last_evaluated_at, tracked.last_bump_at]
            .into_iter()
            .flatten()
        {
            last_updated_at =
                Some(last_updated_at.map_or(timestamp, |current| current.max(timestamp)));
        }
    }

    MempoolTelemetryResponse {
        schema_version: MEMPOOL_TELEMETRY_SCHEMA_VERSION,
        scope: MEMPOOL_TELEMETRY_SCOPE.to_string(),
        network_mempool_observation: NETWORK_MEMPOOL_OBSERVATION.to_string(),
        availability: if tracked_transactions.is_empty() {
            MempoolTelemetryAvailability::Empty
        } else {
            MempoolTelemetryAvailability::Available
        },
        empty_semantics: EMPTY_SEMANTICS.to_string(),
        tracked_transaction_count: tracked_transactions.len() as u64,
        status_counts,
        replaceable_tracked_total,
        cpfp_capable_tracked_total,
        bump_attempts_current_total,
        records_with_bump_attempts,
        last_bump_strategy_counts,
        attempt_semantics: ATTEMPT_SEMANTICS.to_string(),
        strategy_semantics: STRATEGY_SEMANTICS.to_string(),
        last_updated_at,
        last_updated_semantics: LAST_UPDATED_SEMANTICS.to_string(),
    }
}

/// Render bounded Prometheus metrics for the tracked-state view.
///
/// Status and strategy labels are closed enums. No transaction identifiers,
/// addresses, node identifiers, route identifiers, or free-form errors are
/// included. When `telemetry` is `None`, aggregate samples are omitted rather
/// than rendered as misleading network-wide zeros.
pub fn render_prometheus_metrics(
    telemetry: Option<&MempoolTelemetryResponse>,
    state_available: bool,
) -> String {
    let mut body = String::new();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_state_available Whether persisted Gateway-tracked mempool state was available; this is not network-wide mempool availability."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_state_available gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_state_available {}",
        u8::from(state_available)
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_scope_info Scope marker for Gateway-tracked transactions only; this is not a network-wide mempool view."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_scope_info gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_scope_info{{scope=\"gateway_tracked_transactions\",network_mempool_observation=\"not_configured\"}} 1"
    )
    .unwrap();

    let Some(telemetry) = telemetry else {
        return body;
    };

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_empty Whether the loaded Gateway-tracked state is empty; 1 does not mean the network mempool is empty."
    )
    .unwrap();
    writeln!(body, "# TYPE conxian_gateway_tracked_mempool_empty gauge").unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_empty {}",
        u8::from(matches!(
            telemetry.availability,
            MempoolTelemetryAvailability::Empty
        ))
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_transactions Current number of Gateway-tracked transactions; not network-wide."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_transactions gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_transactions {}",
        telemetry.tracked_transaction_count
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_transactions_status Current Gateway-tracked transaction count by closed MempoolTxStatus enum; not network-wide."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_transactions_status gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_transactions_status{{status=\"PENDING\"}} {}",
        telemetry.status_counts.pending
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_transactions_status{{status=\"STUCK\"}} {}",
        telemetry.status_counts.stuck
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_transactions_status{{status=\"BUMP_BROADCASTED\"}} {}",
        telemetry.status_counts.bump_broadcasted
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_transactions_status{{status=\"BUMP_OUTCOME_UNKNOWN\"}} {}",
        telemetry.status_counts.bump_outcome_unknown
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_transactions_status{{status=\"GUARDRAIL_REJECTED\"}} {}",
        telemetry.status_counts.guardrail_rejected
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_transactions_status{{status=\"CONFIRMED\"}} {}",
        telemetry.status_counts.confirmed
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_replaceable_transactions Current replaceable tracked transaction count; not network-wide."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_replaceable_transactions gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_replaceable_transactions {}",
        telemetry.replaceable_tracked_total
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_cpfp_capable_transactions Current CPFP-capable tracked transaction count; not network-wide."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_cpfp_capable_transactions gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_cpfp_capable_transactions {}",
        telemetry.cpfp_capable_tracked_total
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_bump_attempts_current Current sum of persisted bump_attempts fields; not historical attempt volume."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_bump_attempts_current gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_bump_attempts_current {}",
        telemetry.bump_attempts_current_total
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_records_with_bump_attempts Current tracked record count with bump_attempts greater than zero."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_records_with_bump_attempts gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_records_with_bump_attempts {}",
        telemetry.records_with_bump_attempts
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_last_bump_strategy_records Current records grouped by their persisted last_bump_strategy; not historical per-attempt totals."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_last_bump_strategy_records gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_last_bump_strategy_records{{strategy=\"RBF\"}} {}",
        telemetry.last_bump_strategy_counts.rbf
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_last_bump_strategy_records{{strategy=\"CPFP\"}} {}",
        telemetry.last_bump_strategy_counts.cpfp
    )
    .unwrap();

    writeln!(
        body,
        "# HELP conxian_gateway_tracked_mempool_last_updated_at_seconds Latest persisted tracked-state timestamp, or 0 when no tracked timestamp is present; not scrape time."
    )
    .unwrap();
    writeln!(
        body,
        "# TYPE conxian_gateway_tracked_mempool_last_updated_at_seconds gauge"
    )
    .unwrap();
    writeln!(
        body,
        "conxian_gateway_tracked_mempool_last_updated_at_seconds {}",
        telemetry.last_updated_at.unwrap_or_default()
    )
    .unwrap();

    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tracked(status: MempoolTxStatus) -> TrackedMempoolTx {
        TrackedMempoolTx {
            txid: "txid-test-only".to_string(),
            first_seen_at: 10,
            last_evaluated_at: None,
            last_bump_at: None,
            bump_attempts: 0,
            current_fee_rate_sat_vb: 10,
            target_fee_rate_sat_vb: Some(12),
            replaceable: false,
            cpfp_eligible: false,
            status,
            last_bump_strategy: None,
            last_error: None,
            replacement_txid: None,
            lease_owner: None,
            lease_id: None,
            lease_expires_at: None,
            record_generation: 0,
        }
    }

    #[test]
    fn aggregates_every_status_and_capability_total() {
        let mut pending = tracked(MempoolTxStatus::Pending);
        pending.replaceable = true;
        let stuck = tracked(MempoolTxStatus::Stuck);
        let mut broadcasted = tracked(MempoolTxStatus::BumpBroadcasted);
        broadcasted.cpfp_eligible = true;
        let unknown = tracked(MempoolTxStatus::BumpOutcomeUnknown);
        let rejected = tracked(MempoolTxStatus::GuardrailRejected);
        let confirmed = tracked(MempoolTxStatus::Confirmed);

        let telemetry = aggregate_tracked_mempool_transactions(&[
            pending,
            stuck,
            broadcasted,
            unknown,
            rejected,
            confirmed,
        ]);

        assert_eq!(
            telemetry.availability,
            MempoolTelemetryAvailability::Available
        );
        assert_eq!(telemetry.tracked_transaction_count, 6);
        assert_eq!(telemetry.status_counts.pending, 1);
        assert_eq!(telemetry.status_counts.stuck, 1);
        assert_eq!(telemetry.status_counts.bump_broadcasted, 1);
        assert_eq!(telemetry.status_counts.bump_outcome_unknown, 1);
        assert_eq!(telemetry.status_counts.guardrail_rejected, 1);
        assert_eq!(telemetry.status_counts.confirmed, 1);
        assert_eq!(telemetry.replaceable_tracked_total, 1);
        assert_eq!(telemetry.cpfp_capable_tracked_total, 1);
    }

    #[test]
    fn empty_state_is_explicit_and_not_network_zero() {
        let telemetry = aggregate_tracked_mempool_transactions(&[]);

        assert_eq!(telemetry.availability, MempoolTelemetryAvailability::Empty);
        assert_eq!(telemetry.tracked_transaction_count, 0);
        assert_eq!(telemetry.network_mempool_observation, "not_configured");
        assert!(telemetry.empty_semantics.contains("does not mean"));
        assert_eq!(telemetry.last_updated_at, None);
    }

    #[test]
    fn attempt_and_strategy_aggregates_are_honest_about_persisted_precision() {
        let mut rbf = tracked(MempoolTxStatus::Stuck);
        rbf.bump_attempts = 3;
        rbf.last_bump_strategy = Some(FeeBumpStrategy::Rbf);
        let mut cpfp = tracked(MempoolTxStatus::BumpBroadcasted);
        cpfp.bump_attempts = 2;
        cpfp.last_bump_strategy = Some(FeeBumpStrategy::Cpfp);
        let no_attempt = tracked(MempoolTxStatus::Pending);

        let telemetry = aggregate_tracked_mempool_transactions(&[rbf, cpfp, no_attempt]);

        assert_eq!(telemetry.bump_attempts_current_total, 5);
        assert_eq!(telemetry.records_with_bump_attempts, 2);
        assert_eq!(telemetry.last_bump_strategy_counts.rbf, 1);
        assert_eq!(telemetry.last_bump_strategy_counts.cpfp, 1);
        assert!(telemetry.attempt_semantics.contains("current persisted"));
        assert!(telemetry.attempt_semantics.contains("not represented"));
        assert!(telemetry.strategy_semantics.contains("last_bump_strategy"));
    }

    #[test]
    fn last_updated_uses_tracked_timestamps_not_first_seen_or_response_time() {
        let mut first = tracked(MempoolTxStatus::Pending);
        first.first_seen_at = 9_999;
        first.last_evaluated_at = Some(100);
        let mut second = tracked(MempoolTxStatus::Stuck);
        second.first_seen_at = 20_000;
        second.last_bump_at = Some(250);

        let telemetry = aggregate_tracked_mempool_transactions(&[first, second]);

        assert_eq!(telemetry.last_updated_at, Some(250));
        assert!(telemetry.last_updated_semantics.contains("response time"));
    }

    #[test]
    fn aggregation_does_not_mutate_input() {
        let mut original = tracked(MempoolTxStatus::Pending);
        original.bump_attempts = 4;
        original.last_bump_strategy = Some(FeeBumpStrategy::Rbf);
        let before = serde_json::to_value(&original).unwrap();

        let _ = aggregate_tracked_mempool_transactions(std::slice::from_ref(&original));

        assert_eq!(serde_json::to_value(&original).unwrap(), before);
    }

    #[test]
    fn serde_response_is_deterministic_and_versioned() {
        let telemetry =
            aggregate_tracked_mempool_transactions(&[tracked(MempoolTxStatus::Pending)]);
        let first = serde_json::to_string(&telemetry).unwrap();
        let second = serde_json::to_string(&telemetry).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&first).unwrap(),
            json!({
                "schema_version": 2,
                "scope": "gateway_tracked_transactions",
                "network_mempool_observation": "not_configured",
                "availability": "available",
                "empty_semantics": "empty means no Gateway-tracked transactions are persisted; it does not mean the network mempool is empty",
                "tracked_transaction_count": 1,
                "status_counts": {
                    "pending": 1,
                    "stuck": 0,
                    "bump_broadcasted": 0,
                    "bump_outcome_unknown": 0,
                    "guardrail_rejected": 0,
                    "confirmed": 0
                },
                "replaceable_tracked_total": 0,
                "cpfp_capable_tracked_total": 0,
                "bump_attempts_current_total": 0,
                "records_with_bump_attempts": 0,
                "last_bump_strategy_counts": {"rbf": 0, "cpfp": 0},
                "attempt_semantics": "sum of the current persisted bump_attempts fields; historical per-attempt strategy totals are not represented",
                "strategy_semantics": "counts of the current persisted last_bump_strategy fields; each record contributes at most one observation",
                "last_updated_at": null,
                "last_updated_semantics": "latest persisted last_evaluated_at or last_bump_at across tracked records; response time is not used"
            })
        );
    }

    #[test]
    fn prometheus_metrics_are_bounded_and_scope_marked() {
        let mut tx = tracked(MempoolTxStatus::Pending);
        tx.txid = "sensitive-txid".to_string();
        tx.last_error = Some("free-form error".to_string());
        tx.last_bump_strategy = Some(FeeBumpStrategy::Rbf);
        let telemetry = aggregate_tracked_mempool_transactions(&[tx]);

        let rendered = render_prometheus_metrics(Some(&telemetry), true);

        assert!(rendered.contains("scope=\"gateway_tracked_transactions\""));
        assert!(rendered.contains("network_mempool_observation=\"not_configured\""));
        assert!(rendered.contains("status=\"PENDING\""));
        assert!(rendered.contains("strategy=\"RBF\""));
        assert!(!rendered.contains("sensitive-txid"));
        assert!(!rendered.contains("free-form error"));
        assert!(!rendered.contains("address="));
        assert!(!rendered.contains("node_id="));
    }

    #[test]
    fn unavailable_prometheus_state_does_not_emit_misleading_aggregate_zeros() {
        let rendered = render_prometheus_metrics(None, false);

        assert!(rendered.contains("conxian_gateway_tracked_mempool_state_available 0"));
        assert!(rendered.contains("conxian_gateway_tracked_mempool_scope_info"));
        assert!(!rendered.contains("conxian_gateway_tracked_mempool_transactions 0"));
    }
}
