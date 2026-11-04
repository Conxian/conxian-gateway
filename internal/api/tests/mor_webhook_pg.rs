//! Real PostgreSQL conformance test for the MoR webhook key store (Gateway #507).
//!
//! This exercises the actual [`PgKeyStore`] against a disposable PostgreSQL
//! fixture — not a mock — so the SQL, the partial unique index that enforces a
//! single active key per tenant, the idempotency ledger, and the mint/rotate/
//! revoke lifecycle are all verified end to end.
//!
//! It is `#[ignore]` by default so the general workspace test run does not
//! require a database. The dedicated `mor-webhook-conformance` workflow runs it
//! with `--ignored` against a `postgres:17` service and a disposable database
//! supplied through `MOR_WEBHOOK_TEST_DATABASE_URL`. Run it explicitly with:
//!
//! ```text
//! MOR_WEBHOOK_TEST_DATABASE_URL=postgres://... cargo test -p conxian_api \
//!   --test mor_webhook_pg -- --ignored
//! ```

use conxian_api::mor_webhook::{apply_event, ApplyOutcome, KeyAction, KeyStore, SubscriptionEvent};
use conxian_api::mor_webhook_store::PgKeyStore;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

const MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../migrations/0001_managed_api_keys.sql"
));

/// Connects to the disposable fixture database. Panics with a clear message
/// when the required variable is absent so the test can never silently pass.
async fn fixture_pool() -> PgPool {
    let url = std::env::var("MOR_WEBHOOK_TEST_DATABASE_URL").unwrap_or_else(|_| {
        panic!("MOR_WEBHOOK_TEST_DATABASE_URL must select a disposable test database")
    });
    assert!(
        !url.contains("sslmode=require") || url.contains("localhost") || url.contains("127.0.0.1"),
        "refusing to run destructive conformance against a non-local database"
    );
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect to disposable test database");
    // Start from a known-clean schema in this disposable fixture only.
    sqlx::raw_sql(
        "DROP TABLE IF EXISTS managed_api_keys; DROP TABLE IF EXISTS billing_webhook_events;",
    )
    .execute(&pool)
    .await
    .expect("drop prior test tables");
    sqlx::raw_sql(MIGRATION)
        .execute(&pool)
        .await
        .expect("apply managed_api_keys migration");
    pool
}

fn event(id: &str, tenant: &str, plan: &str, action: KeyAction) -> SubscriptionEvent {
    SubscriptionEvent {
        event_id: id.to_string(),
        tenant_id: tenant.to_string(),
        plan: plan.to_string(),
        action,
    }
}

#[tokio::test]
#[ignore = "requires MOR_WEBHOOK_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn pg_key_store_mint_rotate_revoke_and_idempotency() {
    let pool = fixture_pool().await;
    let store = PgKeyStore::new(pool.clone());
    let tenant = "tenant-conformance-1";

    // Mint
    let outcome = apply_event(&store, &event("evt-mint", tenant, "indie", KeyAction::Mint))
        .await
        .expect("mint");
    let ApplyOutcome::Minted { key } = outcome else {
        panic!("expected mint, got {outcome:?}");
    };
    assert!(key.starts_with("cxn_agent_"));
    let active = store.active_key(tenant).await.unwrap().expect("active key");
    assert_eq!(active.tier, "indie");
    assert_eq!(
        active.api_key_hash,
        hex::encode(Sha256::digest(key.as_bytes()))
    );
    // Only the hash is persisted — never the plaintext.
    let plaintext_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM managed_api_keys WHERE api_key_hash = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(plaintext_rows, 0, "plaintext key must never be stored");

    // Replay of the same event id is a no-op.
    assert_eq!(
        apply_event(&store, &event("evt-mint", tenant, "indie", KeyAction::Mint))
            .await
            .unwrap(),
        ApplyOutcome::Duplicate
    );

    // Rotate: new active key, exactly one active row remains.
    let ApplyOutcome::Rotated { key: key2 } = apply_event(
        &store,
        &event("evt-rotate", tenant, "business", KeyAction::Rotate),
    )
    .await
    .unwrap() else {
        panic!("expected rotate");
    };
    assert_ne!(key, key2);
    let active = store
        .active_key(tenant)
        .await
        .unwrap()
        .expect("rotated key");
    assert_eq!(active.tier, "business");
    assert_eq!(
        active.api_key_hash,
        hex::encode(Sha256::digest(key2.as_bytes()))
    );
    let active_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM managed_api_keys WHERE tenant_id = $1 AND status = 'active'",
    )
    .bind(tenant)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_count, 1, "exactly one active key per tenant");

    // Revoke: no active key remains.
    assert_eq!(
        apply_event(
            &store,
            &event("evt-revoke", tenant, "business", KeyAction::Revoke)
        )
        .await
        .unwrap(),
        ApplyOutcome::Revoked
    );
    assert!(store.active_key(tenant).await.unwrap().is_none());

    // The idempotency ledger recorded every distinct event exactly once.
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_webhook_events WHERE tenant_id = $1")
            .bind(tenant)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(events, 3);
}

#[tokio::test]
#[ignore = "requires MOR_WEBHOOK_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn pg_key_store_fails_closed_when_tables_are_absent() {
    use sqlx::postgres::PgConnectOptions;
    use std::str::FromStr;

    let url = std::env::var("MOR_WEBHOOK_TEST_DATABASE_URL").unwrap_or_else(|_| {
        panic!("MOR_WEBHOOK_TEST_DATABASE_URL must select a disposable test database")
    });
    // Create an empty schema, then build a store whose *every* pooled connection
    // uses that schema via the startup `search_path` option. A session-scoped
    // `SET search_path` is unreliable because the pool may hand out a different
    // connection — the exact failure this test guards against.
    let bootstrap = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect");
    sqlx::raw_sql(
        "DROP SCHEMA IF EXISTS mor_absent_fixture CASCADE; CREATE SCHEMA mor_absent_fixture",
    )
    .execute(&bootstrap)
    .await
    .unwrap();
    bootstrap.close().await;

    let options = PgConnectOptions::from_str(&url)
        .expect("parse database URL")
        .options([("search_path", "mor_absent_fixture")]);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .expect("connect with search_path option");
    let store = PgKeyStore::new(pool.clone());

    // The store must return Err, never a fabricated success. This proves a
    // misconfigured database cannot grant entitlement.
    assert!(store.active_key("t-absent").await.is_err());
    assert!(store.record_event("evt", "Mint", "t-absent").await.is_err());
    assert!(store.mint_key("t-absent", "hash", "indie").await.is_err());
}

#[tokio::test]
#[ignore = "requires MOR_WEBHOOK_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn failed_provisioning_rolls_back_event_for_provider_retry() {
    let pool = fixture_pool().await;
    sqlx::query(
        "ALTER TABLE managed_api_keys ADD CONSTRAINT reject_fixture_tier \
         CHECK (tier <> 'force-failure')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let store = PgKeyStore::new(pool.clone());
    let failed = event("evt-retryable", "tenant-retry", "force-failure", KeyAction::Mint);

    assert!(apply_event(&store, &failed).await.is_err());
    let recorded: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_webhook_events WHERE id = $1")
            .bind("evt-retryable")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recorded, 0, "failed mutation must not consume the event id");

    sqlx::query("ALTER TABLE managed_api_keys DROP CONSTRAINT reject_fixture_tier")
        .execute(&pool)
        .await
        .unwrap();
    let retry = event("evt-retryable", "tenant-retry", "business", KeyAction::Mint);
    assert!(matches!(
        apply_event(&store, &retry).await.unwrap(),
        ApplyOutcome::Minted { .. }
    ));
}
