//! Neon/Postgres implementation of the Genesis Credits [`ContributionStore`].
//!
//! Uses the gateway's shared `sqlx` Postgres pool. Every mutation runs inside
//! one transaction that locks the contributor's ledger row (`SELECT ... FOR
//! UPDATE`), so a concurrent award cannot lose an update or bypass the
//! per-period cap. Each mutation is keyed by a client idempotency key: the
//! applied result is recorded in the same transaction, so a retry returns the
//! prior result and moves no credit. Every method fails closed by returning
//! `Err` rather than silently succeeding.
//!
//! The JSONB columns are bound and read as text and cast in SQL (`$n::jsonb`),
//! so the store needs no `sqlx` JSON feature and the JSON codec stays the single
//! `serde_json` used everywhere else in the crate.

use crate::contribution_ledger::{
    ApplyOutcome, Contribution, ContributionLedger, ContributionStore, ContributionSummary,
    CreditAward, Redemption, RedemptionReceipt,
};
use sqlx::{PgPool, Postgres, Transaction};

/// Postgres-backed Genesis Credits ledger store.
pub struct PgContributionStore {
    pool: PgPool,
}

impl PgContributionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Locks the contributor's ledger row and returns it, inserting a zero-balance
/// row first when none exists. The row lock serializes concurrent mutations for
/// the same contributor, so the read-modify-write below is atomic.
async fn lock_ledger(
    tx: &mut Transaction<'_, Postgres>,
    contributor: &str,
    now: u64,
) -> Result<ContributionLedger, String> {
    sqlx::query(
        "INSERT INTO contribution_ledgers (contributor, period_started_at, updated_at) \
         VALUES ($1, $2, $2) ON CONFLICT (contributor) DO NOTHING",
    )
    .bind(contributor)
    .bind(now as i64)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("ensure contribution ledger row: {e}"))?;

    let row: (i64, i64, i64, String) = sqlx::query_as(
        "SELECT balance, period_minted, period_started_at, awards::text \
         FROM contribution_ledgers WHERE contributor = $1 FOR UPDATE",
    )
    .bind(contributor)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| format!("lock contribution ledger: {e}"))?;
    let (balance, period_minted, period_started_at, awards_json) = row;
    let awards: Vec<CreditAward> =
        serde_json::from_str(&awards_json).map_err(|e| format!("decode awards: {e}"))?;
    Ok(ContributionLedger::restore(
        contributor,
        balance as u64,
        awards,
        period_minted as u64,
        period_started_at as u64,
    ))
}

async fn persist_ledger(
    tx: &mut Transaction<'_, Postgres>,
    ledger: &ContributionLedger,
    now: u64,
) -> Result<(), String> {
    let awards =
        serde_json::to_string(ledger.awards()).map_err(|e| format!("encode awards: {e}"))?;
    sqlx::query(
        "UPDATE contribution_ledgers SET balance = $2, period_minted = $3, \
         period_started_at = $4, awards = $5::jsonb, updated_at = $6 WHERE contributor = $1",
    )
    .bind(ledger.contributor())
    .bind(ledger.balance() as i64)
    .bind(ledger.period_minted() as i64)
    .bind(ledger.period_started_at() as i64)
    .bind(awards)
    .bind(now as i64)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("persist contribution ledger: {e}"))?;
    Ok(())
}

#[async_trait::async_trait]
impl ContributionStore for PgContributionStore {
    async fn get(&self, contributor: &str) -> Result<Option<ContributionLedger>, String> {
        let row: Option<(i64, i64, i64, String)> = sqlx::query_as(
            "SELECT balance, period_minted, period_started_at, awards::text \
             FROM contribution_ledgers WHERE contributor = $1",
        )
        .bind(contributor)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("select contribution ledger: {e}"))?;
        let Some((balance, period_minted, period_started_at, awards_json)) = row else {
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

    async fn award(
        &self,
        contributor: &str,
        contribution: Contribution,
        idempotency_key: &str,
        now: u64,
    ) -> Result<ApplyOutcome<ContributionSummary>, String> {
        if idempotency_key.is_empty() {
            return Err("idempotency key must not be empty".to_string());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("begin award tx: {e}"))?;

        // Lock the ledger row first so the idempotency check and the award are
        // one serialized critical section: two concurrent requests with the same
        // key cannot both pass the gate, and concurrent distinct awards cannot
        // lose an update or bypass the cap.
        let mut ledger = lock_ledger(&mut tx, contributor, now).await?;
        let prior: Option<(String,)> = sqlx::query_as(
            "SELECT result::text FROM contribution_award_keys WHERE idempotency_key = $1",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("lookup award key: {e}"))?;
        if let Some((json,)) = prior {
            let summary: ContributionSummary =
                serde_json::from_str(&json).map_err(|e| format!("decode award result: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit award tx: {e}"))?;
            return Ok(ApplyOutcome::Duplicate(summary));
        }

        ledger
            .award(contribution, now)
            .map_err(|e| format!("{e:?}"))?;
        let summary = ledger.summary(now);
        persist_ledger(&mut tx, &ledger, now).await?;
        let result_json =
            serde_json::to_string(&summary).map_err(|e| format!("encode award result: {e}"))?;
        sqlx::query(
            "INSERT INTO contribution_award_keys (idempotency_key, contributor, result, created_at) \
             VALUES ($1, $2, $3::jsonb, $4)",
        )
        .bind(idempotency_key)
        .bind(contributor)
        .bind(result_json)
        .bind(now as i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("record award key: {e}"))?;
        tx.commit()
            .await
            .map_err(|e| format!("commit award tx: {e}"))?;
        Ok(ApplyOutcome::Applied(summary))
    }

    async fn redeem(
        &self,
        contributor: &str,
        redemption: Redemption,
        idempotency_key: &str,
        now: u64,
        grant: &(dyn for<'a> Fn(u64, &'a str) -> Result<(), String> + Send + Sync),
    ) -> Result<ApplyOutcome<RedemptionReceipt>, String> {
        if idempotency_key.is_empty() {
            return Err("idempotency key must not be empty".to_string());
        }

        // Phase 1: lock the ledger row first so the idempotency check, the debit
        // and the outbox insert are one serialized critical section. This makes
        // the read-modify-write atomic (concurrent redemptions cannot over-spend)
        // and makes a same-key retry see the prior row.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("begin redeem tx: {e}"))?;
        let mut ledger = lock_ledger(&mut tx, contributor, now).await?;
        let prior: Option<(String, bool)> = sqlx::query_as(
            "SELECT result::text, grant_applied FROM contribution_redeem_keys \
             WHERE idempotency_key = $1",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("lookup redeem key: {e}"))?;
        let (receipt, grant_applied) = match prior {
            Some((json, applied)) => {
                let receipt: RedemptionReceipt = serde_json::from_str(&json)
                    .map_err(|e| format!("decode redeem result: {e}"))?;
                tx.commit()
                    .await
                    .map_err(|e| format!("commit redeem tx: {e}"))?;
                if applied {
                    return Ok(ApplyOutcome::Duplicate(receipt));
                }
                // A prior attempt committed the debit but not the grant; fall
                // through to re-apply the grant against the same key.
                (receipt, false)
            }
            None => {
                let receipt = ledger
                    .redeem(redemption, now)
                    .map_err(|e| format!("{e:?}"))?;
                persist_ledger(&mut tx, &ledger, now).await?;
                let result_json = serde_json::to_string(&receipt)
                    .map_err(|e| format!("encode redeem result: {e}"))?;
                sqlx::query(
                    "INSERT INTO contribution_redeem_keys \
                     (idempotency_key, contributor, result, grant_applied, created_at) \
                     VALUES ($1, $2, $3::jsonb, false, $4)",
                )
                .bind(idempotency_key)
                .bind(contributor)
                .bind(result_json)
                .bind(now as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("record redeem key: {e}"))?;
                tx.commit()
                    .await
                    .map_err(|e| format!("commit redeem tx: {e}"))?;
                (receipt, false)
            }
        };

        // Phase 2: apply the relay-credit grant (idempotent on the key) and mark
        // the outbox row applied. A failure here leaves `grant_applied = false`,
        // so a retry re-applies it rather than losing the grant.
        if receipt.relay_credits > 0 && !grant_applied {
            grant(receipt.relay_credits, idempotency_key)?;
        }
        if !grant_applied {
            sqlx::query(
                "UPDATE contribution_redeem_keys SET grant_applied = true \
                 WHERE idempotency_key = $1",
            )
            .bind(idempotency_key)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("mark redeem grant applied: {e}"))?;
        }
        Ok(ApplyOutcome::Applied(receipt))
    }
}
