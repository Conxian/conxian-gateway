use conxian_core::ConxianResult;
use tokio::time::{sleep, Duration};
use tracing::info;

pub struct StacksListener {
    last_height: u64,
}

impl StacksListener {
    pub fn new() -> Self {
        Self { last_height: 0 }
    }

    pub async fn run(&mut self) -> ConxianResult<()> {
        info!("Starting Stacks (Nakamoto) listener...");

        // Initial height (simulated)
        self.last_height = 100000;
        info!("Current Stacks height: {}", self.last_height);

        loop {
            // Simulated block ingestion
            // In a real implementation, this would call Stacks RPC
            let current_height = self.last_height + 1; // Simulate a new block

            info!("New Stacks block: {}", current_height);
            self.last_height = current_height;

            sleep(Duration::from_secs(30)).await;
        }
    }
}
