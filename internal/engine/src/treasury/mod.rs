use crate::stacks::alex::AlexClient;
use conxian_core::{AlexSwapRequest, ConxianResult, SharedState};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

pub struct TreasuryMonitor {
    state: SharedState,
    interval_secs: u64,
    alex: Arc<dyn AlexClient>,
}

impl TreasuryMonitor {
    pub fn new(state: SharedState, interval_secs: u64, alex: Arc<dyn AlexClient>) -> Self {
        Self {
            state,
            interval_secs,
            alex,
        }
    }

    pub async fn run(&self) -> ConxianResult<()> {
        info!("Starting Treasury Monitor with ALEX-driven Sovereign Yield Index (SYI)...");

        loop {
            if let Err(e) = self.update_balances().await {
                error!("Treasury monitor error: {}", e);
            }

            sleep(Duration::from_secs(self.interval_secs)).await;
        }
    }

    pub async fn update_balances(&self) -> ConxianResult<()> {
        // Fetch real-time market data from ALEX to anchor SYI
        let quote_req = AlexSwapRequest {
            token_x: "sBTC".to_string(),
            token_y: "STX".to_string(),
            factor: 100_000_000,
            amount: 100_000_000, // 1 sBTC
            min_dy: None,
        };

        let market_yield_proxy = match self.alex.get_swap_quote(quote_req).await {
            Ok(quote) => {
                info!("ALEX Market Quote (1 sBTC -> STX): {}", quote);
                // Use quote volatility or depth as a proxy for opportunity cost in SYI
                (quote as f64 / 1_000_000.0).min(1.0)
            }
            Err(e) => {
                warn!(
                    "Failed to fetch ALEX quote for SYI: {}, falling back to simulation",
                    e
                );
                0.5 // Fallback proxy
            }
        };

        let mut s = self.state.write().expect("lock poisoned");

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

        // SYI calculation: Anchored in ALEX market depth + Sovereign multiplier
        let sovereignty_multiplier = 1.2; // Reward for non-custodial paths
        s.metrics.syi_index = (0.04 + (market_yield_proxy * 0.05)) * sovereignty_multiplier;

        // Simulate growth towards TAM ($10B+)
        let growth_factor = if s.metrics.sbtc_liquidity > 1_000_000_000.0 {
            0.00005
        } else {
            0.0002
        };
        s.metrics.sbtc_liquidity += s.metrics.sbtc_liquidity * growth_factor;

        // CON-452: Structured Finance Yield Distribution
        let base_yield_stx = 1250.0 * (1.0 + market_yield_proxy);
        let senior_share = 0.6; // 60% fixed to senior
        let junior_share = 0.4; // 40% to junior

        let senior_yield = base_yield_stx * senior_share;
        let junior_yield = base_yield_stx * junior_share;

        // Simulate tax on total yield
        let tax_stx = base_yield_stx * 0.01;

        s.metrics.treasury_balance_stx += base_yield_stx - tax_stx;
        s.metrics.last_treasury_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock moved backwards")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stacks::alex::SimulatedAlexClient;
    use conxian_core::GatewayState;
    use std::sync::RwLock;

    #[tokio::test]
    async fn test_treasury_monitor_syi_calculation() {
        let state: SharedState = Arc::new(RwLock::new(GatewayState::new()));
        let alex = Arc::new(SimulatedAlexClient);
        let monitor = TreasuryMonitor::new(state.clone(), 1, alex);

        monitor.update_balances().await.unwrap();

        let s = state.read().expect("lock poisoned");
        assert!(s.metrics.syi_index > 0.0);
        assert!(s.metrics.treasury_balance_stx > 1000000.0);
        assert_eq!(s.metrics.treasury_balance_btc, 1050000000);
    }
}
