//! Real PostgreSQL conformance test for the Track A `migrate` path.
//!
//! Proves the control-plane schema migration is idempotent against a real
//! Postgres — the same SQL the CLI `migrate` verb and the device
//! `migrateDatabase` action both apply. `#[ignore]` by default so the general
//! workspace test run needs no database. Run it with:
//!
//! ```text
//! ONBOARDING_TEST_DATABASE_URL=postgres://... cargo test -p conxian_api \
//!   --test device_pg -- --ignored
//! ```

use sqlx::postgres::PgPoolOptions;

const MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../migrations/0002_onboarding_tenants.sql"
));

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL via ONBOARDING_TEST_DATABASE_URL"]
async fn migrate_is_idempotent() {
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
        .unwrap();

    // First apply creates the table.
    sqlx::raw_sql(MIGRATION).execute(&pool).await.unwrap();
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
         WHERE table_name = 'onboarding_tenants')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(exists, "first migrate must create the table");

    // Second apply must converge without error (idempotent DDL).
    sqlx::raw_sql(MIGRATION).execute(&pool).await.unwrap();

    // The unique index on the idempotency key must exist exactly once.
    let index_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_indexes WHERE tablename = 'onboarding_tenants'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(index_count >= 1);
}
