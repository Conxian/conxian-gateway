use crate::stacks::StacksRpc;
use conxian_core::{ConxianResult, SharedState};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct StacksListener<R: StacksRpc> {
    rpc: R,
    state: SharedState,
    last_height: u64,
}

impl<R: StacksRpc> StacksListener<R> {
    pub fn new(rpc: R, state: SharedState) -> Self {
        Self {
            rpc,
            state,
            last_height: 0,
        }
    }

    pub async fn sync_once(&mut self) -> ConxianResult<()> {
        match self.rpc.get_block_count().await {
            Ok(current_height) => {
                if current_height > self.last_height || self.last_height == 0 {
                    let mut state = self.state.write().unwrap();
                    state.stacks.height = current_height;
                    state.stacks.status = "synced".to_string();
                    state.stacks.last_updated = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    state.stacks.network = "mainnet".to_string();
                    // Research enhancement: Nakamoto-ready signaling
                    state.stacks.mode = Some("nakamoto".to_string());
                    state.stacks.epoch = Some("3.0".to_string());

                    self.last_height = current_height;
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
    use conxian_core::GatewayState;
    use std::sync::{Arc, RwLock};
    use async_trait::async_trait;

    struct MockStacksRpc {
        height: u64,
    }

    #[async_trait]
    impl StacksRpc for MockStacksRpc {
        async fn get_block_count(&self) -> ConxianResult<u64> {
            Ok(self.height)
        }
    }

    #[tokio::test]
    async fn test_stacks_listener_sync_once() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let rpc = MockStacksRpc { height: 555 };
        let mut listener = StacksListener::new(rpc, state.clone());

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
