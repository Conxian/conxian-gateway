use anyhow::Context;
use conxian_api::{configure_routes, new_lightning_adapter, new_settlement_log, AppState};
use conxian_compliance::{CoreVerifier, IdentityManager, ZkcVerifier};
use conxian_core::{persistence::FilePersistence, GatewayState, Persistence, SharedState};
#[cfg(feature = "rgb-native")]
use conxian_engine::StashResolver;
use conxian_engine::{
    run_blocking_persistence,
    stacks::alex::{AlexClient, AlexRpcClient},
    BitcoinListener, BitcoinRpcClient, FeeBumpPolicyConfig, MempoolOrchestrator, NodeRgbAdapter,
    NttRelayer, RedisCoordinator, StacksListener, StacksRpcClient, TreasuryMonitor,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod config;
use config::Config;

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let format = std::env::var("RUST_LOG_FORMAT").unwrap_or_else(|_| "text".into());

    if format == "json" {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .json()
            .with_target(true)
            .with_current_span(false)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing with optional JSON format
    init_tracing();

    info!("Starting Conxian Gateway...");

    // Load configuration
    let config = Config::from_env();

    // Capture server start time for token expiry enforcement (CON-1276)
    let server_start = Instant::now();

    // Construct, exclusively lock, and load the synchronous file backend on
    // Tokio's blocking pool so startup never stalls an async runtime worker.
    let persistence_path = config.gateway_state_path.clone();
    let (persistence, _state_ownership_guard, p_state) =
        run_blocking_persistence("initialize Gateway file persistence", move || {
            let persistence = Arc::new(FilePersistence::new(&persistence_path)?);
            let ownership_guard = persistence.acquire_ownership()?;
            let state = persistence.load()?;
            Ok((persistence, ownership_guard, state))
        })
        .await
        .with_context(|| {
            format!(
                "failed to initialize, lock, or load Gateway persistence path '{}'",
                config.gateway_state_path
            )
        })?;

    // Initialize shared state
    let mut initial_state = GatewayState::default();
    initial_state.bitcoin.height = p_state.bitcoin_height;
    initial_state.stacks.height = p_state.stacks_height;
    info!(
        "Loaded persisted state: Bitcoin height {}, Stacks height {}",
        p_state.bitcoin_height, p_state.stacks_height
    );

    let state: SharedState = Arc::new(RwLock::new(initial_state));

    // Initialize Redis coordinator if configured
    let coordinator = config.redis_url.as_ref().and_then(|url| {
        match RedisCoordinator::new(
            url,
            config.redis_username.as_deref(),
            config.redis_password.as_deref(),
        ) {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                error!("Failed to initialize Redis coordinator: {}", e);
                None
            }
        }
    });

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
        coordinator.clone(),
        config.bitcoin_sync_interval,
    )
    .await?;

    // Initialize mempool orchestrator (CON-718)
    let mempool_rpc = BitcoinRpcClient::new(
        &config.bitcoin_rpc_url,
        &config.bitcoin_rpc_user,
        &config.bitcoin_rpc_pass,
    )?;

    // CON-768: Initialize RGB adapter
    let rgb_adapter: Option<Arc<dyn conxian_core::RgbAdapter>> =
        if config.rgb_mode != conxian_core::RolloutMode::Disabled {
            let adapter = NodeRgbAdapter::new(config.rgb_mode, config.rgb_node_url.clone());
            #[cfg(feature = "rgb-native")]
            let adapter = if let (Some(stash_path), Some(esplora_url)) = (
                config.rgb_stash_path.as_deref(),
                config.rgb_esplora_url.as_deref(),
            ) {
                let resolver = StashResolver::new_with_network(
                    stash_path,
                    esplora_url,
                    matches!(config.network, config::Network::Testnet),
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                adapter.with_stash(Arc::new(resolver))
            } else {
                adapter
            };
            Some(Arc::new(adapter))
        } else {
            None
        };

    let mempool_orchestrator = MempoolOrchestrator::new(
        mempool_rpc,
        persistence.clone(),
        config.mempool_orchestrator_interval,
        FeeBumpPolicyConfig {
            stuck_threshold_secs: config.mempool_stuck_threshold_secs,
            max_attempts: config.mempool_max_fee_bump_attempts,
            max_fee_rate_sat_vb: config.mempool_max_fee_rate_sat_vb,
            min_bump_increment_sat_vb: config.mempool_min_bump_increment_sat_vb,
        },
        rgb_adapter,
    );

    // Initialize Stacks listener
    let stx_rpc = StacksRpcClient::new(&config.stacks_rpc_url);
    let mut stx_listener = StacksListener::new(
        stx_rpc.clone(),
        state.clone(),
        persistence.clone(),
        coordinator.clone(),
        config.stacks_sync_interval,
    )
    .await?;

    // ALEX Client Initialization. The quote path remains read-only and
    // unverified. Unsigned payload preparation is disabled unless an exact,
    // network-qualified helper principal is explicitly configured.
    let alex_network = match config.network {
        config::Network::Mainnet => Some(conxian_core::AlexNetwork::Mainnet),
        config::Network::Testnet => Some(conxian_core::AlexNetwork::Testnet),
        config::Network::Simulated => None,
    };
    let alex_client: Arc<dyn AlexClient> = match (
        alex_network,
        config.alex_helper_principal.as_deref(),
    ) {
        (Some(network), Some(helper_principal)) => Arc::new(AlexRpcClient::with_helper(
            Box::new(stx_rpc.clone()),
            &config.alex_api_url,
            network,
            helper_principal,
        )?),
        (Some(network), None) => {
            info!(
                network = %config.network,
                "ALEX unsigned payload preparation disabled: ALEX_HELPER_PRINCIPAL is not configured"
            );
            Arc::new(AlexRpcClient::new_for_network(
                Box::new(stx_rpc.clone()),
                &config.alex_api_url,
                Some(network),
            ))
        }
        (None, Some(_)) => {
            return Err(anyhow::anyhow!(
                "ALEX_HELPER_PRINCIPAL requires mainnet or testnet; simulated mode remains read-only"
            ));
        }
        (None, None) => {
            info!(
                network = %config.network,
                "ALEX unsigned payload preparation disabled in simulated mode"
            );
            Arc::new(AlexRpcClient::new_for_network(
                Box::new(stx_rpc.clone()),
                &config.alex_api_url,
                None,
            ))
        }
    };

    // Initialize Treasury monitor
    let treasury_monitor = TreasuryMonitor::new(state.clone(), 60, alex_client.clone());

    // Initialize NTT Relayer
    let ntt_relayer = NttRelayer::new(state.clone(), 30);

    // Initialize Institutional Service Routers
    let fiat_router = Arc::new(conxian_api::fiat::FiatRouter::new(
        config.ramp_api_key.clone(),
        config.investec_client_id.clone(),
        config.investec_secret.clone(),
        config.alchemy_pay_app_id.clone(),
        config.alchemy_pay_secret.clone(),
        config.banxa_api_key.clone(),
        config.banxa_secret.clone(),
    ));

    let a2p_router = Arc::new(conxian_api::a2p::A2pRouter::new(
        config.infobip_api_key.clone(),
        config.infobip_base_url.clone(),
        config.hmac_secret.clone(),
    ));

    // Inject StacksRpc into IdentityManager for BNS resolution
    let identity_manager = Arc::new(IdentityManager::with_stacks_rpc(Box::new(stx_rpc.clone())));
    let zkc_verifier = Arc::new(ZkcVerifier::new());

    // Parse offline queue secret into 32-byte key
    let mut offline_key = [0u8; 32];
    let secret_bytes = config.offline_queue_secret.as_bytes();
    if secret_bytes.len() >= 32 {
        offline_key.copy_from_slice(&secret_bytes[0..32]);
    } else {
        offline_key[0..secret_bytes.len()].copy_from_slice(secret_bytes);
    }

    let mut multi_chain: HashMap<String, Arc<dyn conxian_core::ChainAdapter>> = HashMap::new();

    let liquid_rpc =
        BitcoinRpcClient::new(&config.liquid_rpc_url, "", "").expect("Failed to init Liquid RPC");
    multi_chain.insert(
        "liquid".to_string(),
        Arc::new(conxian_engine::LiquidAdapter::new(
            Arc::new(liquid_rpc),
            config.network.to_string(),
        )),
    );

    multi_chain.insert(
        "rootstock".to_string(),
        Arc::new(conxian_engine::RootstockAdapter::new(
            config.rootstock_rpc_url.clone(),
            config.network.to_string(),
        )),
    );

    let babylon_adapter = match config.babylon_api_url.as_deref() {
        Some(api_url) => conxian_engine::BabylonAdapter::with_babylon_api_url(
            config.network.to_string(),
            api_url,
        )?,
        None => conxian_engine::BabylonAdapter::new(config.network.to_string()),
    };
    multi_chain.insert("babylon".to_string(), Arc::new(babylon_adapter));

    multi_chain.insert(
        "bitvm".to_string(),
        Arc::new(conxian_engine::BitVmAdapter::new(
            config.network.to_string(),
        )),
    );

    multi_chain.insert(
        "fedimint".to_string(),
        Arc::new(conxian_engine::FedimintAdapter::new(
            config.network.to_string(),
        )),
    );

    multi_chain.insert(
        "citrea".to_string(),
        Arc::new(conxian_engine::CitreaAdapter::new(
            "https://rpc.testnet.citrea.xyz".to_string(), // Default testnet RPC
            config.network.to_string(),
        )),
    );

    multi_chain.insert(
        "strata".to_string(),
        Arc::new(conxian_engine::StrataAdapter::new(
            config.network.to_string(),
        )),
    );

    let verifier = Arc::new(conxian_compliance::UniversalVerifier::new(
        zkc_verifier.clone() as Arc<dyn CoreVerifier>,
        multi_chain.clone(),
    ));

    // Create AppState
    let app_state = AppState {
        shared: state.clone(),
        persistence: Some(persistence.clone()),
        fiat: fiat_router,
        a2p: a2p_router,
        identity: identity_manager,
        compliance: zkc_verifier,
        verifier,
        alex: alex_client,
        lightning: new_lightning_adapter(),
        fiat_webhook_secret: config.fiat_webhook_secret.clone(),
        settlement_ingress_secret: config.settlement_ingress_secret.clone(),
        settlement_log: new_settlement_log(),
        offline_queue: conxian_api::new_offline_queue(offline_key),
        multi_chain,
        coordinator,
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

    let mut mempool_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            res = mempool_orchestrator.run() => {
                if let Err(e) = res {
                    error!("Mempool orchestrator failed: {}", e);
                }
            }
            _ = mempool_shutdown_rx.recv() => {
                info!("Mempool orchestrator stopping...");
            }
        }
    });

    // Configure and start API server
    let app = configure_routes(
        app_state,
        config.api_token,
        server_start,
        config.token_ttl_seconds,
    );
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
