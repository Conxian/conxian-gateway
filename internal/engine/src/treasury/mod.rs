use conxian_core::{ConxianResult, SharedState};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

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
        info!("Starting Treasury Monitor with TAM-Capture Enhancements...");

        loop {
            if let Err(e) = self.update_balances().await {
                error!("Treasury monitor error: {}", e);
            }

            sleep(Duration::from_secs(self.interval_secs)).await;
        }
    }

    async fn update_balances(&self) -> ConxianResult<()> {
        let mut s = self.state.write().unwrap();

        // Initial setup for institutional balances
        if s.metrics.treasury_balance_stx == 0.0 {
            s.metrics.treasury_balance_stx = 1_000_000.0;
        }
        if s.metrics.treasury_balance_btc == 0.0 {
            s.metrics.treasury_balance_btc = 10.5;
        }

        // Industry Enhancement: The sBTC "Suction" Pattern
        // Incentivize native BTC-to-sBTC migrations via the Sovereign Yield Index (SYI).
        if s.metrics.sbtc_liquidity == 0.0 {
            s.metrics.sbtc_liquidity = 5_000_000.0; // Starting at $5M SAM
        }
        if s.metrics.syi_index == 0.0 {
            s.metrics.syi_index = 0.0525; // 5.25% Initial SYI
        }

        // Simulate growth towards TAM ($10B+)
        let growth_factor = if s.metrics.sbtc_liquidity > 1_000_000_000.0 {
            0.00005
        } else {
            0.0002
        }; // Small incremental growth per cycle
        s.metrics.sbtc_liquidity += s.metrics.sbtc_liquidity * growth_factor;

        // SYI oscillates based on sovereign alignment and liquidity depth
        s.metrics.syi_index =
            (0.05 + (s.metrics.sbtc_liquidity / 10_000_000_000.0) * 0.02).min(0.12);

        // Institutional yield extraction (1% Sovereign Tax - CON-55)
        let yield_stx = 1250.0;
        let tax_stx = yield_stx * 0.01;

        s.metrics.treasury_balance_stx += yield_stx - tax_stx;
        s.metrics.last_treasury_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        info!(
            "TAM Capture Update: sBTC Liquidity: ${:.2}, SYI: {:.4}%",
            s.metrics.sbtc_liquidity,
            s.metrics.syi_index * 100.0
        );

        Ok(())
    }
}
