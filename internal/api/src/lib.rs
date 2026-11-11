#[cfg(all(feature = "mock-integrations", not(any(debug_assertions, test))))]
compile_error!("feature `mock-integrations` must not be enabled in release builds");

pub mod a2p;
pub mod admin;
pub mod auth;
pub mod billing;
pub mod billing_strike;
pub mod camt;
pub mod capabilities;
pub mod contribution_ledger;
pub mod contribution_store;
pub mod credit_ledger;
pub mod discovery;
pub mod fiat;
pub mod handlers;
pub mod lightning;
pub mod mempool_telemetry;
pub mod middleware;
pub mod mor_webhook;
pub mod mor_webhook_store;
pub mod nostr;
pub mod nwc_backend;
pub mod onboarding;
pub mod onboarding_store;
pub mod rate_limit;
pub mod routes;
pub mod sentinel;
pub mod shadow_observation;
pub mod tenant;
pub mod world_id;
pub mod x402;

pub use routes::{configure_routes, init_managed_store};

use crate::a2p::A2pRouter;
use crate::fiat::FiatRouter;
use crate::lightning::{LightningAdapter, ProductionLightningBackend, SimulatedLightningBackend};
use conxian_compliance::{IdentityManager, UniversalVerifier, ZkcVerifier};
use conxian_core::{Persistence, SettlementProposal, SharedState};
pub use conxian_engine::stacks::alex::AlexClient;
use conxian_engine::stacks::alex::AlexPreparationService;
use conxian_engine::BitcoinCoreShadowObserver;
pub use conxian_engine::RedisCoordinator;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::RwLock;

/// Global application state for the Conxian Gateway API.
#[derive(Clone)]
pub struct AppState {
    pub shared: SharedState,
    /// Persistence backend containing the Gateway-owned tracked mempool state.
    /// This is optional for lightweight API test harnesses; production wiring
    /// supplies the same backend used by the listeners and orchestrator.
    pub persistence: Option<Arc<dyn Persistence>>,
    /// Optional, explicitly enabled read-only observer for the configured
    /// configured Bitcoin Core endpoint. It is not used by routing or fee decisions.
    pub bitcoin_core_shadow_observer: Option<Arc<dyn BitcoinCoreShadowObserver>>,
    pub fiat: Arc<FiatRouter>,
    pub a2p: Option<Arc<A2pRouter>>,
    pub identity: Arc<IdentityManager>,
    pub compliance: Arc<ZkcVerifier>,
    pub verifier: Arc<UniversalVerifier>,
    pub alex: Arc<dyn AlexClient>,
    pub alex_preparer: Arc<AlexPreparationService>,
    pub multi_chain: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>>,
    pub lightning: Arc<LightningAdapter>,
    pub fiat_webhook_secret: String,
    pub settlement_ingress_secret: String,
    pub settlement_log: Arc<RwLock<VecDeque<SettlementProposal>>>,
    pub offline_queue: Arc<dyn conxian_core::OfflineQueue>,
    pub coordinator: Option<Arc<RedisCoordinator>>,
}

pub fn new_lightning_adapter() -> Arc<LightningAdapter> {
    Arc::new(LightningAdapter::new(Arc::new(SimulatedLightningBackend)))
}

pub fn new_production_lightning_adapter() -> Arc<LightningAdapter> {
    Arc::new(LightningAdapter::new(Arc::new(ProductionLightningBackend)))
}

/// Select the production Lightning adapter without enabling the legacy NWC
/// pay-invoice path.
///
/// The current x402 request parser accepts caller-supplied invoice data, while
/// `NwcLightningBackend` calls `pay_invoice`. That is the inverse of the
/// required server-issues/client-pays/server-verifies flow and can spend an
/// operator wallet on an invoice supplied by an API-token holder. Keep the
/// production backend fail-closed until #874 implements server-issued invoices,
/// durable resource/amount/expiry binding, and `lookup_invoice` verification.
///
/// The NWC adapter remains in the codebase for the separately reviewed future
/// integration, but `NWC_URI` is deliberately not an activation switch.
fn new_lightning_adapter_for_nwc_uri(nwc_uri: Option<&str>) -> Arc<LightningAdapter> {
    if nwc_uri
        .map(str::trim)
        .filter(|uri| !uri.is_empty())
        .is_some()
    {
        tracing::error!(
            "NWC_URI is configured, but NWC x402 settlement is disabled until server-issued invoice verification is implemented (#874)"
        );
    } else {
        tracing::info!("Lightning settlement backend: fail-closed default (NWC_URI unset)");
    }

    new_production_lightning_adapter()
}

/// Build the Lightning settlement adapter from the environment.
/// `NWC_URI` is read only to emit a safe configuration warning; its value is
/// never logged, parsed, or used to enable wallet payments.
pub fn new_lightning_adapter_from_env() -> Arc<LightningAdapter> {
    let nwc_uri = std::env::var("NWC_URI").ok();
    new_lightning_adapter_for_nwc_uri(nwc_uri.as_deref())
}

pub fn new_settlement_log() -> Arc<RwLock<VecDeque<SettlementProposal>>> {
    Arc::new(RwLock::new(VecDeque::new()))
}

pub fn new_offline_queue(key: [u8; 32]) -> Arc<dyn conxian_core::OfflineQueue> {
    Arc::new(
        conxian_core::persistence::EncryptedOfflineQueue::new("offline_queue.db", key)
            .expect("Failed to init offline queue"),
    )
}

#[cfg(test)]
mod x402_nwc_activation_tests {
    use super::*;
    use crate::lightning::LightningAdapterError;
    use crate::x402::X402PaymentPayload;

    #[tokio::test]
    async fn configured_nwc_uri_keeps_x402_fail_closed() {
        let adapter = new_lightning_adapter_for_nwc_uri(Some("nostr+walletconnect://configured"));
        let payload = X402PaymentPayload {
            amount: 1_000,
            asset: "BTC".to_string(),
            challenge: "client-supplied-invoice-must-not-be-paid".to_string(),
            expiry: u64::MAX,
            proof_refs: vec![],
        };

        assert!(matches!(
            adapter.execute_payment(&payload).await,
            Err(LightningAdapterError::BackendUnavailable)
        ));
    }
}
