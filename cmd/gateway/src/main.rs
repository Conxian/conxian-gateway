mod config;

use api::configure_routes;
use config::Config;
use conxian_core::{GatewayState, SharedState};
use engine::{BitcoinListener, BitcoinRpcClient, StacksListener};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Conxian Gateway...");

    // Load configuration
    let config = Config::from_env();

    // Initialize shared state
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));

    // Initialize Bitcoin RPC
    let btc_rpc = BitcoinRpcClient::new(
        &config.bitcoin_rpc_url,
        &config.bitcoin_rpc_user,
        &config.bitcoin_rpc_pass,
    )?;

    let mut btc_listener = BitcoinListener::new(btc_rpc, state.clone());

    // Initialize Stacks listener
    let mut stx_listener = StacksListener::new(state.clone());

    // Spawn listeners
    tokio::spawn(async move {
        if let Err(e) = btc_listener.run().await {
            tracing::error!("Bitcoin listener failed: {}", e);
        }
    });

    tokio::spawn(async move {
        if let Err(e) = stx_listener.run().await {
            tracing::error!("Stacks listener failed: {}", e);
        }
    });

    // Configure and start API server
    let app = configure_routes(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], config.api_port));
    info!("API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
