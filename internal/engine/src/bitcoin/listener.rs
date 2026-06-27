use crate::bitcoin::BitcoinRpc;
use conxian_core::{ConxianResult, Persistence, SharedState};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct BitcoinListener<R: BitcoinRpc> {
    rpc: R,
    state: SharedState,
    persistence: Arc<dyn Persistence>,
    last_height: u64,
    network: Option<String>,
    sync_interval: u64,
}

impl<R: BitcoinRpc> BitcoinListener<R> {
    pub fn new(
        rpc: R,
        state: SharedState,
        persistence: Arc<dyn Persistence>,
        sync_interval: u64,
    ) -> Self {
        let last_height = persistence.load().map(|s| s.bitcoin_height).unwrap_or(0);
        Self {
            rpc,
            state,
            persistence,
            last_height,
            network: None,
            sync_interval,
        }
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
                                let mut state = self.state.write().expect("lock poisoned");
                                state.bitcoin.height = block.height;
                                state.bitcoin.last_updated = block.timestamp;
                                state.bitcoin.last_sync_time = now;
                                state.bitcoin.status = "synced".to_string();
                                state.bitcoin.best_block_hash = block.hash.clone();
                                if let Some(ref n) = self.network {
                                    state.bitcoin.network = n.clone();
                                }

                                // Save persistence
                                let mut p_state = self.persistence.load().unwrap_or_default();
                                p_state.bitcoin_height = block.height;
                                p_state.stacks_height = state.stacks.height;
                                let _ = self.persistence.save(&p_state);
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
                            let mut state = self.state.write().expect("lock poisoned");
                            if state.bitcoin.best_block_hash != block.hash {
                                info!(
                                    "Bitcoin tip change detected at height {}: {} -> {}",
                                    block.height, state.bitcoin.best_block_hash, block.hash
                                );
                                state.bitcoin.height = block.height;
                                state.bitcoin.last_updated = block.timestamp;
                                state.bitcoin.last_sync_time = now;
                                state.bitcoin.status = "synced".to_string();
                                state.bitcoin.best_block_hash = block.hash;
                                if let Some(ref n) = self.network {
                                    state.bitcoin.network = n.clone();
                                }

                                let mut p_state = self.persistence.load().unwrap_or_default();
                                p_state.bitcoin_height = state.bitcoin.height;
                                p_state.stacks_height = state.stacks.height;
                                let _ = self.persistence.save(&p_state);
                            } else {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use conxian_core::{BlockInfo, GatewayState, PersistentState};
    use std::sync::{Arc, RwLock};

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

    struct SimulatedPersistence;
    impl Persistence for SimulatedPersistence {
        fn save(&self, _state: &PersistentState) -> ConxianResult<()> {
            Ok(())
        }
        fn load(&self) -> ConxianResult<PersistentState> {
            Ok(PersistentState::default())
        }
    }

    #[tokio::test]
    async fn test_bitcoin_listener_sync_once() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let rpc = SimulatedBitcoinRpc { height: 100 };
        let persistence = Arc::new(SimulatedPersistence);
        let mut listener = BitcoinListener::new(rpc, state.clone(), persistence, 10);

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
}
