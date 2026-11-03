//! Neon/Postgres implementation of the MoR webhook [`KeyStore`].
//!
//! Uses the gateway's shared `sqlx` Postgres pool. All statements are
//! parameterized; no key plaintext is ever sent to the database, only its
//! SHA-256 hex hash. Every method fails closed by returning `Err` rather than
//! silently succeeding.

use crate::mor_webhook::{KeyStore, StoredKey};
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
}
