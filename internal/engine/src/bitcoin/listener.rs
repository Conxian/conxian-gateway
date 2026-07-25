use crate::bitcoin::BitcoinRpc;
use crate::coordination::{RedisCoordinator, StateRootPublisher};
use crate::persistence::AsyncPersistence;
use conxian_core::{ConxianResult, Persistence, SharedState};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct BitcoinListener<R: BitcoinRpc> {
    rpc: R,
    state: SharedState,
    persistence: AsyncPersistence,
    coordinator: Option<Arc<dyn StateRootPublisher>>,
    last_height: u64,
    network: Option<String>,
    sync_interval: u64,
}

impl<R: BitcoinRpc> BitcoinListener<R> {
    pub async fn new(
        rpc: R,
        state: SharedState,
        persistence: Arc<dyn Persistence>,
        coordinator: Option<Arc<RedisCoordinator>>,
        sync_interval: u64,
    ) -> ConxianResult<Self> {
        let persistence = AsyncPersistence::new(persistence);
        let last_height = persistence.load().await?.bitcoin_height;
        let coordinator = coordinator.map(|value| value as Arc<dyn StateRootPublisher>);
        Ok(Self {
            rpc,
            state,
            persistence,
            coordinator,
            last_height,
            network: None,
            sync_interval,
        })
    }

    pub async fn sync_once(&mut self) -> ConxianResult<()> {
        if self.network.is_none() {
            match self.rpc.get_network_info().await {
                Ok(n) => self.network = Some(n),
                Err(e) => error!("Failed to get Bitcoin network info: {}", e),
            }
        }

        match self.rpc.get_block_count().await {
            Ok(current_height) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock moved backwards")
                    .as_secs();

                if current_height > self.last_height || self.last_height == 0 {
                    let start_h = if self.last_height == 0 {
                        current_height
                    } else {
                        self.last_height + 1
                    };
                    for h in start_h..=current_height {
                        match self.rpc.get_block_info(h).await {
                            Ok(block) => {
                                info!(
                                    "New Bitcoin block processed: height={}, hash={}, network={:?}",
                                    block.height, block.hash, self.network
                                );

                                let height = block.height;
                                self.persistence
                                    .transactional_update(4, move |state| {
                                        state.bitcoin_height = height;
                                        Ok(())
                                    })
                                    .await?;
                                self.apply_block(&block, now);
                                self.last_height = block.height;
                                if let Some(ref coord) = self.coordinator {
                                    let _ = coord.publish_state_root("bitcoin", &block.hash).await;
                                }
                            }
                            Err(e) => {
                                error!("Failed to get block info for height {}: {}", h, e);
                                return Err(e);
                            }
                        }
                    }
                    self.last_height = current_height;
                } else if current_height == self.last_height {
                    match self.rpc.get_block_info(current_height).await {
                        Ok(block) => {
                            let (changed, best_hash) = {
                                let state = self.state.read().expect("lock poisoned");
                                (
                                    state.bitcoin.best_block_hash != block.hash,
                                    state.bitcoin.best_block_hash.clone(),
                                )
                            };

                            if changed {
                                info!(
                                    "Bitcoin tip change detected at height {}: {} -> {}",
                                    block.height, best_hash, block.hash
                                );

                                let height = block.height;
                                self.persistence
                                    .transactional_update(4, move |state| {
                                        state.bitcoin_height = height;
                                        Ok(())
                                    })
                                    .await?;
                                self.apply_block(&block, now);
                                if let Some(ref coord) = self.coordinator {
                                    let _ = coord.publish_state_root("bitcoin", &block.hash).await;
                                }
                            } else {
                                let mut state = self.state.write().expect("lock poisoned");
                                state.bitcoin.last_sync_time = now;
                            }
                        }
                        Err(e) => {
                            error!(
                                "Failed to refresh Bitcoin tip at height {}: {}",
                                current_height, e
                            );
                            return Err(e);
                        }
                    }
                } else {
                    let mut state = self.state.write().expect("lock poisoned");
                    state.bitcoin.last_sync_time = now;
                }
                Ok(())
            }
            Err(e) => {
                error!("Failed to get Bitcoin block count: {}", e);
                let mut state = self.state.write().expect("lock poisoned");
                state.bitcoin.status = format!("error: {}", e);
                Err(e)
            }
        }
    }

    pub async fn run(&mut self) -> ConxianResult<()> {
        info!(
            "Starting Bitcoin listener with sync interval {}s...",
            self.sync_interval
        );

        loop {
            if let Err(e) = self.sync_once().await {
                error!("Failed to sync Bitcoin: {}", e);
            }
            sleep(Duration::from_secs(self.sync_interval)).await;
        }
    }

    fn apply_block(&self, block: &conxian_core::BlockInfo, now: u64) {
        let mut state = self.state.write().expect("lock poisoned");
        state.bitcoin.height = block.height;
        state.bitcoin.last_updated = block.timestamp;
        state.bitcoin.last_sync_time = now;
        state.bitcoin.status = "synced".to_string();
        state.bitcoin.best_block_hash = block.hash.clone();
        if let Some(ref network) = self.network {
            state.bitcoin.network = network.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use conxian_core::{
        BlockInfo, ConxianError, GatewayState, PersistentState, VersionedPersistentState,
    };
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

    struct SimulatedBitcoinRpc {
        height: u64,
    }

    #[async_trait]
    impl BitcoinRpc for SimulatedBitcoinRpc {
        async fn get_block_count(&self) -> ConxianResult<u64> {
            Ok(self.height)
        }
        async fn get_block_info(&self, height: u64) -> ConxianResult<BlockInfo> {
            Ok(BlockInfo {
                hash: format!("hash-{}", height),
                height,
                timestamp: 123456789,
            })
        }
        async fn get_network_info(&self) -> ConxianResult<String> {
            Ok("mainnet".to_string())
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
                    "injected Bitcoin checkpoint failure".to_string(),
                ));
            }
            let mut current = self.state.lock().expect("lock poisoned");
            if self.conflict_once.swap(false, Ordering::SeqCst) {
                let actual = current.revision + 1;
                current.revision = actual;
                current.state.stacks_height = 999;
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
    async fn test_bitcoin_listener_sync_once() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let rpc = SimulatedBitcoinRpc { height: 100 };
        let persistence = Arc::new(SimulatedPersistence::default());
        let mut listener = BitcoinListener::new(rpc, state.clone(), persistence, None, 10)
            .await
            .unwrap();

        listener.sync_once().await.unwrap();

        {
            let s = state.read().expect("lock poisoned");
            assert_eq!(s.bitcoin.height, 100);
            assert_eq!(s.bitcoin.status, "synced");
            assert_eq!(s.bitcoin.network, "mainnet");
            assert_eq!(s.bitcoin.best_block_hash, "hash-100");
            assert!(s.bitcoin.last_sync_time > 0);
        }

        // Update height
        listener.rpc.height = 101;
        listener.sync_once().await.unwrap();

        {
            let s = state.read().expect("lock poisoned");
            assert_eq!(s.bitcoin.height, 101);
            assert_eq!(s.bitcoin.best_block_hash, "hash-101");
        }
    }

    #[tokio::test]
    async fn bitcoin_listener_retries_conflict_and_preserves_unowned_fields() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let persistence = Arc::new(SimulatedPersistence::with_state(PersistentState {
            bitcoin_height: 0,
            stacks_height: 42,
            mempool_pending_txs: vec![conxian_core::TrackedMempoolTx {
                txid: "preserved".to_string(),
                ..Default::default()
            }],
        }));
        persistence.conflict_once.store(true, Ordering::SeqCst);
        let mut listener = BitcoinListener::new(
            SimulatedBitcoinRpc { height: 100 },
            state,
            persistence.clone(),
            None,
            10,
        )
        .await
        .unwrap();

        listener.sync_once().await.unwrap();
        let persisted = persistence.load().unwrap();
        assert_eq!(persisted.bitcoin_height, 100);
        assert_eq!(persisted.stacks_height, 999);
        assert_eq!(persisted.mempool_pending_txs[0].txid, "preserved");
    }

    #[tokio::test]
    async fn bitcoin_listener_does_not_advance_after_persistence_failure() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let persistence = Arc::new(SimulatedPersistence::default());
        persistence.fail_cas.store(true, Ordering::SeqCst);
        let publisher = Arc::new(RecordingPublisher::default());
        let mut listener = BitcoinListener {
            rpc: SimulatedBitcoinRpc { height: 100 },
            state: state.clone(),
            persistence: AsyncPersistence::new(persistence.clone()),
            coordinator: Some(publisher.clone()),
            last_height: 0,
            network: None,
            sync_interval: 10,
        };

        assert!(listener.sync_once().await.is_err());
        assert_eq!(listener.last_height, 0);
        assert_eq!(state.read().unwrap().bitcoin.height, 0);
        assert_eq!(persistence.load().unwrap().bitcoin_height, 0);
        assert_eq!(publisher.0.load(Ordering::SeqCst), 0);
    }
}
