//! Real PostgreSQL conformance test for the self-serve onboarding store
//! (Gateway #713).
//!
//! Exercises the actual [`PgOnboardingStore`] against a disposable PostgreSQL
//! fixture — not a mock — so the schema, the idempotency unique index, the JSONB
//! step round-trip, and the upsert are all verified end to end.
//!
//! It is `#[ignore]` by default so the general workspace test run does not
//! require a database. The dedicated `onboarding-conformance` workflow runs it
//! with `--ignored` against a `postgres:17` service and a disposable database
//! supplied through `ONBOARDING_TEST_DATABASE_URL`. Run it explicitly with:
//!
//! ```text
//! ONBOARDING_TEST_DATABASE_URL=postgres://... cargo test -p conxian_api \
//!   --test onboarding_pg -- --ignored
//! ```

use conxian_api::onboarding::{
    InMemoryOnboardingStore, OnboardRequest, OnboardingService, OnboardingStore, StepRecord,
    TenantRecord,
};
use conxian_api::onboarding_store::PgOnboardingStore;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;

const MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../migrations/0002_onboarding_tenants.sql"
));

async fn fixture_pool() -> PgPool {
    let url = std::env::var("ONBOARDING_TEST_DATABASE_URL").unwrap_or_else(|_| {
        panic!("ONBOARDING_TEST_DATABASE_URL must select a disposable test database")
    });
    assert!(
        url.contains("localhost") || url.contains("127.0.0.1"),
        "refusing to run destructive conformance against a non-local database"
    );
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect to disposable test database");
    sqlx::raw_sql("DROP TABLE IF EXISTS onboarding_tenants;")
        .execute(&pool)
        .await
        .expect("drop prior test table");
    sqlx::raw_sql(MIGRATION)
        .execute(&pool)
        .await
        .expect("apply onboarding migration");
    pool
}

fn record(tenant: &str, idem: &str) -> TenantRecord {
    TenantRecord {
        tenant_id: tenant.to_string(),
        idempotency_key: idem.to_string(),
        state: "active".to_string(),
        tier: "sandbox".to_string(),
        plan: "sandbox".to_string(),
        endpoint_url: Some(format!(
            "https://api.example.test/api/v1/agent?tenant={tenant}"
        )),
        key_id: Some("0123456789ab".to_string()),
        created_at: 1_700_000_000,
        updated_at: 1_700_000_100,
        steps: vec![StepRecord {
            name: "entitlement".to_string(),
            state: "done".to_string(),
            detail: None,
        }],
    }
}

#[tokio::test]
#[ignore = "requires ONBOARDING_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn pg_onboarding_store_round_trips_and_enforces_idempotency() {
    let pool = fixture_pool().await;
    let store = PgOnboardingStore::new(pool.clone());

    // A fresh idempotency key resolves to nothing.
    assert!(store.find_by_idempotency("idem-a").await.unwrap().is_none());

    // Save then read back: JSONB steps survive the round trip.
    let rec = record("t_abc", "idem-a");
    store.save(&rec).await.unwrap();
    let got = store.get("t_abc").await.unwrap().expect("row present");
    assert_eq!(got, rec);
    assert_eq!(
        store.find_by_idempotency("idem-a").await.unwrap().unwrap(),
        rec
    );

    // Upsert: a second save with a changed state updates in place (no dup row).
    let mut rec2 = rec.clone();
    rec2.state = "terminated".to_string();
    rec2.key_id = None;
    rec2.updated_at = 1_700_000_200;
    store.save(&rec2).await.unwrap();
    assert_eq!(store.get("t_abc").await.unwrap().unwrap(), rec2);

    // The idempotency key is unique: a second tenant cannot claim it.
    let clash = record("t_xyz", "idem-a");
    assert!(store.save(&clash).await.is_err());
}

#[tokio::test]
#[ignore = "requires ONBOARDING_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn pg_service_provisions_idempotently_without_a_key_store() {
    let pool = fixture_pool().await;
    let store: Arc<dyn OnboardingStore> = Arc::new(PgOnboardingStore::new(pool));
    let svc = OnboardingService::new(store, None, "https://api.example.test");

    let first = svc
        .provision(OnboardRequest {
            idempotency_key: "idem-pg".to_string(),
            tier: None,
        })
        .await
        .expect("provision");
    assert_eq!(first.record.state, "active");

    // Idempotent: same tenant, no new key.
    let second = svc
        .provision(OnboardRequest {
            idempotency_key: "idem-pg".to_string(),
            tier: None,
        })
        .await
        .expect("reprovision");
    assert_eq!(first.record.tenant_id, second.record.tenant_id);
    assert!(second.api_key.is_none());

    // The in-memory store must agree on the same contract (contract parity).
    let mem = InMemoryOnboardingStore::new();
    mem.save(&first.record).await.unwrap();
    assert_eq!(
        mem.get(&first.record.tenant_id).await.unwrap().unwrap(),
        first.record
    );
}
