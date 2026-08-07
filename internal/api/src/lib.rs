#[cfg(all(feature = "mock-integrations", not(any(debug_assertions, test))))]
compile_error!("feature `mock-integrations` must not be enabled in release builds");

pub mod a2p;
pub mod admin;
pub mod auth;
pub mod camt;
pub mod fiat;
pub mod handlers;
pub mod lightning;
pub mod mempool_telemetry;
pub mod middleware;
pub mod nostr;
pub mod routes;
pub mod shadow_observation;
pub mod world_id;
pub mod x402;

pub use routes::configure_routes;

use crate::a2p::A2pRouter;
use crate::fiat::FiatRouter;
use crate::lightning::{LightningAdapter, SimulatedLightningBackend};
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
    pub a2p: Arc<A2pRouter>,
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

pub fn new_settlement_log() -> Arc<RwLock<VecDeque<SettlementProposal>>> {
    Arc::new(RwLock::new(VecDeque::new()))
}

pub fn new_offline_queue(key: [u8; 32]) -> Arc<dyn conxian_core::OfflineQueue> {
    Arc::new(
        conxian_core::persistence::EncryptedOfflineQueue::new("offline_queue.db", key)
            .expect("Failed to init offline queue"),
    )
}
