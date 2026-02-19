use crate::auth::auth_middleware;
use crate::handlers;
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use conxian_core::SharedState;

pub fn configure_routes(state: SharedState, api_token: String) -> Router {
    let token_for_auth = api_token.clone();

    let public_routes = Router::new().route("/health", get(handlers::health_check));

    let private_routes = Router::new()
        .route("/state", get(handlers::get_state))
        .route("/verify", post(handlers::verify_attestation))
        .layer(middleware::from_fn(move |req, next| {
            auth_middleware(req, next, token_for_auth.clone())
        }))
        .with_state(state);

    Router::new().nest("/api/v1", public_routes.merge(private_routes))
}
