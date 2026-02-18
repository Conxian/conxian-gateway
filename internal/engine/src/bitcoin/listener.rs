use crate::bitcoin::BitcoinRpcClient;
use conxian_core::{ConxianResult, SharedState};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct BitcoinListener {
    rpc: BitcoinRpcClient,
    state: SharedState,
    last_height: u64,
}

impl BitcoinListener {
    pub fn new(rpc: BitcoinRpcClient, state: SharedState) -> Self {
        Self {
            rpc,
            state,
            last_height: 0,
        }
    }

    pub async fn run(&mut self) -> ConxianResult<()> {
        info!("Starting Bitcoin listener...");

        // Initial height
        self.last_height = self.rpc.get_block_count().await?;
        {
            let mut state = self.state.write().unwrap();
            state.bitcoin.height = self.last_height;
            state.bitcoin.status = "synced".to_string();
            state.bitcoin.last_updated = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
        info!("Current Bitcoin height: {}", self.last_height);

        loop {
            match self.rpc.get_block_count().await {
                Ok(current_height) => {
                    if current_height > self.last_height {
                        for h in (self.last_height + 1)..=current_height {
                            match self.rpc.get_block_info(h).await {
                                Ok(block) => {
                                    info!("New Bitcoin block: {} ({})", block.height, block.hash);
                                    let mut state = self.state.write().unwrap();
                                    state.bitcoin.height = block.height;
                                    state.bitcoin.last_updated = block.timestamp;
                                }
                                Err(e) => {
                                    error!("Failed to get block info for height {}: {}", h, e)
                                }
                            }
                        }
                        self.last_height = current_height;
                    }
                }
                Err(e) => {
                    error!("Failed to get Bitcoin block count: {}", e);
                    let mut state = self.state.write().unwrap();
                    state.bitcoin.status = format!("error: {}", e);
                }
            }
            sleep(Duration::from_secs(10)).await;
        }
    }
}
