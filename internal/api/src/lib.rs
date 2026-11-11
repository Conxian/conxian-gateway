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
pub mod m2m_auth;
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

/// Select the production Lightning adapter from the environment.
///
/// When `NWC_URI` holds a valid `nostr+walletconnect://` URI the real
/// [`crate::nwc_backend::NwcLightningBackend`] is selected; otherwise the
/// fail-closed production backend. Selection never panics: a malformed or absent
/// URI logs a warning and keeps the fail-closed backend.
///
/// The x402 rail uses the **server-issues → client-pays → server-verifies**
/// flow (`issue_challenge` / `verify_challenge`): the server issues the invoice
/// with `make_invoice`, and verifies settlement with `lookup_invoice`. It never
/// pays a client-supplied invoice, so enabling NWC no longer enables the
/// inverted fund-drain path (#874).
fn new_lightning_adapter_for_nwc_uri(nwc_uri: Option<&str>) -> Arc<LightningAdapter> {
    match nwc_uri.map(str::trim).filter(|uri| !uri.is_empty()) {
        Some(uri) => match crate::nostr::NwcConnection::parse_uri(uri) {
            Ok(connection) => {
                tracing::info!("Lightning settlement backend: NWC (Nostr Wallet Connect)");
                Arc::new(LightningAdapter::new(Arc::new(
                    crate::nwc_backend::NwcLightningBackend::new(connection),
                )))
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "NWC_URI is set but invalid; using the fail-closed Lightning backend"
                );
                new_production_lightning_adapter()
            }
        },
        None => {
            tracing::info!("Lightning settlement backend: fail-closed default (NWC_URI unset)");
            new_production_lightning_adapter()
        }
    }
}

/// Build the Lightning settlement adapter from the environment. `NWC_URI` (when
/// valid) selects the NWC backend; its value is never logged.
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

    #[tokio::test]
    async fn invalid_nwc_uri_keeps_x402_fail_closed() {
        // A malformed URI falls back to the fail-closed production backend, so a
        // protected route cannot issue an invoice (and therefore cannot settle).
        let adapter = new_lightning_adapter_for_nwc_uri(Some("nostr+walletconnect://configured"));
        assert!(matches!(
            adapter.issue_challenge("/settle", 1_000, 900).await,
            Err(LightningAdapterError::BackendUnavailable)
        ));
    }

    #[tokio::test]
    async fn unset_nwc_uri_keeps_x402_fail_closed() {
        let adapter = new_lightning_adapter_for_nwc_uri(None);
        assert!(matches!(
            adapter.issue_challenge("/settle", 1_000, 900).await,
            Err(LightningAdapterError::BackendUnavailable)
        ));
    }
}
