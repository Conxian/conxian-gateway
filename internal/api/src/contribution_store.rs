//! Neon/Postgres implementation of the Genesis Credits [`ContributionStore`].
//!
//! Uses the gateway's shared `sqlx` Postgres pool. Every method fails closed by
//! returning `Err` rather than silently succeeding, so a database outage denies
//! an award instead of minting a phantom balance.
//!
//! The `awards` column is JSONB but is bound and read as text and cast in SQL
//! (`$5::jsonb`), so the store needs no `sqlx` JSON feature and the JSON codec
//! stays the single `serde_json` used everywhere else in the crate.

use crate::contribution_ledger::{ContributionLedger, ContributionStore, CreditAward};
use sqlx::PgPool;

/// Postgres-backed Genesis Credits ledger store.
pub struct PgContributionStore {
    pool: PgPool,
}

impl PgContributionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ContributionStore for PgContributionStore {
    async fn get(&self, contributor: &str) -> Result<Option<ContributionLedger>, String> {
        let row: Option<(String, i64, i64, i64, String)> = sqlx::query_as(
            "SELECT contributor, balance, period_minted, period_started_at, awards::text \
             FROM contribution_ledgers WHERE contributor = $1",
        )
        .bind(contributor)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("select contribution ledger: {e}"))?;
        let Some((contributor, balance, period_minted, period_started_at, awards_json)) = row
        else {
            return Ok(None);
        };
        let awards: Vec<CreditAward> =
            serde_json::from_str(&awards_json).map_err(|e| format!("decode awards: {e}"))?;
        Ok(Some(ContributionLedger::restore(
            contributor,
            balance as u64,
            awards,
            period_minted as u64,
            period_started_at as u64,
        )))
    }

    async fn save(&self, ledger: &ContributionLedger) -> Result<(), String> {
        let awards =
            serde_json::to_string(ledger.awards()).map_err(|e| format!("encode awards: {e}"))?;
        sqlx::query(
            "INSERT INTO contribution_ledgers \
             (contributor, balance, period_minted, period_started_at, awards, updated_at) \
             VALUES ($1, $2, $3, $4, $5::jsonb, $6) \
             ON CONFLICT (contributor) DO UPDATE SET \
               balance = EXCLUDED.balance, period_minted = EXCLUDED.period_minted, \
               period_started_at = EXCLUDED.period_started_at, awards = EXCLUDED.awards, \
               updated_at = EXCLUDED.updated_at",
        )
        .bind(ledger.contributor())
        .bind(ledger.balance() as i64)
        .bind(ledger.period_minted() as i64)
        .bind(ledger.period_started_at() as i64)
        .bind(awards)
        .bind(crate::contribution_ledger::now_secs() as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("upsert contribution ledger: {e}"))?;
        Ok(())
    }
}
