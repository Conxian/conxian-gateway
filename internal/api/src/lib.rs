#[cfg(all(feature = "mock-integrations", not(any(debug_assertions, test))))]
compile_error!("feature `mock-integrations` must not be enabled in release builds");

pub mod a2p;
pub mod auth;
pub mod fiat;
pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod x402;

pub use routes::configure_routes;

use crate::a2p::A2pRouter;
use crate::fiat::FiatRouter;
use compliance::{IdentityManager, ZkcVerifier};
use conxian_core::{SettlementProposal, SharedState};
pub use engine::stacks::alex::AlexClient;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::RwLock;

/// Global application state for the Conxian Gateway API.
#[derive(Clone)]
pub struct AppState {
    pub shared: SharedState,
    pub fiat: Arc<FiatRouter>,
    pub a2p: Arc<A2pRouter>,
    pub identity: Arc<IdentityManager>,
    pub compliance: Arc<ZkcVerifier>,
    pub alex: Arc<dyn AlexClient>,
    pub fiat_webhook_secret: String,
    pub settlement_ingress_secret: String,
    pub settlement_log: Arc<RwLock<VecDeque<SettlementProposal>>>,
    pub offline_queue: Arc<dyn conxian_core::OfflineQueue>,
}

pub fn new_settlement_log() -> Arc<RwLock<VecDeque<SettlementProposal>>> {
    Arc::new(RwLock::new(VecDeque::new()))
}

pub fn new_offline_queue() -> Arc<dyn conxian_core::OfflineQueue> {
    let key = [0u8; 32]; // In production, this would be an enclave-wrapped key
    Arc::new(
        conxian_core::persistence::EncryptedOfflineQueue::new("offline_queue.db", key)
            .expect("Failed to init offline queue"),
    )
}
