use crate::shutdown::sleep_or_shutdown;
use crate::stacks::alex::AlexClient;
use conxian_core::{ConxianError, ConxianResult, SharedState};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::Duration;
use tracing::warn;

/// Treasury publication is intentionally disabled until its provider and
/// accounting evidence contract is qualified. The constructor is retained so
/// the existing task graph remains stable without writing synthetic values.
pub struct TreasuryMonitor {
    _state: SharedState,
    interval_secs: u64,
    _alex: Arc<dyn AlexClient>,
}

impl TreasuryMonitor {
    pub fn new(state: SharedState, interval_secs: u64, alex: Arc<dyn AlexClient>) -> Self {
        Self {
            _state: state,
            interval_secs,
            _alex: alex,
        }
    }

    /// Keep the task lifecycle and shutdown behavior, but do not poll an
    /// unverified quote or mutate live Gateway state.
    pub async fn run_until_shutdown(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> ConxianResult<()> {
        warn!(
            "Treasury monitor disabled: provider-backed balance, yield, and FX evidence is not qualified (#880)"
        );

        loop {
            if sleep_or_shutdown(&mut shutdown, Duration::from_secs(self.interval_secs)).await {
                return Ok(());
            }
        }
    }

    /// Explicit update requests fail closed. No fixture, default, quote proxy,
    /// or simulated growth may enter live Gateway state.
    pub async fn update_balances(&self) -> ConxianResult<()> {
        Err(ConxianError::Internal(
            "treasury values are unavailable until provider, network, asset, unit, observation-time, freshness, and provenance evidence is qualified (#880)".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stacks::alex::SimulatedAlexClient;
    use conxian_core::GatewayState;
    use std::sync::RwLock;

    #[tokio::test]
    async fn unqualified_treasury_update_fails_without_mutating_state() {
        let mut initial = GatewayState::new();
        initial.metrics.treasury_balance_stx = 1_000_000.0;
        initial.metrics.treasury_balance_btc = 1_050_000_000;
        initial.metrics.sbtc_liquidity = 5_000_000.0;
        initial.metrics.syi_index = 0.078;
        initial.metrics.last_treasury_update = 1_700_000_000;
        initial.metrics.fx_rmb_usd = 0.1425;
        initial.metrics.fx_rub_usd = 0.0115;
        initial.metrics.fx_inr_usd = 0.0125;
        initial.metrics.fx_aed_usd = 0.273;

        let state: SharedState = Arc::new(RwLock::new(initial));
        let monitor = TreasuryMonitor::new(state.clone(), 1, Arc::new(SimulatedAlexClient));

        assert!(monitor.update_balances().await.is_err());

        let current = state.read().expect("lock poisoned");
        assert_eq!(current.metrics.treasury_balance_stx, 1_000_000.0);
        assert_eq!(current.metrics.treasury_balance_btc, 1_050_000_000);
        assert_eq!(current.metrics.sbtc_liquidity, 5_000_000.0);
        assert_eq!(current.metrics.syi_index, 0.078);
        assert_eq!(current.metrics.last_treasury_update, 1_700_000_000);
        assert_eq!(current.metrics.fx_rmb_usd, 0.1425);
        assert_eq!(current.metrics.fx_rub_usd, 0.0115);
        assert_eq!(current.metrics.fx_inr_usd, 0.0125);
        assert_eq!(current.metrics.fx_aed_usd, 0.273);
    }
}
