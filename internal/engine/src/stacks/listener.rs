use crate::stacks::StacksRpc;
use conxian_core::{ConxianResult, Persistence, PersistentState, SharedState};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct StacksListener<R: StacksRpc> {
    rpc: R,
    state: SharedState,
    persistence: Arc<dyn Persistence>,
    last_height: u64,
}

impl<R: StacksRpc> StacksListener<R> {
    pub fn new(rpc: R, state: SharedState, persistence: Arc<dyn Persistence>) -> Self {
        let last_height = persistence.load().map(|s| s.stacks_height).unwrap_or(0);
        Self {
            rpc,
            state,
            persistence,
            last_height,
        }
    }

    pub async fn sync_once(&mut self) -> ConxianResult<()> {
        match self.rpc.get_network_info().await {
            Ok(info) => {
                if info.height > self.last_height || self.last_height == 0 {
                    let mut state = self.state.write().unwrap();
                    state.stacks.height = info.height;
                    state.stacks.status = "synced".to_string();
                    state.stacks.last_updated = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    state.stacks.network = info.network;
                    state.stacks.mode = Some("nakamoto".to_string());
                    state.stacks.epoch = Some(info.epoch);

                    // Save persistence
                    let p_state = PersistentState {
                        bitcoin_height: state.bitcoin.height,
                        stacks_height: info.height,
                    };
                    let _ = self.persistence.save(&p_state);

                    self.last_height = info.height;
                }
                Ok(())
            }
            Err(e) => {
                let mut state = self.state.write().unwrap();
                state.stacks.status = format!("error: {}", e);
                Err(e)
            }
        }
    }

    pub async fn run(&mut self) -> ConxianResult<()> {
        info!("Starting Stacks (Nakamoto) listener...");

        loop {
            if let Err(e) = self.sync_once().await {
                error!("Failed to sync Stacks: {}", e);
            }
            sleep(Duration::from_secs(30)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stacks::rpc::StacksNetworkInfo;
    use async_trait::async_trait;
    use conxian_core::GatewayState;
    use std::sync::{Arc, RwLock};

    struct MockStacksRpc {
        height: u64,
    }

    #[async_trait]
    impl StacksRpc for MockStacksRpc {
        async fn get_block_count(&self) -> ConxianResult<u64> {
            Ok(self.height)
        }
        async fn get_network_info(&self) -> ConxianResult<StacksNetworkInfo> {
            Ok(StacksNetworkInfo {
                height: self.height,
                network: "mainnet".to_string(),
                epoch: "3.0".to_string(),
            })
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
    async fn test_stacks_listener_sync_once() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let rpc = MockStacksRpc { height: 555 };
        let persistence = Arc::new(MockPersistence);
        let mut listener = StacksListener::new(rpc, state.clone(), persistence);

        listener.sync_once().await.unwrap();

        {
            let s = state.read().unwrap();
            assert_eq!(s.stacks.height, 555);
            assert_eq!(s.stacks.status, "synced");
            assert_eq!(s.stacks.mode.as_deref(), Some("nakamoto"));
        }

        // Update height
        listener.rpc.height = 556;
        listener.sync_once().await.unwrap();

        {
            let s = state.read().unwrap();
            assert_eq!(s.stacks.height, 556);
        }
    }
}
