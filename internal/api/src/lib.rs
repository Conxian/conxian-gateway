pub mod a2p;
pub mod auth;
pub mod fiat;
pub mod handlers;
pub mod middleware;
pub mod routes;

pub use routes::configure_routes;

use crate::a2p::A2pRouter;
use crate::fiat::FiatRouter;
use compliance::{IdentityManager, ZkcVerifier};
use conxian_core::SharedState;
use std::sync::Arc;

/// Global application state for the Conxian Gateway API.
#[derive(Clone)]
pub struct AppState {
    pub shared: SharedState,
    pub fiat: Arc<FiatRouter>,
    pub a2p: Arc<A2pRouter>,
    pub identity: Arc<IdentityManager>,
    pub compliance: Arc<ZkcVerifier>,
    pub fiat_webhook_secret: String,
}
