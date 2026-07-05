use crate::coordination::RedisCoordinator;
use crate::stacks::rpc::StacksRpc;
use conxian_core::{ConxianResult, Persistence, SharedState};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct StacksListener<R: StacksRpc> {
    rpc: R,
    state: SharedState,
    persistence: Arc<dyn Persistence>,
    coordinator: Option<Arc<RedisCoordinator>>,
    last_height: u64,
    sync_interval: u64,
}

impl<R: StacksRpc> StacksListener<R> {
    pub fn new(
        rpc: R,
        state: SharedState,
        persistence: Arc<dyn Persistence>,
        coordinator: Option<Arc<RedisCoordinator>>,
        sync_interval: u64,
    ) -> Self {
        let last_height = persistence.load().map(|s| s.stacks_height).unwrap_or(0);
        Self {
            rpc,
            state,
            persistence,
            coordinator,
            last_height,
            sync_interval,
        }
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

                    // Publish to Redis for cross-gateway coordination
                    if let Some(ref coord) = self.coordinator {
                        let _ = coord
                            .publish_state_root("stacks", &info.height.to_string())
                            .await;
                    }

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

                    // Save persistence
                    let mut p_state = self.persistence.load().unwrap_or_default();
                    {
                        let state = self.state.read().expect("lock poisoned");
                        p_state.bitcoin_height = state.bitcoin.height;
                        p_state.stacks_height = info.height;
                    }
                    let _ = self.persistence.save(&p_state);

                    self.last_height = info.height;
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
    use conxian_core::{GatewayState, PersistentState};
    use std::sync::{Arc, RwLock};

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
    async fn test_stacks_listener_sync_once() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let rpc = SimulatedStacksRpc { height: 555 };
        let persistence = Arc::new(SimulatedPersistence);
        let mut listener = StacksListener::new(rpc, state.clone(), persistence, None, 30);

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
}
