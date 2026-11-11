//! Neon/Postgres implementation of the MoR webhook [`KeyStore`].
//!
//! Uses the gateway's shared `sqlx` Postgres pool. All statements are
//! parameterized; no key plaintext is ever sent to the database, only its
//! SHA-256 hex hash. Every method fails closed by returning `Err` rather than
//! silently succeeding.

use crate::mor_webhook::{
    mint_material, ApplyOutcome, KeyAction, KeyStore, StoredKey, SubscriptionEvent,
};
use sqlx::PgPool;

/// Postgres-backed key store and idempotency ledger.
pub struct PgKeyStore {
    pool: PgPool,
}

impl PgKeyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl KeyStore for PgKeyStore {
    async fn apply_event_atomic(&self, event: &SubscriptionEvent) -> Result<ApplyOutcome, String> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("begin event tx: {e}"))?;

        let inserted = sqlx::query(
            "INSERT INTO billing_webhook_events (id, event_type, tenant_id, payload) \
             VALUES ($1, $2, $3, $4::jsonb) ON CONFLICT (id) DO NOTHING",
        )
        .bind(&event.event_id)
        .bind(format!("{:?}", event.action))
        .bind(&event.tenant_id)
        .bind("{}")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("record event: {e}"))?;
        if inserted.rows_affected() == 0 {
            tx.rollback()
                .await
                .map_err(|e| format!("rollback duplicate event: {e}"))?;
            return Ok(ApplyOutcome::Duplicate);
        }

        let outcome = match event.action {
            KeyAction::Mint | KeyAction::Rotate => {
                let (key, hash) = mint_material();
                sqlx::query(
                    "UPDATE managed_api_keys SET status = 'rotating', rotated_at = now() \
                     WHERE tenant_id = $1 AND status = 'active'",
                )
                .bind(&event.tenant_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("retire previous key: {e}"))?;
                sqlx::query(
                    "INSERT INTO managed_api_keys (tenant_id, tier, api_key_hash, status) \
                     VALUES ($1, $2, $3, 'active')",
                )
                .bind(&event.tenant_id)
                .bind(&event.plan)
                .bind(&hash)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert key: {e}"))?;
                match event.action {
                    KeyAction::Mint => ApplyOutcome::Minted { key },
                    KeyAction::Rotate => ApplyOutcome::Rotated { key },
                    KeyAction::Revoke => unreachable!(),
                }
            }
            KeyAction::Revoke => {
                sqlx::query(
                    "UPDATE managed_api_keys SET status = 'revoked', revoked_at = now() \
                     WHERE tenant_id = $1 AND status = 'active'",
                )
                .bind(&event.tenant_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("revoke key: {e}"))?;
                ApplyOutcome::Revoked
            }
        };

        tx.commit()
            .await
            .map_err(|e| format!("commit event tx: {e}"))?;
        Ok(outcome)
    }

    async fn record_event(
        &self,
        event_id: &str,
        event_type: &str,
        tenant_id: &str,
    ) -> Result<bool, String> {
        // ON CONFLICT DO NOTHING returns 0 affected rows on replay; the caller
        // treats that as an idempotent no-op.
        let result = sqlx::query(
            "INSERT INTO billing_webhook_events (id, event_type, tenant_id, payload) \
             VALUES ($1, $2, $3, $4::jsonb) ON CONFLICT (id) DO NOTHING",
        )
        .bind(event_id)
        .bind(event_type)
        .bind(tenant_id)
        .bind("{}")
        .execute(&self.pool)
        .await
        .map_err(|e| format!("record event: {e}"))?;
        Ok(result.rows_affected() == 1)
    }

    async fn revoke_active(&self, tenant_id: &str) -> Result<u64, String> {
        let result = sqlx::query(
            "UPDATE managed_api_keys SET status = 'revoked', revoked_at = now() \
             WHERE tenant_id = $1 AND status = 'active'",
        )
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("revoke key: {e}"))?;
        Ok(result.rows_affected())
    }

    async fn mint_key(
        &self,
        tenant_id: &str,
        api_key_hash: &str,
        tier: &str,
    ) -> Result<(), String> {
        // Retire the previous active key, then insert the new one. The partial
        // unique index on (tenant_id) WHERE status='active' guarantees at most
        // one active key per tenant even under concurrency.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("begin tx: {e}"))?;
        sqlx::query(
            "UPDATE managed_api_keys SET status = 'rotating', rotated_at = now() \
             WHERE tenant_id = $1 AND status = 'active'",
        )
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("retire previous key: {e}"))?;
        sqlx::query(
            "INSERT INTO managed_api_keys (tenant_id, tier, api_key_hash, status) \
             VALUES ($1, $2, $3, 'active')",
        )
        .bind(tenant_id)
        .bind(tier)
        .bind(api_key_hash)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("insert key: {e}"))?;
        tx.commit().await.map_err(|e| format!("commit: {e}"))?;
        Ok(())
    }

    async fn active_key(&self, tenant_id: &str) -> Result<Option<StoredKey>, String> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT tenant_id, api_key_hash, tier FROM managed_api_keys \
             WHERE tenant_id = $1 AND status = 'active' \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("select active key: {e}"))?;
        Ok(row.map(|(tenant_id, api_key_hash, tier)| StoredKey {
            tenant_id,
            api_key_hash,
            tier,
        }))
    }

    async fn active_key_by_hash(&self, api_key_hash: &str) -> Result<Option<StoredKey>, String> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT tenant_id, api_key_hash, tier FROM managed_api_keys \
             WHERE api_key_hash = $1 AND status = 'active' LIMIT 1",
        )
        .bind(api_key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("select active key by hash: {e}"))?;
        Ok(row.map(|(tenant_id, api_key_hash, tier)| StoredKey {
            tenant_id,
            api_key_hash,
            tier,
        }))
    }
}

/// The idempotent schema migrations for the MoR/onboarding stores.
///
/// Both files use `CREATE TABLE IF NOT EXISTS` / `IF NOT EXISTS` throughout, so
/// applying them on every startup is safe and converges the schema without a
/// separate migration tool.
pub const MANAGED_API_KEYS_MIGRATION: &str =
    include_str!("../../../migrations/0001_managed_api_keys.sql");
pub const ONBOARDING_TENANTS_MIGRATION: &str =
    include_str!("../../../migrations/0002_onboarding_tenants.sql");
pub const CONTRIBUTION_LEDGER_MIGRATION: &str =
    include_str!("../../../migrations/0003_contribution_ledger.sql");

/// Applies the idempotent schema migrations to `pool`.
///
/// Call once at startup when `MOR_WEBHOOK_DATABASE_URL` is configured so the
/// durable stores have their tables before any webhook, managed-auth lookup or
/// onboarding request is served. Fails closed: an error is returned to the
/// caller rather than swallowed, so a half-provisioned schema is never served.
pub async fn run_migrations(pool: &PgPool) -> Result<(), String> {
    for (label, sql) in [
        ("0001_managed_api_keys", MANAGED_API_KEYS_MIGRATION),
        ("0002_onboarding_tenants", ONBOARDING_TENANTS_MIGRATION),
        ("0003_contribution_ledger", CONTRIBUTION_LEDGER_MIGRATION),
    ] {
        sqlx::raw_sql(sql)
            .execute(pool)
            .await
            .map_err(|e| format!("apply migration {label}: {e}"))?;
    }
    Ok(())
}
