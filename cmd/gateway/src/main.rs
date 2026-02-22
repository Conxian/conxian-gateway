mod config;

use api::configure_routes;
use config::Config;
use conxian_core::persistence::FilePersistence;
use conxian_core::{GatewayState, Persistence, SharedState};
use engine::{BitcoinListener, BitcoinRpcClient, StacksListener, StacksRpcClient};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::signal;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Conxian Gateway...");

    // Load configuration
    let config = Config::from_env();

    // Initialize persistence
    let persistence = Arc::new(FilePersistence::new("gateway_state.json"));

    // Initialize shared state
    let mut initial_state = GatewayState::default();
    if let Ok(p_state) = persistence.load() {
        initial_state.bitcoin.height = p_state.bitcoin_height;
        initial_state.stacks.height = p_state.stacks_height;
        info!(
            "Loaded persisted state: Bitcoin height {}, Stacks height {}",
            p_state.bitcoin_height, p_state.stacks_height
        );
    }

    let state: SharedState = Arc::new(RwLock::new(initial_state));

    // Initialize Bitcoin RPC
    let btc_rpc = BitcoinRpcClient::new(
        &config.bitcoin_rpc_url,
        &config.bitcoin_rpc_user,
        &config.bitcoin_rpc_pass,
    )?;

    let mut btc_listener = BitcoinListener::new(btc_rpc, state.clone(), persistence.clone());

    // Initialize Stacks listener
    let stx_rpc = StacksRpcClient::new(&config.stacks_rpc_url);
    let mut stx_listener = StacksListener::new(stx_rpc, state.clone(), persistence);

    // Create a cancellation token for graceful shutdown of listeners
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    let mut btc_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            res = btc_listener.run() => {
                if let Err(e) = res {
                    error!("Bitcoin listener failed: {}", e);
                }
            }
            _ = btc_shutdown_rx.recv() => {
                info!("Bitcoin listener stopping...");
            }
        }
    });

    let mut stx_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            res = stx_listener.run() => {
                if let Err(e) = res {
                    error!("Stacks listener failed: {}", e);
                }
            }
            _ = stx_shutdown_rx.recv() => {
                info!("Stacks listener stopping...");
            }
        }
    });

    // Configure and start API server
    let app = configure_routes(state, config.api_token);
    let addr = SocketAddr::from(([0, 0, 0, 0], config.api_port));
    info!("API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Axum graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await?;

    info!("Conxian Gateway shut down successfully.");
    Ok(())
}

async fn shutdown_signal(shutdown_tx: tokio::sync::broadcast::Sender<()>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received...");
    let _ = shutdown_tx.send(());
}
