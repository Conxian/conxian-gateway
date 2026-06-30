use crate::auth::auth_middleware;
use crate::middleware::latency_tracker;
use crate::AppState;
#[cfg(feature = "iso20022")]
use crate::camt;
use crate::world_id;
use crate::{admin, handlers, x402::x402_filter};
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
    Router,
};

pub fn configure_routes(state: AppState, api_token: String) -> Router {
    let token_for_auth = api_token.clone();

    let public_routes = Router::new()
        .route("/health", get(handlers::get_health))
        .with_state(state.clone());

    let admin_routes = Router::new()
        .route(
            "/releases/request-approval",
            post(admin::request_release_approval),
        )
        .route("/releases/decision", post(admin::submit_release_decision))
        .route(
            "/governance/decision",
            post(admin::submit_governance_decision),
        )
        .with_state(state.clone());

    let mut private_routes = Router::new()
        .route("/metrics", get(handlers::get_metrics))
        .route("/state", get(handlers::get_state))
        .route("/verify", post(handlers::verify_attestation))
        .route("/identity/exchange", post(handlers::exchange_identity))
        .route("/identity/resolve", post(handlers::resolve_identity_v1))
        .route("/fiat/session", post(handlers::create_fiat_session))
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
        .route("/settlement/cips", post(handlers::ingress_cips))
        .route("/settlement/spfs", post(handlers::ingress_spfs))
        .route("/settlement/mbridge", post(handlers::ingress_mbridge))
        .route("/ingress/iso20022", post(handlers::ingress_iso20022))
        .route("/ingress/papss", post(handlers::ingress_papss))
        .route("/ingress/brics", post(handlers::ingress_brics))
        .route("/ingress/cips", post(handlers::ingress_cips))
        .route("/ingress/spfs", post(handlers::ingress_spfs))
        .route("/ingress/mbridge", post(handlers::ingress_mbridge))
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
        .route("/pos/offline", post(handlers::handle_offline_pos))
        .route("/pos/sync", post(handlers::sync_offline_receipts))
        .route("/handoff/status", get(handlers::get_handoff_status))
        .route("/handoff/update", post(handlers::update_handoff_state))
        .route("/chains/list", get(handlers::list_supported_chains))
        .route("/chains/{chain}/height", get(handlers::get_chain_height))
        .route("/chains/{chain}/prepare", post(handlers::prepare_chain_tx))
        .route("/chains/{chain}/verify", post(handlers::verify_state_proof))
        .route("/dlc/bond", post(handlers::create_dlc_bond))
        .route(
            "/musig2/aggregate-keys",
            post(handlers::aggregate_musig2_keys),
        )
        .route("/verify/worldcoin", post(world_id::verify_world_id))
        .route("/nwc/relay", post(handlers::nwc_relay_settle));
    #[cfg(feature = "iso20022")]
    {
        private_routes = private_routes
            .route("/treasury/camt053", post(camt::generate_camt053))
            .route("/treasury/camt054", post(camt::generate_camt054));
    }
    let private_routes = private_routes
        .layer(middleware::from_fn_with_state(state.clone(), x402_filter))
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
        .route("/metrics", get(handlers::get_prometheus_metrics))
        .nest("/api/v1", public_routes.merge(private_routes))
        .nest("/admin/v1", admin_routes)
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB global body limit
}
