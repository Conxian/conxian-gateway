use crate::coordination::{RedisCoordinator, StateRootPublisher};
use crate::persistence::AsyncPersistence;
use crate::stacks::rpc::StacksRpc;
use conxian_core::{ConxianResult, Persistence, SharedState};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct StacksListener<R: StacksRpc> {
    rpc: R,
    state: SharedState,
    persistence: AsyncPersistence,
    coordinator: Option<Arc<dyn StateRootPublisher>>,
    last_height: u64,
    sync_interval: u64,
}

impl<R: StacksRpc> StacksListener<R> {
    pub async fn new(
        rpc: R,
        state: SharedState,
        persistence: Arc<dyn Persistence>,
        coordinator: Option<Arc<RedisCoordinator>>,
        sync_interval: u64,
    ) -> ConxianResult<Self> {
        let persistence = AsyncPersistence::new(persistence);
        let last_height = persistence.load().await?.stacks_height;
        let coordinator = coordinator.map(|value| value as Arc<dyn StateRootPublisher>);
        Ok(Self {
            rpc,
            state,
            persistence,
            coordinator,
            last_height,
            sync_interval,
        })
    }

    pub async fn sync_once(&mut self) -> ConxianResult<()> {
        match self.rpc.get_network_info().await {
            Ok(info) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock moved backwards")
                    .as_secs();

                if info.height > self.last_height || self.last_height == 0 {
                    info!("New Stacks block processed: height={}, network={}, epoch={}, burn_height={}", info.height, info.network, info.epoch, info.burn_block_height);

                    let height = info.height;
                    self.persistence
                        .transactional_update(4, move |state| {
                            state.stacks_height = height;
                            Ok(())
                        })
                        .await?;

                    {
                        let mut state = self.state.write().expect("lock poisoned");
                        state.stacks.height = info.height;
                        state.stacks.status = "synced".to_string();
                        state.stacks.last_updated = now;
                        state.stacks.last_sync_time = now;
                        state.stacks.network = info.network;
                        state.stacks.mode = Some("nakamoto".to_string());
                        state.stacks.epoch = Some(info.epoch);
                        state.stacks.burn_block_height = Some(info.burn_block_height);
                    }

                    self.last_height = info.height;
                    if let Some(ref coord) = self.coordinator {
                        let _ = coord
                            .publish_state_root("stacks", &info.height.to_string())
                            .await;
                    }
                } else if info.height < self.last_height {
                    info!(
                        "Stacks tip moved backwards: height={} -> {}, network={}",
                        self.last_height, info.height, info.network
                    );

                    let height = info.height;
                    self.persistence
                        .transactional_update(4, move |state| {
                            state.stacks_height = height;
                            Ok(())
                        })
                        .await?;

                    {
                        let mut state = self.state.write().expect("lock poisoned");
                        state.stacks.height = info.height;
                        state.stacks.status = "synced".to_string();
                        state.stacks.last_updated = now;
                        state.stacks.last_sync_time = now;
                        state.stacks.network = info.network;
                        state.stacks.mode = Some("nakamoto".to_string());
                        state.stacks.epoch = Some(info.epoch);
                        state.stacks.burn_block_height = Some(info.burn_block_height);
                    }

                    self.last_height = height;
                    if let Some(ref coord) = self.coordinator {
                        let _ = coord
                            .publish_state_root("stacks", &height.to_string())
                            .await;
                    }
                } else {
                    let mut state = self.state.write().expect("lock poisoned");
                    state.stacks.last_sync_time = now;
                }
                Ok(())
            }
            Err(e) => {
                let mut state = self.state.write().expect("lock poisoned");
                state.stacks.status = format!("error: {}", e);
                Err(e)
            }
        }
    }

    pub async fn run(&mut self) -> ConxianResult<()> {
        info!(
            "Starting Stacks (Nakamoto) listener with sync interval {}s...",
            self.sync_interval
        );

        loop {
            if let Err(e) = self.sync_once().await {
                error!("Failed to sync Stacks: {}", e);
            }
            sleep(Duration::from_secs(self.sync_interval)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stacks::rpc::StacksNetworkInfo;
    use async_trait::async_trait;
    use conxian_core::{ConxianError, GatewayState, PersistentState, VersionedPersistentState};
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    };

    #[derive(Default)]
    struct RecordingPublisher(AtomicUsize);

    #[async_trait]
    impl StateRootPublisher for RecordingPublisher {
        async fn publish_state_root(&self, _chain: &str, _root: &str) -> ConxianResult<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct SimulatedStacksRpc {
        height: u64,
    }

    #[async_trait]
    impl conxian_core::SimulatedStacksRpcTrait for SimulatedStacksRpc {
        async fn call_read_only(
            &self,
            _contract: &str,
            _function: &str,
            _args: Vec<serde_json::Value>,
        ) -> ConxianResult<serde_json::Value> {
            Ok(serde_json::json!({ "okay": true }))
        }
    }

    #[async_trait]
    impl StacksRpc for SimulatedStacksRpc {
        async fn get_block_count(&self) -> ConxianResult<u64> {
            Ok(self.height)
        }
        async fn get_network_info(&self) -> ConxianResult<StacksNetworkInfo> {
            Ok(StacksNetworkInfo {
                height: self.height,
                network: "mainnet".to_string(),
                epoch: "3.0".to_string(),
                burn_block_height: self.height / 10,
            })
        }
    }

    struct SimulatedPersistence {
        state: Mutex<VersionedPersistentState>,
        conflict_once: AtomicBool,
        fail_cas: AtomicBool,
    }

    impl Default for SimulatedPersistence {
        fn default() -> Self {
            Self::with_state(PersistentState::default())
        }
    }

    impl SimulatedPersistence {
        fn with_state(state: PersistentState) -> Self {
            Self {
                state: Mutex::new(VersionedPersistentState { revision: 0, state }),
                conflict_once: AtomicBool::new(false),
                fail_cas: AtomicBool::new(false),
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
            if self.fail_cas.load(Ordering::SeqCst) {
                return Err(ConxianError::Persistence(
                    "injected Stacks checkpoint failure".to_string(),
                ));
            }
            let mut current = self.state.lock().expect("lock poisoned");
            if self.conflict_once.swap(false, Ordering::SeqCst) {
                let actual = current.revision + 1;
                current.revision = actual;
                current.state.bitcoin_height = 888;
                return Err(ConxianError::PersistenceConflict {
                    expected: expected_revision,
                    actual,
                });
            }
            if current.revision != expected_revision {
                return Err(ConxianError::PersistenceConflict {
                    expected: expected_revision,
                    actual: current.revision,
                });
            }
            current.revision += 1;
            current.state = state.clone();
            Ok(current.clone())
        }
    }

    #[tokio::test]
    async fn test_stacks_listener_sync_once() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let rpc = SimulatedStacksRpc { height: 555 };
        let persistence = Arc::new(SimulatedPersistence::default());
        let mut listener = StacksListener::new(rpc, state.clone(), persistence, None, 30)
            .await
            .unwrap();

        listener.sync_once().await.unwrap();

        {
            let s = state.read().expect("lock poisoned");
            assert_eq!(s.stacks.height, 555);
            assert_eq!(s.stacks.status, "synced");
            assert_eq!(s.stacks.mode.as_deref(), Some("nakamoto"));
            assert_eq!(s.stacks.burn_block_height, Some(55));
            assert!(s.stacks.last_sync_time > 0);
        }

        // Update height
        listener.rpc.height = 556;
        listener.sync_once().await.unwrap();

        {
            let s = state.read().expect("lock poisoned");
            assert_eq!(s.stacks.height, 556);
            assert_eq!(s.stacks.burn_block_height, Some(55)); // Simulated int div
        }
    }

    #[tokio::test]
    async fn stacks_listener_retries_conflict_and_preserves_unowned_fields() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let persistence = Arc::new(SimulatedPersistence::with_state(PersistentState {
            bitcoin_height: 42,
            stacks_height: 0,
            mempool_pending_txs: vec![conxian_core::TrackedMempoolTx {
                txid: "preserved".to_string(),
                ..Default::default()
            }],
        }));
        persistence.conflict_once.store(true, Ordering::SeqCst);
        let mut listener = StacksListener::new(
            SimulatedStacksRpc { height: 555 },
            state,
            persistence.clone(),
            None,
            30,
        )
        .await
        .unwrap();

        listener.sync_once().await.unwrap();
        let persisted = persistence.load().unwrap();
        assert_eq!(persisted.stacks_height, 555);
        assert_eq!(persisted.bitcoin_height, 888);
        assert_eq!(persisted.mempool_pending_txs[0].txid, "preserved");
    }

    #[tokio::test]
    async fn stacks_listener_does_not_advance_after_persistence_failure() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let persistence = Arc::new(SimulatedPersistence::default());
        persistence.fail_cas.store(true, Ordering::SeqCst);
        let publisher = Arc::new(RecordingPublisher::default());
        let mut listener = StacksListener {
            rpc: SimulatedStacksRpc { height: 555 },
            state: state.clone(),
            persistence: AsyncPersistence::new(persistence.clone()),
            coordinator: Some(publisher.clone()),
            last_height: 0,
            sync_interval: 30,
        };

        assert!(listener.sync_once().await.is_err());
        assert_eq!(listener.last_height, 0);
        assert_eq!(state.read().unwrap().stacks.height, 0);
        assert_eq!(persistence.load().unwrap().stacks_height, 0);
        assert_eq!(publisher.0.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn stacks_listener_persists_lower_tip_and_preserves_unowned_fields() {
        let mut gateway_state = GatewayState::default();
        gateway_state.stacks.height = 555;
        gateway_state.stacks.network = "mainnet".to_string();
        let state = Arc::new(RwLock::new(gateway_state));
        let persistence = Arc::new(SimulatedPersistence::with_state(PersistentState {
            bitcoin_height: 42,
            stacks_height: 555,
            mempool_pending_txs: vec![conxian_core::TrackedMempoolTx {
                txid: "preserved-lower-tip".to_string(),
                ..Default::default()
            }],
        }));
        let mut listener = StacksListener::new(
            SimulatedStacksRpc { height: 554 },
            state.clone(),
            persistence.clone(),
            None,
            30,
        )
        .await
        .unwrap();

        listener.sync_once().await.unwrap();

        let persisted = persistence.load().unwrap();
        assert_eq!(persisted.stacks_height, 554);
        assert_eq!(persisted.bitcoin_height, 42);
        assert_eq!(persisted.mempool_pending_txs[0].txid, "preserved-lower-tip");
        assert_eq!(listener.last_height, 554);
        let shared = state.read().unwrap();
        assert_eq!(shared.stacks.height, 554);
        assert_eq!(shared.stacks.burn_block_height, Some(55));
    }

    #[tokio::test]
    async fn stacks_listener_lower_tip_failure_leaves_all_heights_unchanged() {
        let mut gateway_state = GatewayState::default();
        gateway_state.stacks.height = 555;
        let state = Arc::new(RwLock::new(gateway_state));
        let persistence = Arc::new(SimulatedPersistence::with_state(PersistentState {
            bitcoin_height: 42,
            stacks_height: 555,
            ..PersistentState::default()
        }));
        persistence.fail_cas.store(true, Ordering::SeqCst);
        let mut listener = StacksListener::new(
            SimulatedStacksRpc { height: 554 },
            state.clone(),
            persistence.clone(),
            None,
            30,
        )
        .await
        .unwrap();

        assert!(listener.sync_once().await.is_err());
        assert_eq!(listener.last_height, 555);
        let persisted = persistence.load().unwrap();
        assert_eq!(persisted.stacks_height, 555);
        assert_eq!(persisted.bitcoin_height, 42);
        assert_eq!(state.read().unwrap().stacks.height, 555);
    }
}
