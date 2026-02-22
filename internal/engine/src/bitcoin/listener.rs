use crate::bitcoin::BitcoinRpc;
use conxian_core::{ConxianResult, Persistence, PersistentState, SharedState};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct BitcoinListener<R: BitcoinRpc> {
    rpc: R,
    state: SharedState,
    persistence: Arc<dyn Persistence>,
    last_height: u64,
    network: Option<String>,
}

impl<R: BitcoinRpc> BitcoinListener<R> {
    pub fn new(rpc: R, state: SharedState, persistence: Arc<dyn Persistence>) -> Self {
        let last_height = persistence.load().map(|s| s.bitcoin_height).unwrap_or(0);
        Self {
            rpc,
            state,
            persistence,
            last_height,
            network: None,
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
                if current_height > self.last_height || self.last_height == 0 {
                    let start_h = if self.last_height == 0 {
                        current_height
                    } else {
                        self.last_height + 1
                    };
                    for h in start_h..=current_height {
                        match self.rpc.get_block_info(h).await {
                            Ok(block) => {
                                info!("New Bitcoin block: {} ({})", block.height, block.hash);
                                let mut state = self.state.write().unwrap();
                                state.bitcoin.height = block.height;
                                state.bitcoin.last_updated = block.timestamp;
                                state.bitcoin.status = "synced".to_string();
                                state.bitcoin.best_block_hash = block.hash.clone();
                                if let Some(ref n) = self.network {
                                    state.bitcoin.network = n.clone();
                                }

                                // Save persistence
                                let p_state = PersistentState {
                                    bitcoin_height: block.height,
                                    stacks_height: state.stacks.height,
                                };
                                let _ = self.persistence.save(&p_state);
                            }
                            Err(e) => {
                                error!("Failed to get block info for height {}: {}", h, e);
                                return Err(e);
                            }
                        }
                    }
                    self.last_height = current_height;
                }
                Ok(())
            }
            Err(e) => {
                error!("Failed to get Bitcoin block count: {}", e);
                let mut state = self.state.write().unwrap();
                state.bitcoin.status = format!("error: {}", e);
                Err(e)
            }
        }
    }

    pub async fn run(&mut self) -> ConxianResult<()> {
        info!("Starting Bitcoin listener...");

        loop {
            if let Err(e) = self.sync_once().await {
                error!("Failed to sync Bitcoin: {}", e);
            }
            sleep(Duration::from_secs(10)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use conxian_core::{BlockInfo, GatewayState};
    use std::sync::{Arc, RwLock};

    struct MockBitcoinRpc {
        height: u64,
    }

    #[async_trait]
    impl BitcoinRpc for MockBitcoinRpc {
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
            Ok("testnet".to_string())
        }
    }

    struct MockPersistence;
    impl Persistence for MockPersistence {
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
        let rpc = MockBitcoinRpc { height: 100 };
        let persistence = Arc::new(MockPersistence);
        let mut listener = BitcoinListener::new(rpc, state.clone(), persistence);

        listener.sync_once().await.unwrap();

        {
            let s = state.read().unwrap();
            assert_eq!(s.bitcoin.height, 100);
            assert_eq!(s.bitcoin.status, "synced");
            assert_eq!(s.bitcoin.network, "testnet");
            assert_eq!(s.bitcoin.best_block_hash, "hash-100");
        }

        // Update height
        listener.rpc.height = 101;
        listener.sync_once().await.unwrap();

        {
            let s = state.read().unwrap();
            assert_eq!(s.bitcoin.height, 101);
            assert_eq!(s.bitcoin.best_block_hash, "hash-101");
        }
    }
}
