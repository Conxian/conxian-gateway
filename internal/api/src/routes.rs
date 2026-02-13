use axum::{
    routing::{get, post},
    Router,
    middleware,
};
use crate::handlers;
use crate::auth::auth_middleware;

pub fn configure_routes() -> Router {
    let public_routes = Router::new()
        .route("/health", get(handlers::health_check));

    let private_routes = Router::new()
        .route("/state", get(handlers::get_state))
        .route("/verify", post(handlers::verify_attestation))
        .layer(middleware::from_fn(auth_middleware));

    Router::new()
        .nest("/api/v1", public_routes.merge(private_routes))
}
