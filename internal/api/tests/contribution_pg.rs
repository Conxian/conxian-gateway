//! Real PostgreSQL conformance test for the Genesis Credits contribution store.
//!
//! Exercises the actual [`PgContributionStore`] against a disposable PostgreSQL
//! fixture — not a mock — so the schema, the per-contributor row lock, the
//! per-period cap under concurrency, the expiry-applied-once marker, and the
//! award/redemption idempotency keys are all verified end to end.
//!
//! It is `#[ignore]` by default so the general workspace test run does not
//! require a database. The dedicated `contribution-conformance` workflow runs it
//! with `--ignored` against a `postgres:17` service and a disposable database
//! supplied through `CONTRIBUTION_TEST_DATABASE_URL`. Run it explicitly with:
//!
//! ```text
//! CONTRIBUTION_TEST_DATABASE_URL=postgres://... cargo test -p conxian_api \
//!   --test contribution_pg -- --ignored
//! ```

use conxian_api::contribution_ledger::{ApplyOutcome, Contribution, ContributionStore, Redemption};
use conxian_api::contribution_store::PgContributionStore;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;

const MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../migrations/0003_contribution_ledger.sql"
));

async fn fixture_pool() -> PgPool {
    let url = std::env::var("CONTRIBUTION_TEST_DATABASE_URL").unwrap_or_else(|_| {
        panic!("CONTRIBUTION_TEST_DATABASE_URL must select a disposable test database")
    });
    assert!(
        url.contains("localhost") || url.contains("127.0.0.1"),
        "refusing to run destructive conformance against a non-local database"
    );
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .expect("connect to disposable test database");
    sqlx::raw_sql(
        "DROP TABLE IF EXISTS contribution_redeem_keys; \
         DROP TABLE IF EXISTS contribution_award_keys; \
         DROP TABLE IF EXISTS contribution_ledgers;",
    )
    .execute(&pool)
    .await
    .expect("drop prior test tables");
    sqlx::raw_sql(MIGRATION)
        .execute(&pool)
        .await
        .expect("apply contribution migration");
    pool
}

#[tokio::test]
#[ignore = "requires CONTRIBUTION_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn pg_award_round_trips_and_is_idempotent() {
    let pool = fixture_pool().await;
    let store = PgContributionStore::new(pool);

    let first = store
        .award("pg_alice", Contribution::SecurityAudit, "k-1", 1_000)
        .await
        .expect("award");
    assert!(matches!(first, ApplyOutcome::Applied(_)));
    assert_eq!(
        store.get("pg_alice").await.unwrap().unwrap().balance(),
        1_000
    );

    // A retry with the same key returns the stored result and mints nothing.
    let second = store
        .award("pg_alice", Contribution::SecurityAudit, "k-1", 1_000)
        .await
        .expect("award retry");
    assert!(matches!(second, ApplyOutcome::Duplicate(_)));
    assert_eq!(
        store.get("pg_alice").await.unwrap().unwrap().balance(),
        1_000
    );
}

#[tokio::test]
#[ignore = "requires CONTRIBUTION_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn pg_concurrent_awards_cannot_bypass_the_period_cap() {
    let pool = fixture_pool().await;
    let store = Arc::new(PgContributionStore::new(pool));
    // Each award is 2_000 credits and the cap is 20_000, so exactly ten may
    // succeed even when twelve race for the row.
    let mut handles = Vec::new();
    for i in 0..12 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store
                .award(
                    "pg_cap",
                    Contribution::EnterpriseReferral,
                    &format!("cap-{i}"),
                    5_000,
                )
                .await
        }));
    }
    let mut applied = 0;
    for h in handles {
        if matches!(h.await.unwrap(), Ok(ApplyOutcome::Applied(_))) {
            applied += 1;
        }
    }
    assert_eq!(applied, 10, "the cap admitted exactly ten awards");
    assert_eq!(
        store.get("pg_cap").await.unwrap().unwrap().balance(),
        20_000
    );
}

#[tokio::test]
#[ignore = "requires CONTRIBUTION_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn pg_redeem_is_idempotent_and_grants_once() {
    let pool = fixture_pool().await;
    let store = PgContributionStore::new(pool);
    let grants = std::sync::Mutex::new(Vec::<String>::new());
    let grant = |_amount: u64, key: &str| {
        grants.lock().unwrap().push(key.to_string());
        Ok(())
    };
    store
        .award("pg_bob", Contribution::VerifiedDisproof, "a-1", 1_000)
        .await
        .unwrap();
    let first = store
        .redeem("pg_bob", Redemption::RelayCredits, "r-1", 1_000, &grant)
        .await
        .unwrap();
    assert!(matches!(first, ApplyOutcome::Applied(_)));
    let second = store
        .redeem("pg_bob", Redemption::RelayCredits, "r-1", 1_000, &grant)
        .await
        .unwrap();
    assert!(matches!(second, ApplyOutcome::Duplicate(_)));
    assert_eq!(store.get("pg_bob").await.unwrap().unwrap().balance(), 400);
    assert_eq!(grants.lock().unwrap().len(), 1, "granted exactly once");
}

#[tokio::test]
#[ignore = "requires CONTRIBUTION_TEST_DATABASE_URL and a PostgreSQL fixture"]
async fn pg_expiry_is_applied_only_once() {
    let pool = fixture_pool().await;
    let store = PgContributionStore::new(pool);
    let grant = |_: u64, _: &str| Ok(());
    let later = 366 * 24 * 60 * 60;
    // One award expires before `later`; a second is still live there, so the
    // redemption can proceed and the expired award must be deducted once.
    store
        .award("pg_exp", Contribution::VerifiedDisproof, "e-1", 0)
        .await
        .unwrap();
    store
        .award("pg_exp", Contribution::SecurityAudit, "e-2", later)
        .await
        .unwrap();
    let first = store
        .redeem("pg_exp", Redemption::RelayCredits, "x-1", later, &grant)
        .await
        .unwrap();
    // 500 (expired, pruned once) + 1000 (live) - 100 = 900; a second expiry of
    // the same award would show here as 400.
    match first {
        ApplyOutcome::Applied(r) => assert_eq!(r.remaining, 900),
        other => panic!("expected Applied, got {other:?}"),
    }
    let second = store
        .redeem("pg_exp", Redemption::RelayCredits, "x-2", later, &grant)
        .await
        .unwrap();
    match second {
        ApplyOutcome::Applied(r) => assert_eq!(r.remaining, 800),
        other => panic!("expected Applied, got {other:?}"),
    }
}
