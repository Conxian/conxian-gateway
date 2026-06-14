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
        info!("Starting Treasury Monitor with Structured Finance & TAM-Capture...");

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
        if s.metrics.treasury_balance_btc == 0 {
            s.metrics.treasury_balance_btc = 1_050_000_000; // 10.5 BTC in satoshis
        }

        // Industry Enhancement: The sBTC "Suction" Pattern
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
        };
        s.metrics.sbtc_liquidity += s.metrics.sbtc_liquidity * growth_factor;

        // SYI oscillates based on sovereign alignment and liquidity depth
        s.metrics.syi_index =
            (0.05 + (s.metrics.sbtc_liquidity / 10_000_000_000.0) * 0.02).min(0.12);

        // CON-452: Structured Finance Yield Distribution
        // Senior tranches get priority on base yield; Junior tranches capture excess/risk yield.
        let base_yield_stx = 1250.0;
        let senior_share = 0.6; // 60% fixed to senior
        let junior_share = 0.4; // 40% to junior

        let senior_yield = base_yield_stx * senior_share;
        let junior_yield = base_yield_stx * junior_share;

        // Simulate tax on total yield
        let tax_stx = base_yield_stx * 0.01;

        s.metrics.treasury_balance_stx += base_yield_stx - tax_stx;
        s.metrics.last_treasury_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        info!(
            "TAM Capture Update: sBTC Liquidity: ${:.2}, SYI: {:.4}%. Tranche Yields: Senior ${:.2}, Junior ${:.2}",
            s.metrics.sbtc_liquidity,
            s.metrics.syi_index * 100.0,
            senior_yield,
            junior_yield
        );

        Ok(())
    }
}
