//! SBTCBridge: sBTC deposit/withdrawal lifecycle monitoring via Emily API.
//!
//! Tracks the full sBTC peg operation lifecycle:
//! - Deposits: BTC → signer validation → Stacks mint (PENDING→ACCEPTED→CONFIRMED)
//! - Withdrawals: Stacks burn → signer approval → Bitcoin tx (PENDING→ACCEPTED→CONFIRMED)
//!
//! Integrates with the StacksListener to provide real-time sBTC liquidity tracking
//! alongside existing block-height synchronization.

use conxian_core::ConxianResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ── sBTC operation tracking ───────────────────────────────────────────────────

/// Lifecycle states mirroring the Emily API state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SbtcOperationState {
    #[default]
    Pending,
    Accepted,
    Confirmed,
    Failed,
}

/// Direction of the peg operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SbtcOperationKind {
    #[default]
    Deposit,
    Withdrawal,
}

/// A tracked sBTC peg operation (deposit or withdrawal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbtcOperation {
    /// Unique operation identifier (from Emily API).
    pub id: String,
    pub kind: SbtcOperationKind,
    pub state: SbtcOperationState,
    /// Bitcoin transaction ID (deposit: funding tx; withdrawal: payout tx).
    pub bitcoin_txid: Option<String>,
    /// Stacks transaction ID (deposit: mint tx; withdrawal: burn tx).
    pub stacks_txid: Option<String>,
    /// Amount in satoshis.
    pub amount_sats: u64,
    /// Recipient Stacks address (deposit) or Bitcoin address (withdrawal).
    pub recipient: String,
    /// Sender Stacks address (withdrawal) or Bitcoin address (deposit).
    pub sender: String,
    /// UNIX timestamp of last state transition.
    pub last_updated: u64,
    /// Number of signer confirmations received.
    pub signer_confirmations: u16,
}

/// Aggregated sBTC bridge metrics computed from tracked operations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SbtcBridgeMetrics {
    /// Total BTC deposited (in satoshis) across all confirmed deposits.
    pub total_deposited_sats: u64,
    /// Total BTC withdrawn (in satoshis) across all confirmed withdrawals.
    pub total_withdrawn_sats: u64,
    /// Current effective sBTC supply (deposited - withdrawn).
    pub circulating_sats: u64,
    /// Number of pending deposit operations.
    pub pending_deposits: u64,
    /// Number of pending withdrawal operations.
    pub pending_withdrawals: u64,
    /// Number of confirmed deposit operations.
    pub confirmed_deposits: u64,
    /// Number of confirmed withdrawal operations.
    pub confirmed_withdrawals: u64,
    /// Number of failed operations.
    pub failed_operations: u64,
    /// UNIX timestamp of the last metrics refresh.
    pub last_refresh: u64,
}

// ── Emily API client ──────────────────────────────────────────────────────────

/// Lightweight client for the sBTC Emily API.
///
/// The Emily API is the authoritative source for sBTC peg operation
/// status. It tracks deposits and withdrawals through four lifecycle
/// states: PENDING → ACCEPTED → CONFIRMED (or FAILED).
pub struct SbtcEmilyClient {
    base_url: String,
    client: reqwest::Client,
}

impl SbtcEmilyClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Fetches all deposit operations with optional state filter.
    pub async fn get_deposits(
        &self,
        state: Option<SbtcOperationState>,
    ) -> ConxianResult<Vec<SbtcOperation>> {
        let mut url = format!("{}/deposits", self.base_url);
        if let Some(s) = state {
            url.push_str(&format!("?state={}", state_to_query(s)));
        }
        self.fetch_operations(&url, SbtcOperationKind::Deposit)
            .await
    }

    /// Fetches all withdrawal operations with optional state filter.
    pub async fn get_withdrawals(
        &self,
        state: Option<SbtcOperationState>,
    ) -> ConxianResult<Vec<SbtcOperation>> {
        let mut url = format!("{}/withdrawals", self.base_url);
        if let Some(s) = state {
            url.push_str(&format!("?state={}", state_to_query(s)));
        }
        self.fetch_operations(&url, SbtcOperationKind::Withdrawal)
            .await
    }

    /// Fetches a single operation by ID.
    pub async fn get_operation(&self, id: &str) -> ConxianResult<Option<SbtcOperation>> {
        let url = format!("{}/operations/{}", self.base_url, id);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| conxian_core::ConxianError::Stacks(e.to_string()))?;

        if resp.status() == 404 {
            return Ok(None);
        }

        if !resp.status().is_success() {
            return Err(conxian_core::ConxianError::Stacks(format!(
                "Emily API error: status {}",
                resp.status()
            )));
        }

        let raw: EmilyOperationResponse = resp
            .json()
            .await
            .map_err(|e| conxian_core::ConxianError::Stacks(e.to_string()))?;

        Ok(Some(SbtcOperation {
            id: raw.id,
            kind: raw.kind,
            state: raw.state,
            bitcoin_txid: raw.bitcoin_txid,
            stacks_txid: raw.stacks_txid,
            amount_sats: raw.amount_sats,
            recipient: raw.recipient,
            sender: raw.sender,
            last_updated: raw.last_updated,
            signer_confirmations: raw.signer_confirmations,
        }))
    }

    /// Fetches health/readiness status from the Emily API.
    pub async fn health_check(&self) -> ConxianResult<bool> {
        let url = format!("{}/health", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| conxian_core::ConxianError::Stacks(e.to_string()))?;
        Ok(resp.status().is_success())
    }

    async fn fetch_operations(
        &self,
        url: &str,
        kind: SbtcOperationKind,
    ) -> ConxianResult<Vec<SbtcOperation>> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| conxian_core::ConxianError::Stacks(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(conxian_core::ConxianError::Stacks(format!(
                "Emily API error: status {}",
                resp.status()
            )));
        }

        let raw_ops: Vec<EmilyOperationResponse> = resp
            .json()
            .await
            .map_err(|e| conxian_core::ConxianError::Stacks(e.to_string()))?;

        Ok(raw_ops
            .into_iter()
            .map(|raw| SbtcOperation {
                id: raw.id,
                kind,
                state: raw.state,
                bitcoin_txid: raw.bitcoin_txid,
                stacks_txid: raw.stacks_txid,
                amount_sats: raw.amount_sats,
                recipient: raw.recipient,
                sender: raw.sender,
                last_updated: raw.last_updated,
                signer_confirmations: raw.signer_confirmations,
            })
            .collect())
    }
}

fn state_to_query(state: SbtcOperationState) -> &'static str {
    match state {
        SbtcOperationState::Pending => "pending",
        SbtcOperationState::Accepted => "accepted",
        SbtcOperationState::Confirmed => "confirmed",
        SbtcOperationState::Failed => "failed",
    }
}

// ── Emily API response types (internal) ───────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EmilyOperationResponse {
    id: String,
    #[serde(default)]
    kind: SbtcOperationKind,
    #[serde(default)]
    state: SbtcOperationState,
    #[serde(default)]
    bitcoin_txid: Option<String>,
    #[serde(default)]
    stacks_txid: Option<String>,
    #[serde(default)]
    amount_sats: u64,
    #[serde(default)]
    recipient: String,
    #[serde(default)]
    sender: String,
    #[serde(default = "default_timestamp")]
    last_updated: u64,
    #[serde(default)]
    signer_confirmations: u16,
}

fn default_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── sBTC Bridge Monitor ───────────────────────────────────────────────────────

/// Monitors sBTC peg operations and maintains aggregated bridge metrics.
///
/// Designed to be called from `StacksListener::sync_once()` alongside
/// existing block-height synchronization.
pub struct SbtcBridgeMonitor {
    client: SbtcEmilyClient,
    /// Tracked operations keyed by operation ID.
    operations: HashMap<String, SbtcOperation>,
    metrics: SbtcBridgeMetrics,
    /// Whether the Emily API is reachable.
    healthy: bool,
}

impl SbtcBridgeMonitor {
    pub fn new(emily_base_url: &str) -> Self {
        Self {
            client: SbtcEmilyClient::new(emily_base_url),
            operations: HashMap::new(),
            metrics: SbtcBridgeMetrics::default(),
            healthy: false,
        }
    }

    /// Returns the current aggregated bridge metrics.
    pub fn metrics(&self) -> &SbtcBridgeMetrics {
        &self.metrics
    }

    /// Returns whether the Emily API is reachable.
    pub fn is_healthy(&self) -> bool {
        self.healthy
    }

    /// Returns the current estimated sBTC liquidity in BTC (not sats).
    pub fn sbtc_liquidity_btc(&self) -> f64 {
        self.metrics.circulating_sats as f64 / 100_000_000.0
    }

    /// Synchronizes sBTC operation state from the Emily API.
    ///
    /// Fetches all pending and accepted deposits/withdrawals and
    /// recomputes aggregated metrics.
    pub async fn sync(&mut self) -> ConxianResult<()> {
        // Health check first
        self.healthy = self.client.health_check().await.unwrap_or(false);
        if !self.healthy {
            return Ok(());
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Fetch active operations (pending + accepted)
        let mut all_ops: Vec<SbtcOperation> = Vec::new();

        if let Ok(deposits) = self.client.get_deposits(None).await {
            all_ops.extend(deposits);
        }
        if let Ok(withdrawals) = self.client.get_withdrawals(None).await {
            all_ops.extend(withdrawals);
        }

        // Update operation store
        for op in &all_ops {
            self.operations.insert(op.id.clone(), op.clone());
        }

        // Recompute metrics
        self.recompute_metrics(now);

        Ok(())
    }

    fn recompute_metrics(&mut self, now: u64) {
        let mut metrics = SbtcBridgeMetrics::default();

        for op in self.operations.values() {
            match op.state {
                SbtcOperationState::Confirmed => match op.kind {
                    SbtcOperationKind::Deposit => {
                        metrics.total_deposited_sats += op.amount_sats;
                        metrics.confirmed_deposits += 1;
                    }
                    SbtcOperationKind::Withdrawal => {
                        metrics.total_withdrawn_sats += op.amount_sats;
                        metrics.confirmed_withdrawals += 1;
                    }
                },
                SbtcOperationState::Pending => match op.kind {
                    SbtcOperationKind::Deposit => metrics.pending_deposits += 1,
                    SbtcOperationKind::Withdrawal => metrics.pending_withdrawals += 1,
                },
                SbtcOperationState::Accepted => {
                    // Accepted but not yet confirmed — count as pending
                    match op.kind {
                        SbtcOperationKind::Deposit => metrics.pending_deposits += 1,
                        SbtcOperationKind::Withdrawal => metrics.pending_withdrawals += 1,
                    }
                }
                SbtcOperationState::Failed => metrics.failed_operations += 1,
            }
        }

        metrics.circulating_sats = metrics
            .total_deposited_sats
            .saturating_sub(metrics.total_withdrawn_sats);
        metrics.last_refresh = now;

        self.metrics = metrics;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_default_is_zero() {
        let m = SbtcBridgeMetrics::default();
        assert_eq!(m.total_deposited_sats, 0);
        assert_eq!(m.total_withdrawn_sats, 0);
        assert_eq!(m.circulating_sats, 0);
        assert_eq!(m.pending_deposits, 0);
        assert_eq!(m.confirmed_deposits, 0);
        assert_eq!(m.failed_operations, 0);
    }

    #[test]
    fn sbtc_liquidity_conversion() {
        let monitor = SbtcBridgeMonitor::new("http://localhost:3030");
        assert_eq!(monitor.sbtc_liquidity_btc(), 0.0);
    }

    #[test]
    fn state_to_query_mapping() {
        assert_eq!(state_to_query(SbtcOperationState::Pending), "pending");
        assert_eq!(state_to_query(SbtcOperationState::Accepted), "accepted");
        assert_eq!(state_to_query(SbtcOperationState::Confirmed), "confirmed");
        assert_eq!(state_to_query(SbtcOperationState::Failed), "failed");
    }

    #[test]
    fn operation_serde_roundtrip() {
        let op = SbtcOperation {
            id: "op-001".into(),
            kind: SbtcOperationKind::Deposit,
            state: SbtcOperationState::Pending,
            bitcoin_txid: Some("abc123".into()),
            stacks_txid: None,
            amount_sats: 100_000_000,
            recipient: "SP2...".into(),
            sender: "bc1...".into(),
            last_updated: 1700000000,
            signer_confirmations: 0,
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: SbtcOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "op-001");
        assert_eq!(back.amount_sats, 100_000_000);
    }

    #[test]
    fn metrics_circulating_sats_saturating_sub() {
        let mut metrics = SbtcBridgeMetrics::default();
        metrics.total_deposited_sats = 100_000_000;
        metrics.total_withdrawn_sats = 150_000_000;
        metrics.circulating_sats = metrics
            .total_deposited_sats
            .saturating_sub(metrics.total_withdrawn_sats);
        assert_eq!(metrics.circulating_sats, 0);
    }
}
