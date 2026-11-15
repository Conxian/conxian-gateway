//! Protocol-fee accumulation and reporting (Gateway #974).
//!
//! `conxian_engine::billing` defines `ProtocolFeeRecord`, `ProtocolFeeReport`,
//! `accumulate_protocol_fee` and `generate_protocol_fee_report`, but nothing
//! called them: the aggregators were dead code and the market SDK's
//! `submitFeeReport` posted to a route that did not exist. This module wires the
//! two halves of that pipeline:
//!
//! ```text
//! settlement (priced headers)  -> record_from_headers -> accumulate
//! POST /billing/fee-report     -> SubmittedFeeReport  -> accumulate
//! GET  /billing/fee-report     -> generate_protocol_fee_report
//! ```
//!
//! The fee basis is ADR-007 (one 50/30/20 split on the platform take); the rate
//! itself is priced by the caller (the market, ADR-005 dynamic model) and carried
//! on the `x-conxian-fee-*` headers. This module never re-derives the rate and
//! never moves value: it accumulates and aggregates only. Activation stays gated
//! on market #363 (`protocolFeeActive`).

use axum::http::HeaderMap;
use conxian_engine::billing::{
    accumulate_protocol_fee, generate_protocol_fee_report, BillingPeriod, GatewayDeployment,
    ProtocolFeeRecord, ProtocolFeeReport,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex, OnceLock};

/// Fee headers the market emits on a settlement (ADR-007, market #361).
pub const FEE_BPS_HEADER: &str = "x-conxian-fee-bps";
pub const FEE_SAT_HEADER: &str = "x-conxian-fee-sat";
pub const TIER_HEADER: &str = "x-conxian-tier";
pub const RAIL_HEADER: &str = "x-conxian-rail";

fn store() -> &'static Arc<Mutex<Vec<ProtocolFeeRecord>>> {
    static STORE: OnceLock<Arc<Mutex<Vec<ProtocolFeeRecord>>>> = OnceLock::new();
    STORE.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Build a `ProtocolFeeRecord` from the market's settlement headers, or `None`
/// when they are absent. A settlement without priced headers (a self-hosted or
/// unpriced call) is not accumulated — it never invents a fee.
pub fn record_from_headers(
    headers: &HeaderMap,
    settlement_id: &str,
    builder_id: &str,
    amount_sat: u64,
    tx_id: Option<String>,
) -> Option<ProtocolFeeRecord> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
    };
    let fee_bps: u16 = header(FEE_BPS_HEADER)?.parse().ok()?;
    let fee_sat: u64 = header(FEE_SAT_HEADER)?.parse().ok()?;
    Some(ProtocolFeeRecord {
        settlement_id: settlement_id.to_string(),
        rail: header(RAIL_HEADER).unwrap_or("unknown").to_string(),
        tier: header(TIER_HEADER).unwrap_or("unknown").to_string(),
        amount_sat,
        fee_sat,
        fee_bps,
        timestamp: now_unix(),
        builder_id: builder_id.to_string(),
        tx_id,
    })
}

/// Append a priced fee to the running collection. No-op when `record` is `None`.
pub fn accumulate(record: Option<ProtocolFeeRecord>) {
    if let Some(record) = record {
        if let Ok(mut guard) = store().lock() {
            accumulate_protocol_fee(&mut guard, record);
        }
    }
}

/// The current-period report over every accumulated record.
pub fn current_report() -> ProtocolFeeReport {
    let period = BillingPeriod::current(GatewayDeployment::from_env());
    let records = store()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    generate_protocol_fee_report(period, &records)
}

/// The market SDK's `ProtocolFeeReport` (camelCase, bigints as strings) as posted
/// to `POST /billing/fee-report` by `gateway_client.ts::submitFeeReport`.
#[derive(Debug, Deserialize)]
pub struct SubmittedFeeReport {
    #[serde(rename = "periodStart", default)]
    pub period_start: i64,
    /// Required: a report with no rail breakdown is not a fee report. Absent
    /// `byRail` is a 422, not a silent zero-record success (fail closed).
    #[serde(rename = "byRail")]
    pub by_rail: Vec<SubmittedRail>,
    #[serde(rename = "byTier", default)]
    pub by_tier: Vec<SubmittedTier>,
}

#[derive(Debug, Deserialize)]
pub struct SubmittedRail {
    pub rail: String,
    #[serde(rename = "count", default)]
    pub count: u64,
    #[serde(rename = "totalAmountSat", default)]
    pub total_amount_sat: String,
    #[serde(rename = "totalFeeSat", default)]
    pub total_fee_sat: String,
    #[serde(rename = "avgFeeBps", default)]
    pub avg_fee_bps: u16,
}

#[derive(Debug, Deserialize)]
pub struct SubmittedTier {
    pub tier: String,
    #[serde(rename = "count", default)]
    pub count: u64,
}

impl SubmittedFeeReport {
    /// Expand the submitted per-rail aggregates into individual records.
    ///
    /// One record per rail entry: the market's per-settlement report carries a
    /// single rail entry, and a multi-rail report is expanded to one record per
    /// rail so the gateway's own aggregation reproduces the same totals. Fields
    /// the report does not carry (builder, settlement id) are left empty rather
    /// than fabricated; the settlement-header channel carries them losslessly.
    pub fn into_records(self) -> Vec<ProtocolFeeRecord> {
        let timestamp = if self.period_start > 0 {
            self.period_start
        } else {
            now_unix()
        };
        self.by_rail
            .into_iter()
            .filter_map(|rail| {
                let amount_sat: u64 = rail.total_amount_sat.parse().ok()?;
                let fee_sat: u64 = rail.total_fee_sat.parse().ok()?;
                let tier = self
                    .by_tier
                    .first()
                    .map(|t| t.tier.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                Some(ProtocolFeeRecord {
                    settlement_id: format!("report-{timestamp}-{}", rail.rail),
                    rail: rail.rail,
                    tier,
                    amount_sat,
                    fee_sat,
                    fee_bps: rail.avg_fee_bps,
                    timestamp,
                    builder_id: String::new(),
                    tx_id: None,
                })
            })
            .collect()
    }
}

/// Ingest a submitted report: accumulate one record per rail and return the
/// count accepted.
pub fn ingest(report: SubmittedFeeReport) -> usize {
    let records = report.into_records();
    let count = records.len();
    if let Ok(mut guard) = store().lock() {
        for record in records {
            accumulate_protocol_fee(&mut guard, record);
        }
    }
    count
}

/// `POST /billing/fee-report` — accept the market SDK's priced fee report.
///
/// Returns `{ "received": true, "records": <n> }`. The market client treats a
/// non-2xx as a failed report and rejects the settlement (fail-closed), so this
/// returns 200 only when the body parsed.
pub async fn submit_fee_report_handler(
    axum::Json(report): axum::Json<SubmittedFeeReport>,
) -> axum::Json<serde_json::Value> {
    let records = ingest(report);
    axum::Json(serde_json::json!({ "received": true, "records": records }))
}

/// `GET /billing/fee-report` — the gateway's own aggregated report over every
/// accumulated record (from settlement headers and submitted reports).
pub async fn get_fee_report_handler() -> axum::Json<ProtocolFeeReport> {
    axum::Json(current_report())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    #[test]
    fn absent_headers_yield_no_record() {
        assert!(record_from_headers(&HeaderMap::new(), "s1", "b1", 1_000, None).is_none());
    }

    #[test]
    fn partial_headers_yield_no_record() {
        // fee-bps present but fee-sat missing -> not a priced settlement.
        let h = headers(&[(FEE_BPS_HEADER, "50")]);
        assert!(record_from_headers(&h, "s1", "b1", 1_000, None).is_none());
    }

    #[test]
    fn priced_headers_build_the_record() {
        let h = headers(&[
            (FEE_BPS_HEADER, "50"),
            (FEE_SAT_HEADER, "50"),
            (TIER_HEADER, "Expedient"),
            (RAIL_HEADER, "lightning"),
        ]);
        let record = record_from_headers(&h, "s1", "builder-1", 10_000, Some("tx".into())).unwrap();
        assert_eq!(record.rail, "lightning");
        assert_eq!(record.tier, "Expedient");
        assert_eq!(record.amount_sat, 10_000);
        assert_eq!(record.fee_sat, 50);
        assert_eq!(record.fee_bps, 50);
        assert_eq!(record.settlement_id, "s1");
        assert_eq!(record.builder_id, "builder-1");
    }

    #[test]
    fn malformed_fee_bps_is_rejected_not_defaulted() {
        let h = headers(&[(FEE_BPS_HEADER, "not-a-number"), (FEE_SAT_HEADER, "50")]);
        assert!(record_from_headers(&h, "s1", "b1", 1_000, None).is_none());
    }

    #[test]
    fn submitted_report_expands_one_record_per_rail() {
        let report = SubmittedFeeReport {
            period_start: 1_700_000_000,
            by_rail: vec![SubmittedRail {
                rail: "lightning".to_string(),
                count: 1,
                total_amount_sat: "10000".to_string(),
                total_fee_sat: "50".to_string(),
                avg_fee_bps: 50,
            }],
            by_tier: vec![SubmittedTier {
                tier: "Expedient".to_string(),
                count: 1,
            }],
        };
        let records = report.into_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].rail, "lightning");
        assert_eq!(records[0].tier, "Expedient");
        assert_eq!(records[0].amount_sat, 10_000);
        assert_eq!(records[0].fee_sat, 50);
        assert_eq!(records[0].timestamp, 1_700_000_000);
    }

    #[test]
    fn submitted_report_with_unparseable_amounts_is_skipped() {
        let report = SubmittedFeeReport {
            period_start: 0,
            by_rail: vec![SubmittedRail {
                rail: "lightning".to_string(),
                count: 1,
                total_amount_sat: "not-a-number".to_string(),
                total_fee_sat: "50".to_string(),
                avg_fee_bps: 50,
            }],
            by_tier: vec![],
        };
        assert!(report.into_records().is_empty());
    }
}
