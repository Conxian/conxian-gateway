use crate::auth::auth_middleware;
use crate::handlers;
use crate::middleware::latency_tracker;
use crate::AppState;
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
    Router,
};

pub fn configure_routes(state: AppState, api_token: String) -> Router {
    let token_for_auth = api_token.clone();

    let public_routes = Router::new()
        .route("/health", get(handlers::health_check))
        .with_state(state.clone());

    let private_routes = Router::new()
        .route("/metrics", get(handlers::get_metrics))
        .route("/state", get(handlers::get_state))
        .route("/verify", post(handlers::verify_attestation))
        .route("/identity/exchange", post(handlers::exchange_identity))
        .route("/identity/resolve", post(handlers::resolve_identity_v1))
        .route("/fiat/webhook", post(handlers::verify_fiat_webhook))
        .route("/a2p/otp", post(handlers::send_otp))
        .route("/a2p/verify", post(handlers::verify_otp))
        .route("/erp/sync", post(handlers::sync_erp_ledger))
        .route("/settle", post(handlers::settle_job_card))
        .route("/iso20022/payment", post(handlers::generate_iso_payment))
        .route("/iso20022/pacs008", post(handlers::ingress_iso20022))
        .route("/iso20022/pacs009", post(handlers::ingress_iso20022))
        .route("/settlement/papss", post(handlers::ingress_papss))
        .route("/settlement/brics", post(handlers::ingress_brics))
        .route("/ingress/iso20022", post(handlers::ingress_iso20022))
        .route("/ingress/papss", post(handlers::ingress_papss))
        .route("/ingress/brics", post(handlers::ingress_brics))
        .route(
            "/settlements/external",
            get(handlers::get_external_settlements),
        )
        .route("/alex/quote", get(handlers::get_alex_quote))
        .route("/alex/swap", post(handlers::execute_alex_swap))
        .route(
            "/bounties/payouts/toggle",
            post(handlers::toggle_bounty_payouts),
        )
        .layer(middleware::from_fn(move |req, next| {
            auth_middleware(req, next, token_for_auth.clone())
        }))
        .with_state(state.clone());

    Router::new()
        .route("/api/v1/version", get(|| async { conxian_core::VERSION }))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            latency_tracker,
        ))
        .nest("/api/v1", public_routes.merge(private_routes))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB global body limit
}
