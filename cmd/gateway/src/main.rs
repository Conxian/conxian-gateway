use api::configure_routes;
use engine::{BitcoinRpcClient, BitcoinListener, StacksListener};
use std::net::SocketAddr;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Conxian Gateway...");

    // Initialize Bitcoin RPC (using placeholders for now)
    let btc_rpc = BitcoinRpcClient::new("http://localhost:18332", "user", "pass")?;
    let mut btc_listener = BitcoinListener::new(btc_rpc);

    // Initialize Stacks listener
    let mut stx_listener = StacksListener::new();

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
    let app = configure_routes();
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
