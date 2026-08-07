use anyhow::Context;
use conxian_api::{configure_routes, new_lightning_adapter, new_settlement_log, AppState};
use conxian_compliance::{CoreVerifier, IdentityManager, ZkcVerifier};
use conxian_core::{ConxianError, GatewayState, Persistence, SharedState};
use conxian_engine::{
    run_blocking_persistence,
    stacks::alex::{
        load_alex_venue_manifest_for_network, AlexClient, AlexPreparationService, AlexRpcClient,
    },
    BitcoinCoreShadowObserver, BitcoinCoreShadowObserverClient, BitcoinListener, BitcoinRpcClient,
    FeeBumpPolicyConfig, MempoolOrchestrator, NodeRgbAdapter, NttRelayer, RedisCoordinator,
    SovereignBackend, StacksListener, StacksRpcClient, TreasuryMonitor,
};
#[cfg(feature = "rgb-native")]
use conxian_engine::{Bip340IssuerPolicy, StashResolver};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod config;
mod supervisor;
use config::Config;
use supervisor::{shutdown_requested, supervise, CriticalTask};

const CRITICAL_TASK_SHUTDOWN_GRACE: Duration = Duration::from_secs(30);

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
    let allow_unknown_filesystem = config.gateway_allow_unknown_state_filesystem;
    let backend = SovereignBackend::from_env();
    let (persistence, _state_ownership_guard, p_state) =
        run_blocking_persistence("initialize Gateway persistence", move || {
            config::validate_state_filesystem(
                std::path::Path::new(&persistence_path),
                allow_unknown_filesystem,
            )
            .map_err(ConxianError::Persistence)?;

            if backend == SovereignBackend::File {
                let fp = conxian_core::persistence::FilePersistence::new(std::path::Path::new(
                    &persistence_path,
                ))?;
                let ownership_guard = fp.acquire_ownership()?;
                let state = fp.load()?;
                Ok((
                    Arc::new(fp) as Arc<dyn Persistence>,
                    Some(ownership_guard),
                    state,
                ))
            } else {
                let persistence = backend.build(std::path::Path::new(&persistence_path))?;
                let state = persistence.load()?;
                Ok((persistence, None, state))
            }
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

    let bitcoin_core_shadow_observer: Option<Arc<dyn BitcoinCoreShadowObserver>> = config
        .bitcoin_core_shadow_observation_enabled
        .then(|| {
            BitcoinCoreShadowObserverClient::new(
                &config.bitcoin_rpc_url,
                &config.bitcoin_rpc_user,
                &config.bitcoin_rpc_pass,
            )
            .map(|observer| Arc::new(observer) as Arc<dyn BitcoinCoreShadowObserver>)
        })
        .transpose()?;

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
            let adapter = {
                let mut adapter = adapter;
                if let (Some(stash_path), Some(esplora_url)) = (
                    config.rgb_stash_path.as_deref(),
                    config.rgb_esplora_url.as_deref(),
                ) {
                    let resolver = StashResolver::new_with_network(
                        stash_path,
                        esplora_url,
                        matches!(config.network, config::Network::Testnet),
                    )
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    adapter = adapter.with_stash(Arc::new(resolver));
                }
                if let Some(ref policy_path) = config.rgb_issuer_policy_path {
                    match Bip340IssuerPolicy::load_json_file(std::path::Path::new(policy_path)) {
                        Ok(policy) => {
                            tracing::info!(
                                issuer_count = policy.issuer_count(),
                                "RGB BIP340 issuer policy loaded"
                            );
                            adapter = adapter.with_issuer_policy(policy);
                        }
                        Err(error) => {
                            tracing::error!(
                                path = %policy_path,
                                error = %error,
                                "Failed to load RGB issuer policy; \
                                 all issuer signatures will be rejected"
                            );
                        }
                    }
                }
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

    // ALEX quote compatibility remains read-only and unverified. Preparation
    // is enabled only when a strict operator-supplied venue manifest verifies.
    let alex_network = match config.network {
        config::Network::Mainnet => Some(conxian_core::AlexNetwork::Mainnet),
        config::Network::Testnet => Some(conxian_core::AlexNetwork::Testnet),
        config::Network::Simulated => None,
    };
    let alex_client: Arc<dyn AlexClient> = Arc::new(AlexRpcClient::new(
        Box::new(stx_rpc.clone()),
        &config.alex_api_url,
    ));
    let now_epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let alex_manifest = match config.alex_venue_manifest_path.as_deref() {
        None => {
            info!(
                network = %config.network,
                "ALEX prepare disabled: no venue manifest configured"
            );
            None
        }
        Some(path) => match alex_network {
            None => {
                error!(
                    network = %config.network,
                    code = "ALEX_MANIFEST_NETWORK_MISMATCH",
                    "ALEX prepare disabled"
                );
                None
            }
            Some(network) => match load_alex_venue_manifest_for_network(
                std::path::Path::new(path),
                now_epoch_secs,
                network,
            ) {
                Ok(manifest) => {
                    info!(
                        network = %config.network,
                        manifest_id = %manifest.manifest().manifest_id,
                        manifest_revision = %manifest.manifest().manifest_revision,
                        "ALEX venue manifest loaded"
                    );
                    Some(manifest)
                }
                Err(error) => {
                    error!(
                        network = %config.network,
                        code = error.code(),
                        "ALEX prepare disabled"
                    );
                    None
                }
            },
        },
    };
    let alex_preparer = Arc::new(AlexPreparationService::new(
        alex_client.clone(),
        alex_manifest,
    ));

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
        bitcoin_core_shadow_observer,
        fiat: fiat_router,
        a2p: a2p_router,
        identity: identity_manager,
        compliance: zkc_verifier,
        verifier,
        alex: alex_client,
        alex_preparer,
        lightning: new_lightning_adapter(),
        fiat_webhook_secret: config.fiat_webhook_secret.clone(),
        settlement_ingress_secret: config.settlement_ingress_secret.clone(),
        settlement_log: new_settlement_log(),
        offline_queue: conxian_api::new_offline_queue(offline_key),
        multi_chain,
        coordinator,
    };

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
    let tasks = vec![
        CriticalTask::new("Bitcoin listener", move |shutdown| async move {
            btc_listener
                .run_until_shutdown(shutdown)
                .await
                .map_err(Into::into)
        }),
        CriticalTask::new("Stacks listener", move |shutdown| async move {
            stx_listener
                .run_until_shutdown(shutdown)
                .await
                .map_err(Into::into)
        }),
        CriticalTask::new("treasury monitor", move |shutdown| async move {
            treasury_monitor
                .run_until_shutdown(shutdown)
                .await
                .map_err(Into::into)
        }),
        CriticalTask::new("NTT relayer", move |shutdown| async move {
            ntt_relayer
                .run_until_shutdown(shutdown)
                .await
                .map_err(Into::into)
        }),
        CriticalTask::new("mempool orchestrator", move |shutdown| async move {
            mempool_orchestrator
                .run_until_shutdown(shutdown)
                .await
                .map_err(Into::into)
        }),
        CriticalTask::new("HTTP server", move |mut shutdown| async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    shutdown_requested(&mut shutdown).await;
                })
                .await
                .context("HTTP server failed")
        }),
    ];

    supervise(tasks, shutdown_signal(), CRITICAL_TASK_SHUTDOWN_GRACE).await?;

    info!("Conxian Gateway shut down successfully.");
    Ok(())
}

async fn shutdown_signal() -> anyhow::Result<()> {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .context("failed to install Ctrl+C handler")
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .context("failed to install SIGTERM handler")?
            .recv()
            .await;
        anyhow::Ok(())
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<anyhow::Result<()>>();

    tokio::select! {
        result = ctrl_c => result?,
        result = terminate => result?,
    }
    Ok(())
}
