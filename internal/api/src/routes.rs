use crate::auth::auth_middleware;
use crate::handlers;
use crate::middleware::latency_tracker;
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use conxian_core::SharedState;

pub fn configure_routes(state: SharedState, api_token: String) -> Router {
    let token_for_auth = api_token.clone();

    let public_routes = Router::new()
        .route("/health", get(handlers::health_check))
        .route("/metrics", get(handlers::get_metrics))
        .with_state(state.clone());

    let private_routes = Router::new()
        .route("/state", get(handlers::get_state))
        .route("/verify", post(handlers::verify_attestation))
        .route("/identity/exchange", post(handlers::exchange_identity))
        .route("/iso20022/payment", post(handlers::generate_iso_payment))
        .route("/fiat/session", post(handlers::create_fiat_session))
        .route("/fiat/webhook", post(handlers::verify_fiat_webhook))
        .route("/a2p/otp", post(handlers::send_otp))
        .route("/a2p/verify", post(handlers::verify_otp))
        .route("/erp/sync", post(handlers::sync_erp_ledger))
        .route("/settle", post(handlers::settle_job_card))
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
}
