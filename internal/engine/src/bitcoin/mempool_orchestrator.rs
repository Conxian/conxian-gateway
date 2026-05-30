use crate::bitcoin::fee_bump_policy::{
    classify_stuck, decide_fee_bump, FeeBumpAction, FeeBumpCandidate, FeeBumpDecision,
    FeeBumpPolicyConfig,
};
use crate::bitcoin::BitcoinRpc;
use conxian_core::{FeeBumpStrategy, MempoolTxStatus, Persistence, TrackedMempoolTx};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

pub struct MempoolOrchestrator<R: BitcoinRpc> {
    rpc: R,
    persistence: Arc<dyn Persistence>,
    poll_interval_secs: u64,
    policy_config: FeeBumpPolicyConfig,
    rgb_adapter: Option<Arc<dyn conxian_core::RgbAdapter>>,
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
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!(
            "Starting mempool orchestrator with poll interval {}s",
            self.poll_interval_secs
        );

        loop {
            if let Err(err) = self.tick().await {
                warn!("Mempool orchestrator tick failed: {}", err);
            }
            sleep(Duration::from_secs(self.poll_interval_secs)).await;
        }
    }

    pub async fn tick(&self) -> anyhow::Result<()> {
        let mut persisted = self
            .persistence
            .load()
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;

        let now = unix_now();
        for idx in 0..persisted.mempool_pending_txs.len() {
            let mut tracked = persisted.mempool_pending_txs[idx].clone();
            self.evaluate_pending_tx(&mut tracked, now).await;
            persisted.mempool_pending_txs[idx] = tracked;
        }

        self.persistence
            .save(&persisted)
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;

        Ok(())
    }

    async fn evaluate_pending_tx(&self, tx: &mut TrackedMempoolTx, now: u64) {
        if matches!(tx.status, MempoolTxStatus::Confirmed) {
            return;
        }

        tx.last_evaluated_at = Some(now);

        // CON-768: Shadow-mode RGB contract lookup
        if let Some(ref rgb) = self.rgb_adapter {
            let _ = rgb.lookup_contract(&format!("rgb:{}", tx.txid)).await;
        }

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
                    Err(err) => {
                        self.try_cpfp_fallback(
                            tx,
                            action,
                            &format!("RBF replacement failed: {}", err),
                        )
                        .await
                    }
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
                        reason: "CPFP child broadcast not available (TODO: implement wallet signing + sendrawtransaction)".to_string(),
                    },
                    Err(err) => ExecutionResult::NotBroadcasted {
                        strategy: FeeBumpStrategy::Cpfp,
                        reason: format!("CPFP child submission failed: {}", err),
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
                    "{}; CPFP fallback not available (TODO: implement child tx construction + broadcast)",
                    reason
                ),
            },
            Err(err) => ExecutionResult::NotBroadcasted {
                strategy: FeeBumpStrategy::Cpfp,
                reason: format!("{}; CPFP fallback failed: {}", reason, err),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitcoin::NodeRgbAdapter;
    use async_trait::async_trait;
    use conxian_core::{
        BlockInfo, ConxianError, ConxianResult, MempoolTxStatus, PersistentState, RolloutMode,
    };
    use std::sync::Mutex;

    struct MockPersistence {
        state: Mutex<PersistentState>,
    }

    impl MockPersistence {
        fn new(state: PersistentState) -> Self {
            Self {
                state: Mutex::new(state),
            }
        }
    }

    impl Persistence for MockPersistence {
        fn save(&self, state: &PersistentState) -> ConxianResult<()> {
            *self.state.lock().unwrap() = state.clone();
            Ok(())
        }

        fn load(&self) -> ConxianResult<PersistentState> {
            Ok(self.state.lock().unwrap().clone())
        }
    }

    struct MockBitcoinRpc {
        rbf_txid: Option<String>,
        cpfp_txid: Option<String>,
    }

    #[async_trait]
    impl BitcoinRpc for MockBitcoinRpc {
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
            Ok(self.rbf_txid.clone())
        }

        async fn submit_cpfp_child(
            &self,
            _parent_txid: &str,
            _target_fee_rate_sat_vb: u64,
        ) -> ConxianResult<Option<String>> {
            Ok(self.cpfp_txid.clone())
        }
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
        }
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
            "http://localhost:8080".to_string(),
        ));
        let persistence = Arc::new(MockPersistence::new(PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![tracked_tx()],
        }));

        let orchestrator = MempoolOrchestrator::new(
            MockBitcoinRpc {
                rbf_txid: Some("rbf-tx".to_string()),
                cpfp_txid: Some("cpfp-tx".to_string()),
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
            "http://localhost:8080".to_string(),
        ));
        let persistence = Arc::new(MockPersistence::new(PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![tracked_tx()],
        }));

        let orchestrator = MempoolOrchestrator::new(
            MockBitcoinRpc {
                rbf_txid: None,
                cpfp_txid: Some("cpfp-tx".to_string()),
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
        assert_eq!(tx.last_bump_strategy, Some(FeeBumpStrategy::Cpfp));
        assert_eq!(tx.replacement_txid.as_deref(), Some("cpfp-tx"));
        assert_eq!(tx.bump_attempts, 1);
    }

    #[tokio::test]
    async fn orchestrator_guardrail_rejection_path() {
        let rgb_adapter = Arc::new(NodeRgbAdapter::new(
            RolloutMode::Shadow,
            "http://localhost:8080".to_string(),
        ));
        let mut tx = tracked_tx();
        tx.bump_attempts = 3;

        let persistence = Arc::new(MockPersistence::new(PersistentState {
            bitcoin_height: 0,
            stacks_height: 0,
            mempool_pending_txs: vec![tx],
        }));

        let orchestrator = MempoolOrchestrator::new(
            MockBitcoinRpc {
                rbf_txid: Some("rbf-tx".to_string()),
                cpfp_txid: Some("cpfp-tx".to_string()),
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
}
