use crate::auth::auth_middleware;
use crate::middleware::latency_tracker;
use crate::mor_webhook::{provider_from_env, KeyStore, MorWebhookState};
use crate::tenant::{managed_auth_middleware, ManagedAuth};
use crate::world_id;
use crate::AppState;
use crate::{admin, camt, handlers, shadow_observation, x402::x402_filter};
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, post},
    Router,
};
use std::time::Instant;

/// Builds the MoR webhook state from the environment, or `None` when the
/// provider or signing secret is unconfigured (the route is then not mounted).
///
/// A configured `MOR_WEBHOOK_DATABASE_URL` selects the durable Neon/Postgres
/// store; without it the webhook stays unconfigured rather than falling back to
/// a non-durable store, so an entitlement is never granted from volatile state.
fn mor_webhook_state_from_env(store: std::sync::Arc<dyn KeyStore>) -> Option<MorWebhookState> {
    let provider = provider_from_env()?;
    let secret = std::env::var("MOR_WEBHOOK_SECRET")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())?;
    Some(MorWebhookState {
        provider,
        secret,
        store,
    })
}

/// Builds the durable managed-key store from `MOR_WEBHOOK_DATABASE_URL`, if set.
/// Used both by the MoR webhook receiver and the managed-auth path so a key
/// minted by a webhook authenticates against the same store.
pub(crate) fn mor_key_store_from_env() -> Option<std::sync::Arc<dyn KeyStore>> {
    let database_url = std::env::var("MOR_WEBHOOK_DATABASE_URL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy(&database_url)
        .ok()?;
    Some(std::sync::Arc::new(
        crate::mor_webhook_store::PgKeyStore::new(pool),
    ))
}

/// Builds the self-serve onboarding API state (Gateway #713) from the
/// environment, or `None` when unconfigured.
///
/// The lane requires `ONBOARDING_TOKEN` (the no-card evaluation credential) and
/// a durable database (`MOR_WEBHOOK_DATABASE_URL`). Without a durable store the
/// routes are not mounted, so an entitlement is never granted from volatile
/// state — mirroring the MoR webhook's fail-closed mounting.
fn onboarding_api_from_env(
    key_store: Option<std::sync::Arc<dyn KeyStore>>,
) -> Option<crate::onboarding::OnboardingApi> {
    let database_url = std::env::var("MOR_WEBHOOK_DATABASE_URL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy(&database_url)
        .ok()?;
    let store = std::sync::Arc::new(crate::onboarding_store::PgOnboardingStore::new(pool));
    let service = crate::onboarding::OnboardingService::from_env(store, key_store);
    crate::onboarding::OnboardingApi::from_env(service)
}

pub fn configure_routes(
    state: AppState,
    api_token: String,
    server_start: Instant,
    token_ttl_seconds: Option<u64>,
) -> Router {
    let token_for_auth = api_token.clone();
    let token_for_alex = api_token.clone();

    // Durable managed-key store, shared by the MoR webhook receiver and the
    // managed-auth path so a webhook-minted key authenticates immediately.
    let mor_store = mor_key_store_from_env();

    // Multi-tenant managed-subscriber auth (managed API keys + Fusion JWT +
    // per-tenant rate limiting + Sentinel secret filtering).
    let managed_auth = ManagedAuth::from_env_with_store(&api_token, mor_store.clone());

    let public_routes = Router::new()
        .route("/health", get(handlers::get_health))
        .route(
            "/billing/subscribe",
            post(crate::billing::subscribe_handler),
        )
        .route("/billing/verify", post(crate::billing::verify_handler))
        .route("/billing/quote", post(crate::billing::quote_handler))
        .route("/billing/balance", get(crate::billing::balance_handler))
        .with_state(state.clone());

    let token_for_admin = api_token.clone();
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
        .layer(middleware::from_fn(move |req, next| {
            auth_middleware(
                req,
                next,
                token_for_admin.clone(),
                server_start,
                token_ttl_seconds,
            )
        }))
        .with_state(state.clone());

    let private_routes = Router::new()
        .route("/capabilities", get(crate::capabilities::get_capabilities))
        .route("/metrics", get(handlers::get_metrics))
        .route(
            "/bitcoin/mempool/telemetry",
            get(handlers::get_mempool_telemetry),
        )
        .route(
            "/bitcoin/core/shadow-observation",
            get(shadow_observation::get_bitcoin_core_shadow_observation),
        )
        .route("/state", get(handlers::get_state))
        .route("/treasury/syi", get(handlers::get_sovereign_yield_index))
        .route("/verify", post(handlers::verify_attestation))
        .route("/identity/exchange", post(handlers::exchange_identity))
        .route("/identity/resolve", post(handlers::resolve_identity_v1))
        .route(
            "/identity/resolve/machine",
            post(handlers::resolve_machine_identity),
        )
        .route("/fiat/session", post(handlers::create_fiat_session))
        .route("/fiat/webhook", post(handlers::verify_fiat_webhook))
        .route("/a2p/otp", post(handlers::send_otp))
        .route("/a2p/verify", post(handlers::verify_otp))
        .route("/erp/sync", post(handlers::sync_erp_ledger))
        .route("/settle", post(handlers::settle_job_card))
        .route("/iso20022/payment", post(handlers::generate_iso_payment))
        .route(
            "/iso20022/pain001",
            post(handlers::generate_iso_pain001_payment),
        )
        .route("/iso20022/pacs008", post(handlers::ingress_iso20022))
        .route("/iso20022/pacs009", post(handlers::ingress_iso20022))
        .route("/iso20022/camt053/generate", post(camt::generate_camt053))
        .route("/iso20022/camt054/generate", post(camt::generate_camt054))
        .route("/settlement/papss", post(handlers::ingress_papss))
        .route("/settlement/brics", post(handlers::ingress_brics))
        .route("/settlement/cips", post(handlers::ingress_cips))
        .route("/settlement/spfs", post(handlers::ingress_spfs))
        .route("/settlement/mbridge", post(handlers::ingress_mbridge))
        .route("/ingress/iso20022", post(handlers::ingress_iso20022))
        .route("/ingress/pain001", post(handlers::ingress_iso20022))
        .route("/ingress/papss", post(handlers::ingress_papss))
        .route("/ingress/brics", post(handlers::ingress_brics))
        .route("/ingress/cips", post(handlers::ingress_cips))
        .route("/ingress/spfs", post(handlers::ingress_spfs))
        .route("/ingress/mbridge", post(handlers::ingress_mbridge))
        .route(
            "/settlements/external",
            get(handlers::get_external_settlements),
        )
        .route(
            "/bounties/payouts/toggle",
            post(handlers::toggle_bounty_payouts),
        )
        .route("/pos/offline", post(handlers::handle_offline_pos))
        .route("/pos/event", post(handlers::ingress_pos_event))
        .route("/ingress/pos", post(handlers::ingress_pos_event))
        .route("/ingress/edi", post(handlers::ingress_edi_purchase_order))
        .route("/supply/edi", post(handlers::ingress_edi_purchase_order))
        .route("/invoicing/ubl", post(handlers::ingress_ubl_invoice))
        .route("/ingress/invoice", post(handlers::ingress_ubl_invoice))
        .route(
            "/compliance/zk-kyc",
            post(handlers::extract_zk_kyc_commitment),
        )
        .route("/ingress/kyc", post(handlers::extract_zk_kyc_commitment))
        .route("/pos/sync", post(handlers::sync_offline_receipts))
        .route("/handoff/status", get(handlers::get_handoff_status))
        .route("/handoff/update", post(handlers::update_handoff_state))
        .route("/chains/list", get(handlers::list_supported_chains))
        .route("/chains/{chain}/height", get(handlers::get_chain_height))
        .route("/chains/{chain}/prepare", post(handlers::prepare_chain_tx))
        .route("/chains/{chain}/verify", post(handlers::verify_state_proof))
        .route(
            "/canton/cbtc/verify",
            post(handlers::verify_cbtc_attestation),
        )
        .route("/dlc/bond", post(handlers::create_dlc_bond))
        .route(
            "/musig2/aggregate-keys",
            post(handlers::aggregate_musig2_keys),
        )
        .route("/verify/worldcoin", post(world_id::verify_world_id))
        .route("/nwc/relay", post(handlers::nwc_relay_settle))
        .route("/m2m/settle", post(handlers::settle_m2m))
        .route(
            "/canton/state/translate",
            post(handlers::translate_canton_state),
        )
        .route("/ccip/route", post(handlers::route_ccip_message))
        .route(
            "/rwa/machine/verify-revenue",
            post(handlers::verify_machine_rwa_revenue),
        );
    let private_routes = private_routes
        .layer(middleware::from_fn_with_state(state.clone(), x402_filter))
        .layer(middleware::from_fn(move |req, next| {
            auth_middleware(
                req,
                next,
                token_for_auth.clone(),
                server_start,
                token_ttl_seconds,
            )
        }))
        .with_state(state.clone());

    // ALEX remains private and bearer-authenticated, but its read-only quote,
    // unsigned preparation, and stably disabled execution surfaces are not a
    // paid/x402 capability boundary.
    let alex_routes = Router::new()
        .route("/alex/quote", get(handlers::get_alex_quote))
        .route("/alex/prepare", post(handlers::prepare_alex_swap))
        .route("/alex/swap", post(handlers::execute_alex_swap))
        .layer(middleware::from_fn(move |req, next| {
            auth_middleware(
                req,
                next,
                token_for_alex.clone(),
                server_start,
                token_ttl_seconds,
            )
        }))
        .with_state(state.clone());

    // Managed SaaS Gateway subscriber surface (`/api/v1/agent/*`), bound to
    // https://api.conxian-labs.com/v1/agent in production. Multi-tenant keys or
    // Fusion JWTs are accepted; legacy institutional bearer tokens remain valid.
    let managed_routes = Router::new()
        .route(
            "/agent/capabilities",
            get(crate::capabilities::get_capabilities),
        )
        .route("/agent/settle", post(handlers::settle_job_card))
        .route("/agent/verify", post(handlers::verify_attestation))
        .route("/agent/chains", get(handlers::list_supported_chains))
        .layer(middleware::from_fn_with_state(
            managed_auth.clone(),
            managed_auth_middleware,
        ))
        .with_state(state.clone());

    // Merchant-of-Record billing webhook (Gateway #507). Mounted only when a
    // provider and signing secret are configured; the route is not advertised
    // when unconfigured. The handler verifies the HMAC over the raw body before
    // any entitlement mutation (fail closed).
    let webhook_routes = match mor_store
        .as_ref()
        .and_then(|store| mor_webhook_state_from_env(store.clone()))
    {
        Some(webhook_state) => Router::new()
            .route(
                "/billing/mor/webhook",
                post(crate::mor_webhook::mor_webhook_handler),
            )
            .with_state(webhook_state),
        None => Router::new(),
    };

    // Self-serve onboarding API (Gateway #713). The public `POST /v1/onboard`
    // is gated by the no-card onboarding token; the tenant-scoped reads/writes
    // require a managed tenant key. Mounted only when a durable store and the
    // onboarding token are configured (fail closed).
    let onboarding_routes = match onboarding_api_from_env(mor_store.clone()) {
        Some(api) => {
            let tenant_scoped = Router::new()
                .route("/onboard/{tenant_id}", get(crate::onboarding::get_onboard))
                .route(
                    "/tenants/{tenant_id}/limits",
                    get(crate::onboarding::get_limits),
                )
                .route(
                    "/tenants/{tenant_id}/keys",
                    post(crate::onboarding::rotate_key),
                )
                .route(
                    "/tenants/{tenant_id}/keys/{key_id}",
                    delete(crate::onboarding::revoke_key),
                )
                .route(
                    "/tenants/{tenant_id}/deploy",
                    post(crate::onboarding::post_deploy),
                )
                .route(
                    "/tenants/{tenant_id}/usage",
                    get(crate::onboarding::get_usage),
                )
                .layer(middleware::from_fn_with_state(
                    managed_auth.clone(),
                    managed_auth_middleware,
                ))
                .with_state(api.clone());
            Router::new()
                .route("/onboard", post(crate::onboarding::post_onboard))
                .with_state(api)
                .merge(tenant_scoped)
        }
        None => Router::new(),
    };

    Router::new()
        .route("/api/v1/version", get(|| async { conxian_core::VERSION }))
        // Standards-based, unauthenticated API discovery (Gateway #711).
        .route("/openapi.json", get(crate::discovery::get_openapi))
        .route(
            "/.well-known/api-catalog",
            get(crate::discovery::get_api_catalog),
        )
        .route(
            "/.well-known/x402",
            get(crate::discovery::get_x402_discovery),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            latency_tracker,
        ))
        .route("/metrics", get(handlers::get_prometheus_metrics))
        .nest(
            "/api/v1",
            public_routes
                .merge(private_routes)
                .merge(alex_routes)
                .merge(managed_routes)
                .merge(webhook_routes)
                .merge(onboarding_routes),
        )
        .nest("/admin/v1", admin_routes)
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB global body limit
}
