#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::{GatewayState, SharedState};
    use crate::stacks::alex::SimulatedAlexClient;
    use std::sync::{Arc, RwLock};

    #[tokio::test]
    async fn test_treasury_monitor_syi_calculation() {
        let state: SharedState = Arc::new(RwLock::new(GatewayState::new()));
        let alex = Arc::new(SimulatedAlexClient);
        let monitor = TreasuryMonitor::new(state.clone(), 1, alex);

        monitor.update_balances().await.unwrap();

        let s = state.read().unwrap();
        assert!(s.metrics.syi_index > 0.0);
        assert!(s.metrics.treasury_balance_stx > 1000000.0);
        assert_eq!(s.metrics.treasury_balance_btc, 1050000000);
    }
}
