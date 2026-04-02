mod config;

use api::a2p::A2pRouter;
use api::fiat::FiatRouter;
use api::{configure_routes, AppState};
use compliance::{IdentityManager, ZkcVerifier};
use config::Config;
use conxian_core::persistence::FilePersistence;
use conxian_core::{GatewayState, Persistence, SharedState};
use engine::{
    BitcoinListener, BitcoinRpcClient, NttRelayer, StacksListener, StacksRpcClient, TreasuryMonitor,
};
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

    let mut btc_listener = BitcoinListener::new(
        btc_rpc,
        state.clone(),
        persistence.clone(),
        config.bitcoin_sync_interval,
    );

    // Initialize Stacks listener
    let stx_rpc = StacksRpcClient::new(&config.stacks_rpc_url);
    let mut stx_listener = StacksListener::new(
        stx_rpc,
        state.clone(),
        persistence,
        config.stacks_sync_interval,
    );

    // Initialize Treasury monitor
    let treasury_monitor = TreasuryMonitor::new(state.clone(), 60);

    // Initialize NTT Relayer
    let ntt_relayer = NttRelayer::new(state.clone(), 30);

    // Initialize Institutional Service Routers (Industry Enhancement: CON-41, CON-66)
    let fiat_router = Arc::new(FiatRouter::new(
        config.ramp_api_key.clone(),
        config.investec_client_id.clone(),
        config.investec_secret.clone(),
        config.alchemy_pay_app_id.clone(),
        config.alchemy_pay_secret.clone(),
        config.banxa_api_key.clone(),
        config.banxa_secret.clone(),
    ));

    let a2p_router = Arc::new(A2pRouter::new(
        config.infobip_api_key.clone(),
        config.infobip_base_url.clone(),
        config.hmac_secret.clone(),
    ));

    let identity_manager = Arc::new(IdentityManager::new());
    let zkc_verifier = Arc::new(ZkcVerifier::new());

    // Create AppState
    let app_state = AppState {
        shared: state.clone(),
        fiat: fiat_router,
        a2p: a2p_router,
        identity: identity_manager,
        compliance: zkc_verifier,
        fiat_webhook_secret: config.fiat_webhook_secret.clone(),
    };

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

    let mut treasury_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            res = treasury_monitor.run() => {
                if let Err(e) = res {
                    error!("Treasury monitor failed: {}", e);
                }
            }
            _ = treasury_shutdown_rx.recv() => {
                info!("Treasury monitor stopping...");
            }
        }
    });

    let mut ntt_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            res = ntt_relayer.run() => {
                if let Err(e) = res {
                    error!("NTT relayer failed: {}", e);
                }
            }
            _ = ntt_shutdown_rx.recv() => {
                info!("NTT relayer stopping...");
            }
        }
    });

    // Configure and start API server
    let app = configure_routes(app_state, config.api_token);
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
