#[cfg(all(feature = "mock-integrations", not(any(debug_assertions, test))))]
compile_error!("feature `mock-integrations` must not be enabled in release builds");

pub mod a2p;
pub mod admin;
pub mod auth;
pub mod billing;
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

/// Build the Lightning settlement adapter from the environment.
///
/// When `NWC_URI` holds a valid `nostr+walletconnect://` URI the real
/// [`crate::nwc_backend::NwcLightningBackend`] is selected; otherwise the
/// adapter falls back to the fail-closed production backend. Selection never
/// panics: a malformed or absent URI logs a warning and keeps the fail-closed backend, so the
/// process always starts and x402 settlement stays closed until configured.
pub fn new_lightning_adapter_from_env() -> Arc<LightningAdapter> {
    match std::env::var("NWC_URI") {
        Ok(uri) if !uri.trim().is_empty() => {
            match crate::nostr::NwcConnection::parse_uri(uri.trim()) {
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
            }
        }
        _ => {
            tracing::info!("Lightning settlement backend: fail-closed default (NWC_URI unset)");
            new_production_lightning_adapter()
        }
    }
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
