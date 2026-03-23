use conxian_core::{SharedState, ConxianResult};
use tokio::time::{sleep, Duration};
use tracing::{info, error};

pub struct TreasuryMonitor {
    state: SharedState,
    interval_secs: u64,
}

impl TreasuryMonitor {
    pub fn new(state: SharedState, interval_secs: u64) -> Self {
        Self {
            state,
            interval_secs,
        }
    }

    pub async fn run(&self) -> ConxianResult<()> {
        info!("Starting Treasury Monitor...");

        loop {
            if let Err(e) = self.update_balances().await {
                error!("Treasury monitor error: {}", e);
            }

            sleep(Duration::from_secs(self.interval_secs)).await;
        }
    }

    async fn update_balances(&self) -> ConxianResult<()> {
        // Research enhancement: Implement real balance fetching via Hiro API in the future.
        // For now, we simulate the institutional yield extraction (1% Sovereign Tax).

        let mut s = self.state.write().unwrap();

        // Mocking some initial balances if zero
        if s.metrics.treasury_balance_stx == 0.0 {
            s.metrics.treasury_balance_stx = 1000000.0; // 1M STX
        }
        if s.metrics.treasury_balance_btc == 0.0 {
            s.metrics.treasury_balance_btc = 10.5; // 10.5 BTC
        }

        // Simulate a small yield increase and tax extraction
        let yield_stx = 1250.0;
        let tax_stx = yield_stx * 0.01; // 1% Sovereign Tax (CON-55)

        s.metrics.treasury_balance_stx += yield_stx - tax_stx;
        s.metrics.last_treasury_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        info!("Treasury balances updated. Current STX: {}, BTC: {}", s.metrics.treasury_balance_stx, s.metrics.treasury_balance_btc);

        Ok(())
    }
}
