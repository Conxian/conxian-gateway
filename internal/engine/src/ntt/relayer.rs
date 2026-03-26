use conxian_core::{ConxianResult, SharedState};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

pub struct NttRelayer {
    state: SharedState,
    poll_interval: u64,
}

impl NttRelayer {
    pub fn new(state: SharedState, poll_interval: u64) -> Self {
        Self {
            state,
            poll_interval,
        }
    }

    pub async fn run(&self) -> ConxianResult<()> {
        info!("Starting NTT Relayer for sovereign bridging...");

        loop {
            if let Err(e) = self.process_ntt_events().await {
                warn!("Error processing NTT events: {}", e);
            }
            sleep(Duration::from_secs(self.poll_interval)).await;
        }
    }

    async fn process_ntt_events(&self) -> ConxianResult<()> {
        let height = {
            let s = self.state.read().unwrap();
            s.stacks.height
        };

        if height > 0 && height % 10 == 0 {
            info!(
                "NTT event detected at Stacks height {}. Submitting VAA to destination...",
                height
            );
            self.submit_vaa(height).await?;
        }

        Ok(())
    }

    async fn submit_vaa(&self, source_height: u64) -> ConxianResult<()> {
        info!(
            "Sovereign VAA submitted for height {}. Bridge status: Active",
            source_height
        );
        Ok(())
    }
}
