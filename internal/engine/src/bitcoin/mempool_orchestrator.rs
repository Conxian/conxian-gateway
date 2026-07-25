use crate::bitcoin::fee_bump_policy::{
    classify_stuck, decide_fee_bump, FeeBumpAction, FeeBumpCandidate, FeeBumpDecision,
    FeeBumpPolicyConfig,
};
use crate::bitcoin::BitcoinRpc;
use conxian_core::{
    ConxianError, ConxianResult, FeeBumpStrategy, MempoolTxStatus, Persistence, TrackedMempoolTx,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, timeout, Duration};
use tracing::{info, warn};
use uuid::Uuid;

const PERSISTENCE_ATTEMPTS: usize = 4;
const DEFAULT_LEASE_TTL_SECS: u64 = 120;

pub struct MempoolOrchestrator<R: BitcoinRpc> {
    rpc: R,
    persistence: Arc<dyn Persistence>,
    poll_interval_secs: u64,
    policy_config: FeeBumpPolicyConfig,
    // Kept as runtime wiring for future RGB-aware policy decisions. A Bitcoin
    // txid is not an RGB contract ID, so this orchestrator intentionally does
    // not synthesize `rgb:`/`contract:` identifiers for lookups.
    rgb_adapter: Option<Arc<dyn conxian_core::RgbAdapter>>,
    owner_id: String,
    lease_ttl_secs: u64,
    rpc_deadline: Duration,
}

struct ClaimedTransaction {
    snapshot: TrackedMempoolTx,
    lease_id: String,
    record_generation: u64,
}

impl<R: BitcoinRpc> MempoolOrchestrator<R> {
    pub fn new(
        rpc: R,
        persistence: Arc<dyn Persistence>,
        poll_interval_secs: u64,
        policy_config: FeeBumpPolicyConfig,
        rgb_adapter: Option<Arc<dyn conxian_core::RgbAdapter>>,
    ) -> Self {
        Self {
            rpc,
            persistence,
            poll_interval_secs,
            policy_config,
            rgb_adapter,
            owner_id: Uuid::new_v4().to_string(),
            lease_ttl_secs: DEFAULT_LEASE_TTL_SECS,
            rpc_deadline: Duration::from_secs(DEFAULT_LEASE_TTL_SECS - 15),
        }
    }

    #[cfg(test)]
    fn with_owner(mut self, owner_id: &str, lease_ttl_secs: u64) -> Self {
        self.owner_id = owner_id.to_string();
        self.lease_ttl_secs = lease_ttl_secs;
        self.rpc_deadline = Duration::from_secs(lease_ttl_secs.saturating_sub(1).max(1));
        self
    }

    #[cfg(test)]
    fn with_rpc_deadline(mut self, deadline: Duration) -> Self {
        self.rpc_deadline = deadline;
        self
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!(
            poll_interval_secs = self.poll_interval_secs,
            rgb_adapter_configured = self.rgb_adapter.is_some(),
            "Starting mempool orchestrator"
        );

        loop {
            if let Err(err) = self.tick().await {
                warn!("Mempool orchestrator tick failed: {}", err);
            }
            sleep(Duration::from_secs(self.poll_interval_secs)).await;
        }
    }

    pub async fn tick(&self) -> anyhow::Result<()> {
        self.tick_at(unix_now()).await
    }

    async fn tick_at(&self, now: u64) -> anyhow::Result<()> {
        let txids = self
            .persistence
            .load()
            .map_err(|err| anyhow::anyhow!(err.to_string()))?
            .mempool_pending_txs
            .into_iter()
            .map(|tx| tx.txid)
            .collect::<Vec<_>>();

        for txid in txids {
            let Some(claimed) = self
                .claim_transaction(&txid, now)
                .map_err(|err| anyhow::anyhow!(err.to_string()))?
            else {
                continue;
            };
            let mut tracked = claimed.snapshot.clone();
            if timeout(
                self.rpc_deadline,
                self.evaluate_pending_tx(&mut tracked, now),
            )
            .await
            .is_err()
            {
                tracked.status = MempoolTxStatus::BumpOutcomeUnknown;
                tracked.last_error = Some(
                    "fee-bump RPC deadline exceeded; reconcile node state before retry".to_string(),
                );
            }
            self.complete_transaction(&txid, &claimed, tracked)
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        }
        Ok(())
    }

    fn claim_transaction(&self, txid: &str, now: u64) -> ConxianResult<Option<ClaimedTransaction>> {
        let lease_expires_at = now.checked_add(self.lease_ttl_secs).ok_or_else(|| {
            ConxianError::Persistence("mempool lease expiry overflow".to_string())
        })?;
        for attempt in 0..PERSISTENCE_ATTEMPTS {
            let current = self.persistence.load_versioned()?;
            let mut next = current.state.clone();
            let Some(tx) = next
                .mempool_pending_txs
                .iter_mut()
                .find(|tx| tx.txid == txid)
            else {
                return Ok(None);
            };
            if matches!(
                tx.status,
                MempoolTxStatus::Confirmed
                    | MempoolTxStatus::BumpBroadcasted
                    | MempoolTxStatus::BumpOutcomeUnknown
            ) {
                return Ok(None);
            }
            if tx
                .lease_expires_at
                .is_some_and(|expires_at| expires_at > now)
            {
                return Ok(None);
            }
            let lease_id = Uuid::new_v4().to_string();
            tx.record_generation = tx.record_generation.checked_add(1).ok_or_else(|| {
                ConxianError::Persistence("mempool record generation overflow".to_string())
            })?;
            tx.lease_owner = Some(self.owner_id.clone());
            tx.lease_id = Some(lease_id.clone());
            tx.lease_expires_at = Some(lease_expires_at);
            let claimed = tx.clone();
            match self.persistence.compare_and_swap(current.revision, &next) {
                Ok(_) => {
                    return Ok(Some(ClaimedTransaction {
                        record_generation: claimed.record_generation,
                        snapshot: claimed,
                        lease_id,
                    }))
                }
                Err(ConxianError::PersistenceConflict { .. })
                    if attempt + 1 < PERSISTENCE_ATTEMPTS => {}
                Err(error) => return Err(error),
            }
        }
        Err(ConxianError::Persistence(
            "mempool claim exhausted conflict retries".to_string(),
        ))
    }

    fn complete_transaction(
        &self,
        txid: &str,
        claimed: &ClaimedTransaction,
        mut completed: TrackedMempoolTx,
    ) -> ConxianResult<()> {
        for attempt in 0..PERSISTENCE_ATTEMPTS {
            let current = self.persistence.load_versioned()?;
            let mut next = current.state.clone();
            let Some(tx) = next
                .mempool_pending_txs
                .iter_mut()
                .find(|tx| tx.txid == txid)
            else {
                return Err(ConxianError::PersistenceLeaseLost {
                    txid: txid.to_string(),
                    owner: self.owner_id.clone(),
                });
            };
            if tx.lease_owner.as_deref() != Some(&self.owner_id)
                || tx.lease_id.as_deref() != Some(&claimed.lease_id)
                || tx.record_generation != claimed.record_generation
            {
                return Err(ConxianError::PersistenceLeaseLost {
                    txid: txid.to_string(),
                    owner: self.owner_id.clone(),
                });
            }
            completed.lease_owner = None;
            completed.lease_id = None;
            completed.lease_expires_at = None;
            completed.record_generation = claimed.record_generation;
            *tx = completed.clone();
            match self.persistence.compare_and_swap(current.revision, &next) {
                Ok(_) => return Ok(()),
                Err(ConxianError::PersistenceConflict { .. })
                    if attempt + 1 < PERSISTENCE_ATTEMPTS => {}
                Err(error) => return Err(error),
            }
        }
        Err(ConxianError::Persistence(
            "mempool completion exhausted conflict retries".to_string(),
        ))
    }

    async fn evaluate_pending_tx(&self, tx: &mut TrackedMempoolTx, now: u64) {
        if matches!(tx.status, MempoolTxStatus::Confirmed) {
            return;
        }

        tx.last_evaluated_at = Some(now);

        // RGB lookups require a real `contract:` ID from RGB state. The
        // tracked Bitcoin transaction has no such source, so skip the lookup.

        let candidate = FeeBumpCandidate {
            txid: tx.txid.clone(),
            first_seen_at: tx.first_seen_at,
            last_bump_at: tx.last_bump_at,
            bump_attempts: tx.bump_attempts,
            current_fee_rate_sat_vb: tx.current_fee_rate_sat_vb,
            target_fee_rate_sat_vb: tx.target_fee_rate_sat_vb,
            rbf_allowed: tx.replaceable,
            cpfp_allowed: tx.cpfp_eligible,
        };

        let classification = classify_stuck(&candidate, now, &self.policy_config);
        tx.status = if classification.is_stuck() {
            MempoolTxStatus::Stuck
        } else {
            MempoolTxStatus::Pending
        };

        match decide_fee_bump(&candidate, now, &self.policy_config) {
            FeeBumpDecision::NoAction { .. } => {
                tx.last_error = None;
            }
            FeeBumpDecision::Reject { reason } => {
                tx.status = MempoolTxStatus::GuardrailRejected;
                tx.last_error = Some(format!("{reason:?}"));
            }
            FeeBumpDecision::Execute(action) => {
                tx.bump_attempts += 1;
                tx.last_bump_at = Some(now);

                match self.execute_action(tx, &action).await {
                    ExecutionResult::Broadcasted {
                        strategy,
                        replacement_txid,
                    } => {
                        tx.status = MempoolTxStatus::BumpBroadcasted;
                        tx.current_fee_rate_sat_vb = action.target_fee_rate_sat_vb;
                        tx.last_bump_strategy = Some(strategy);
                        tx.replacement_txid = Some(replacement_txid);
                        tx.last_error = None;
                    }
                    ExecutionResult::NotBroadcasted { strategy, reason } => {
                        tx.status = MempoolTxStatus::Stuck;
                        tx.last_bump_strategy = Some(strategy);
                        tx.last_error = Some(reason);
                    }
                    ExecutionResult::OutcomeUnknown { strategy, reason } => {
                        tx.status = MempoolTxStatus::BumpOutcomeUnknown;
                        tx.last_bump_strategy = Some(strategy);
                        tx.last_error = Some(reason);
                    }
                }
            }
        }
    }

    async fn execute_action(
        &self,
        tx: &TrackedMempoolTx,
        action: &FeeBumpAction,
    ) -> ExecutionResult {
        match action.strategy {
            FeeBumpStrategy::Rbf => {
                match self
                    .rpc
                    .submit_rbf_replacement(&tx.txid, action.target_fee_rate_sat_vb)
                    .await
                {
                    Ok(Some(replacement_txid)) => ExecutionResult::Broadcasted {
                        strategy: FeeBumpStrategy::Rbf,
                        replacement_txid,
                    },
                    Ok(None) => {
                        self.try_cpfp_fallback(tx, action, "RBF replacement not available")
                            .await
                    }
                    Err(err) => ExecutionResult::OutcomeUnknown {
                        strategy: FeeBumpStrategy::Rbf,
                        reason: format!(
                            "RBF submission outcome unknown; reconcile node state before retry: {err}"
                        ),
                    },
                }
            }
            FeeBumpStrategy::Cpfp => {
                match self
                    .rpc
                    .submit_cpfp_child(&tx.txid, action.target_fee_rate_sat_vb)
                    .await
                {
                    Ok(Some(replacement_txid)) => ExecutionResult::Broadcasted {
                        strategy: FeeBumpStrategy::Cpfp,
                        replacement_txid,
                    },
                    Ok(None) => ExecutionResult::NotBroadcasted {
                        strategy: FeeBumpStrategy::Cpfp,
                        reason: "CPFP child broadcast unavailable: adapter cannot construct/sign a child transaction with current context".to_string(),
                    },
                    Err(err) => ExecutionResult::OutcomeUnknown {
                        strategy: FeeBumpStrategy::Cpfp,
                        reason: format!(
                            "CPFP submission outcome unknown; reconcile node state before retry: {err}"
                        ),
                    },
                }
            }
        }
    }

    async fn try_cpfp_fallback(
        &self,
        tx: &TrackedMempoolTx,
        action: &FeeBumpAction,
        reason: &str,
    ) -> ExecutionResult {
        if !tx.cpfp_eligible {
            return ExecutionResult::NotBroadcasted {
                strategy: FeeBumpStrategy::Rbf,
                reason: reason.to_string(),
            };
        }

        match self
            .rpc
            .submit_cpfp_child(&tx.txid, action.target_fee_rate_sat_vb)
            .await
        {
            Ok(Some(replacement_txid)) => ExecutionResult::Broadcasted {
                strategy: FeeBumpStrategy::Cpfp,
                replacement_txid,
            },
            Ok(None) => ExecutionResult::NotBroadcasted {
                strategy: FeeBumpStrategy::Cpfp,
                reason: format!(
                    "{}; CPFP fallback unavailable: adapter cannot construct/sign a child transaction with current context",
                    reason
                ),
            },
            Err(err) => ExecutionResult::OutcomeUnknown {
                strategy: FeeBumpStrategy::Cpfp,
                reason: format!(
                    "{}; CPFP fallback outcome unknown; reconcile node state before retry: {}",
                    reason, err
                ),
            },
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

enum ExecutionResult {
    Broadcasted {
        strategy: FeeBumpStrategy,
        replacement_txid: String,
    },
    NotBroadcasted {
        strategy: FeeBumpStrategy,
        reason: String,
    },
    OutcomeUnknown {
        strategy: FeeBumpStrategy,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitcoin::NodeRgbAdapter;
    use async_trait::async_trait;
    use conxian_core::{
        BlockInfo, ConxianError, ConxianResult, MempoolTxStatus, PersistentState, RolloutMode,
        VersionedPersistentState,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use tokio::sync::Notify;

    struct SimulatedPersistence {
        state: Mutex<VersionedPersistentState>,
        unknown_revision: Mutex<Option<u64>>,
    }

    impl SimulatedPersistence {
        fn new(state: PersistentState) -> Self {
            Self {
                state: Mutex::new(VersionedPersistentState { revision: 0, state }),
                unknown_revision: Mutex::new(None),
            }
        }

        fn with_unknown_revision(state: PersistentState, revision: u64) -> Self {
            Self {
                state: Mutex::new(VersionedPersistentState { revision: 0, state }),
                unknown_revision: Mutex::new(Some(revision)),
            }
        }
    }

    impl Persistence for SimulatedPersistence {
        fn load_versioned(&self) -> ConxianResult<VersionedPersistentState> {
            Ok(self.state.lock().expect("lock poisoned").clone())
        }

        fn compare_and_swap(
            &self,
            expected_revision: u64,
            state: &PersistentState,
        ) -> ConxianResult<VersionedPersistentState> {
            let mut current = self.state.lock().expect("lock poisoned");
            if current.revision != expected_revision {
                return Err(ConxianError::PersistenceConflict {
                    expected: expected_revision,
                    actual: current.revision,
                });
            }
            current.revision += 1;
            current.state = state.clone();
            let committed = current.clone();
            let mut unknown_revision = self.unknown_revision.lock().expect("lock poisoned");
            if *unknown_revision == Some(committed.revision) {
                *unknown_revision = None;
                return Err(ConxianError::PersistenceCommitUnknown {
                    revision: committed.revision,
                    message: "injected post-rename failure".to_string(),
                });
            }
            Ok(committed)
        }
    }

    struct SimulatedBitcoinRpc {
        rbf_txid: Option<String>,
        cpfp_txid: Option<String>,
        rbf_calls: Arc<AtomicUsize>,
        cpfp_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl BitcoinRpc for SimulatedBitcoinRpc {
        async fn get_block_count(&self) -> ConxianResult<u64> {
            Ok(0)
        }

        async fn get_block_info(&self, _height: u64) -> ConxianResult<BlockInfo> {
            Err(ConxianError::Bitcoin("not used".to_string()))
        }

        async fn get_network_info(&self) -> ConxianResult<String> {
            Ok("regtest".to_string())
        }

        async fn submit_rbf_replacement(
            &self,
            _txid: &str,
            _target_fee_rate_sat_vb: u64,
        ) -> ConxianResult<Option<String>> {
            self.rbf_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.rbf_txid.clone())
        }

        async fn submit_cpfp_child(
            &self,
            _parent_txid: &str,
            _target_fee_rate_sat_vb: u64,
        ) -> ConxianResult<Option<String>> {
            self.cpfp_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.cpfp_txid.clone())
        }
    }

    struct BlockingBitcoinRpc {
        calls: Arc<AtomicUsize>,
        started: Arc<Notify>,
        release: Arc<Notify>,
    }

    #[async_trait]
    impl BitcoinRpc for BlockingBitcoinRpc {
        async fn get_block_count(&self) -> ConxianResult<u64> {
            Ok(0)
        }

        async fn get_block_info(&self, _height: u64) -> ConxianResult<BlockInfo> {
            Err(ConxianError::Bitcoin("not used".to_string()))
        }

        async fn get_network_info(&self) -> ConxianResult<String> {
            Ok("regtest".to_string())
        }

        async fn submit_rbf_replacement(
            &self,
            _txid: &str,
            _target_fee_rate_sat_vb: u64,
        ) -> ConxianResult<Option<String>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_waiters();
            self.release.notified().await;
            Ok(Some("rbf-blocking".to_string()))
        }

        async fn submit_cpfp_child(
            &self,
            _parent_txid: &str,
            _target_fee_rate_sat_vb: u64,
        ) -> ConxianResult<Option<String>> {
            Ok(None)
        }
    }

    fn simulated_rpc(
        rbf_txid: Option<&str>,
        cpfp_txid: Option<&str>,
    ) -> (SimulatedBitcoinRpc, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let rbf_calls = Arc::new(AtomicUsize::new(0));
        let cpfp_calls = Arc::new(AtomicUsize::new(0));
        (
            SimulatedBitcoinRpc {
                rbf_txid: rbf_txid.map(str::to_string),
                cpfp_txid: cpfp_txid.map(str::to_string),
                rbf_calls: Arc::clone(&rbf_calls),
                cpfp_calls: Arc::clone(&cpfp_calls),
            },
            rbf_calls,
            cpfp_calls,
        )
    }

    fn tracked_tx() -> TrackedMempoolTx {
        TrackedMempoolTx {
            txid: "parent-tx".to_string(),
            first_seen_at: 0,
            last_evaluated_at: None,
            last_bump_at: None,
            bump_attempts: 0,
            current_fee_rate_sat_vb: 10,
            target_fee_rate_sat_vb: Some(14),
            replaceable: true,
            cpfp_eligible: true,
            status: MempoolTxStatus::Pending,
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
    fn legacy_tracked_transaction_defaults_fencing_fields() {
        let value = serde_json::json!({
            "txid": "legacy",
            "first_seen_at": 1,
            "last_evaluated_at": null,
            "last_bump_at": null,
            "bump_attempts": 0,
            "current_fee_rate_sat_vb": 1,
            "target_fee_rate_sat_vb": null,
            "replaceable": false,
            "cpfp_eligible": false,
            "status": "PENDING",
            "last_bump_strategy": null,
            "last_error": null,
            "replacement_txid": null,
            "lease_owner": null,
            "lease_expires_at": null
        });
        let tx: TrackedMempoolTx = serde_json::from_value(value).unwrap();
        assert_eq!(tx.lease_id, None);
        assert_eq!(tx.record_generation, 0);
    }

    fn test_policy() -> FeeBumpPolicyConfig {
        FeeBumpPolicyConfig {
            stuck_threshold_secs: 1,
            max_attempts: 3,
            max_fee_rate_sat_vb: 200,
            min_bump_increment_sat_vb: 2,
        }
    }

    #[tokio::test]
    async fn orchestrator_rbf_success_path() {
        let rgb_adapter = Arc::new(NodeRgbAdapter::new(
            RolloutMode::Shadow,
            "https://example.invalid".to_string(),
        ));
        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![tracked_tx()],
        }));

        let orchestrator = MempoolOrchestrator::new(
            SimulatedBitcoinRpc {
                rbf_txid: Some("rbf-tx".to_string()),
                cpfp_txid: Some("cpfp-tx".to_string()),
                rbf_calls: Arc::new(AtomicUsize::new(0)),
                cpfp_calls: Arc::new(AtomicUsize::new(0)),
            },
            persistence.clone(),
            30,
            test_policy(),
            Some(rgb_adapter),
        );

        orchestrator.tick().await.unwrap();

        let state = persistence.load().unwrap();
        let tx = &state.mempool_pending_txs[0];
        assert_eq!(tx.status, MempoolTxStatus::BumpBroadcasted);
        assert_eq!(tx.last_bump_strategy, Some(FeeBumpStrategy::Rbf));
        assert_eq!(tx.replacement_txid.as_deref(), Some("rbf-tx"));
        assert_eq!(tx.bump_attempts, 1);
    }

    #[tokio::test]
    async fn orchestrator_cpfp_fallback_when_rbf_unavailable() {
        let rgb_adapter = Arc::new(NodeRgbAdapter::new(
            RolloutMode::Shadow,
            "https://example.invalid".to_string(),
        ));
        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![tracked_tx()],
        }));

        let (rpc, _, _) = simulated_rpc(None, Some("cpfp-tx"));
        let orchestrator = MempoolOrchestrator::new(
            rpc,
            persistence.clone(),
            30,
            test_policy(),
            Some(rgb_adapter),
        );

        orchestrator.tick().await.unwrap();

        let state = persistence.load().unwrap();
        let tx = &state.mempool_pending_txs[0];
        assert_eq!(tx.status, MempoolTxStatus::BumpBroadcasted);
        assert_eq!(tx.last_bump_strategy, Some(FeeBumpStrategy::Cpfp));
        assert_eq!(tx.replacement_txid.as_deref(), Some("cpfp-tx"));
        assert_eq!(tx.bump_attempts, 1);
    }

    #[tokio::test]
    async fn orchestrator_guardrail_rejection_path() {
        let rgb_adapter = Arc::new(NodeRgbAdapter::new(
            RolloutMode::Shadow,
            "https://example.invalid".to_string(),
        ));
        let mut tx = tracked_tx();
        tx.bump_attempts = 3;

        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![tx],
        }));

        let orchestrator = MempoolOrchestrator::new(
            SimulatedBitcoinRpc {
                rbf_txid: Some("rbf-tx".to_string()),
                cpfp_txid: Some("cpfp-tx".to_string()),
                rbf_calls: Arc::new(AtomicUsize::new(0)),
                cpfp_calls: Arc::new(AtomicUsize::new(0)),
            },
            persistence.clone(),
            30,
            test_policy(),
            Some(rgb_adapter),
        );

        orchestrator.tick().await.unwrap();

        let state = persistence.load().unwrap();
        let tx = &state.mempool_pending_txs[0];
        assert_eq!(tx.status, MempoolTxStatus::GuardrailRejected);
        assert!(tx
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("MaxAttemptsReached"));
    }

    #[tokio::test]
    async fn orchestrator_cpfp_not_available_records_stuck_reason() {
        let mut tx = tracked_tx();
        tx.replaceable = false;
        tx.cpfp_eligible = true;

        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![tx],
        }));

        let (rpc, _, _) = simulated_rpc(None, None);
        let orchestrator =
            MempoolOrchestrator::new(rpc, persistence.clone(), 30, test_policy(), None);

        orchestrator.tick().await.unwrap();

        let state = persistence.load().unwrap();
        let tx = &state.mempool_pending_txs[0];
        assert_eq!(tx.status, MempoolTxStatus::Stuck);
        assert_eq!(tx.last_bump_strategy, Some(FeeBumpStrategy::Cpfp));
        assert!(tx
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("CPFP child broadcast unavailable"));
    }

    #[tokio::test]
    async fn active_claim_allows_only_one_orchestrator_to_broadcast() {
        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            bitcoin_height: 11,
            stacks_height: 22,
            mempool_pending_txs: vec![tracked_tx()],
        }));
        let (rpc_a, rbf_calls, _) = simulated_rpc(Some("rbf-a"), None);
        let (rpc_b, _, _) = simulated_rpc(Some("rbf-b"), None);
        let first = MempoolOrchestrator::new(rpc_a, persistence.clone(), 30, test_policy(), None)
            .with_owner("owner-a", 10);
        let second = MempoolOrchestrator::new(rpc_b, persistence.clone(), 30, test_policy(), None)
            .with_owner("owner-b", 10);

        assert!(first.claim_transaction("parent-tx", 100).unwrap().is_some());
        second.tick_at(101).await.unwrap();
        assert_eq!(rbf_calls.load(Ordering::SeqCst), 0);
        let tx = &persistence.load().unwrap().mempool_pending_txs[0];
        assert_eq!(tx.lease_owner.as_deref(), Some("owner-a"));
        assert_eq!(tx.lease_expires_at, Some(110));
    }

    #[tokio::test]
    async fn concurrent_same_owner_ticks_submit_exactly_once() {
        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            mempool_pending_txs: vec![tracked_tx()],
            ..PersistentState::default()
        }));
        let calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let orchestrator = Arc::new(
            MempoolOrchestrator::new(
                BlockingBitcoinRpc {
                    calls: calls.clone(),
                    started: started.clone(),
                    release: release.clone(),
                },
                persistence,
                30,
                test_policy(),
                None,
            )
            .with_owner("same-owner", 10),
        );

        let first = tokio::spawn({
            let orchestrator = orchestrator.clone();
            async move { orchestrator.tick_at(100).await }
        });
        started.notified().await;
        let second = tokio::spawn({
            let orchestrator = orchestrator.clone();
            async move { orchestrator.tick_at(100).await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        release.notify_waiters();
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rpc_deadline_records_reconciliation_required_and_prevents_replay() {
        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            mempool_pending_txs: vec![tracked_tx()],
            ..PersistentState::default()
        }));
        let calls = Arc::new(AtomicUsize::new(0));
        let orchestrator = MempoolOrchestrator::new(
            BlockingBitcoinRpc {
                calls: calls.clone(),
                started: Arc::new(Notify::new()),
                release: Arc::new(Notify::new()),
            },
            persistence.clone(),
            30,
            test_policy(),
            None,
        )
        .with_owner("deadline-owner", 5)
        .with_rpc_deadline(Duration::from_millis(20));

        orchestrator.tick_at(100).await.unwrap();
        let tx = &persistence.load().unwrap().mempool_pending_txs[0];
        assert_eq!(tx.status, MempoolTxStatus::BumpOutcomeUnknown);
        assert!(tx.last_error.as_deref().unwrap().contains("reconcile"));
        orchestrator.tick_at(106).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stale_completion_cannot_overwrite_concurrent_reconciliation() {
        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            mempool_pending_txs: vec![tracked_tx()],
            ..PersistentState::default()
        }));
        let (rpc, _, _) = simulated_rpc(Some("rbf"), None);
        let orchestrator =
            MempoolOrchestrator::new(rpc, persistence.clone(), 30, test_policy(), None)
                .with_owner("owner-a", 10);
        let claimed = orchestrator
            .claim_transaction("parent-tx", 100)
            .unwrap()
            .unwrap();
        let current = persistence.load_versioned().unwrap();
        let mut reconciled = current.state.clone();
        let tx = &mut reconciled.mempool_pending_txs[0];
        tx.status = MempoolTxStatus::Confirmed;
        tx.record_generation += 1;
        tx.lease_owner = None;
        tx.lease_id = None;
        tx.lease_expires_at = None;
        persistence
            .compare_and_swap(current.revision, &reconciled)
            .unwrap();

        let mut stale = claimed.snapshot.clone();
        stale.status = MempoolTxStatus::BumpBroadcasted;
        assert!(matches!(
            orchestrator.complete_transaction("parent-tx", &claimed, stale),
            Err(ConxianError::PersistenceLeaseLost { .. })
        ));
        assert_eq!(
            persistence.load().unwrap().mempool_pending_txs[0].status,
            MempoolTxStatus::Confirmed
        );
    }

    #[tokio::test]
    async fn expired_claim_is_recovered_by_another_orchestrator() {
        let persistence = Arc::new(SimulatedPersistence::new(PersistentState {
            bitcoin_height: 11,
            stacks_height: 22,
            mempool_pending_txs: vec![tracked_tx()],
        }));
        let (rpc_a, _, _) = simulated_rpc(Some("rbf-a"), None);
        let (rpc_b, rbf_calls, _) = simulated_rpc(Some("rbf-b"), None);
        let first = MempoolOrchestrator::new(rpc_a, persistence.clone(), 30, test_policy(), None)
            .with_owner("owner-a", 5);
        let second = MempoolOrchestrator::new(rpc_b, persistence.clone(), 30, test_policy(), None)
            .with_owner("owner-b", 5);

        assert!(first.claim_transaction("parent-tx", 100).unwrap().is_some());
        second.tick_at(106).await.unwrap();
        assert_eq!(rbf_calls.load(Ordering::SeqCst), 1);
        let state = persistence.load().unwrap();
        let tx = &state.mempool_pending_txs[0];
        assert_eq!(tx.status, MempoolTxStatus::BumpBroadcasted);
        assert_eq!(tx.replacement_txid.as_deref(), Some("rbf-b"));
        assert_eq!(tx.lease_owner, None);
        assert_eq!(state.bitcoin_height, 11);
        assert_eq!(state.stacks_height, 22);
    }

    #[tokio::test]
    async fn unknown_completion_commit_is_not_replayed() {
        let persistence = Arc::new(SimulatedPersistence::with_unknown_revision(
            PersistentState {
                bitcoin_height: 0,
                stacks_height: 0,
                mempool_pending_txs: vec![tracked_tx()],
            },
            2,
        ));
        let (rpc, rbf_calls, _) = simulated_rpc(Some("rbf-tx"), None);
        let orchestrator =
            MempoolOrchestrator::new(rpc, persistence.clone(), 30, test_policy(), None)
                .with_owner("owner-a", 5);

        let error = orchestrator.tick_at(100).await.expect_err("commit unknown");
        assert!(error.to_string().contains("commit outcome is unknown"));
        assert_eq!(rbf_calls.load(Ordering::SeqCst), 1);
        let tx = &persistence.load().unwrap().mempool_pending_txs[0];
        assert_eq!(tx.status, MempoolTxStatus::BumpBroadcasted);

        orchestrator.tick_at(101).await.unwrap();
        assert_eq!(rbf_calls.load(Ordering::SeqCst), 1);
    }
}
