use crate::bitcoin::BitcoinRpcClient;
use conxian_core::ConxianResult;
use tokio::time::{sleep, Duration};
use tracing::{info, error};

pub struct BitcoinListener {
    rpc: BitcoinRpcClient,
    last_height: u64,
}

impl BitcoinListener {
    pub fn new(rpc: BitcoinRpcClient) -> Self {
        Self { rpc, last_height: 0 }
    }

    pub async fn run(&mut self) -> ConxianResult<()> {
        info!("Starting Bitcoin listener...");

        // Initial height
        self.last_height = self.rpc.get_block_count().await?;
        info!("Current Bitcoin height: {}", self.last_height);

        loop {
            match self.rpc.get_block_count().await {
                Ok(current_height) => {
                    if current_height > self.last_height {
                        for h in (self.last_height + 1)..=current_height {
                            match self.rpc.get_block_info(h).await {
                                Ok(block) => {
                                    info!("New Bitcoin block: {} ({})", block.height, block.hash);
                                    // Here we would trigger events/indexing
                                }
                                Err(e) => error!("Failed to get block info for height {}: {}", h, e),
                            }
                        }
                        self.last_height = current_height;
                    }
                }
                Err(e) => error!("Failed to get Bitcoin block count: {}", e),
            }
            sleep(Duration::from_secs(10)).await;
        }
    }
}
