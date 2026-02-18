use conxian_core::{ConxianResult, SharedState};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::info;

pub struct StacksListener {
    state: SharedState,
    last_height: u64,
}

impl StacksListener {
    pub fn new(state: SharedState) -> Self {
        Self {
            state,
            last_height: 0,
        }
    }

    pub async fn run(&mut self) -> ConxianResult<()> {
        info!("Starting Stacks (Nakamoto) listener...");

        // Initial height (simulated)
        self.last_height = 100000;
        {
            let mut state = self.state.write().unwrap();
            state.stacks.height = self.last_height;
            state.stacks.status = "synced".to_string();
            state.stacks.last_updated = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
        info!("Current Stacks height: {}", self.last_height);

        loop {
            // Simulated block ingestion
            let current_height = self.last_height + 1;

            info!("New Stacks block: {}", current_height);
            {
                let mut state = self.state.write().unwrap();
                state.stacks.height = current_height;
                state.stacks.last_updated = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
            }
            self.last_height = current_height;

            sleep(Duration::from_secs(30)).await;
        }
    }
}
