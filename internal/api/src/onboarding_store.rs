//! Neon/Postgres implementation of the onboarding [`OnboardingStore`].
//!
//! Uses the gateway's shared `sqlx` Postgres pool. Every method fails closed by
//! returning `Err` rather than silently succeeding, so a database outage denies
//! onboarding instead of producing a phantom tenant.
//!
//! The `steps` column is JSONB but is bound and read as text and cast in SQL
//! (`$8::jsonb`), so the store needs no `sqlx` JSON feature and the JSON codec
//! stays the single `serde_json` used everywhere else in the crate.

use crate::onboarding::{OnboardingStore, StepRecord, TenantRecord};
use sqlx::PgPool;

/// Postgres-backed onboarding tenant store.
pub struct PgOnboardingStore {
    pool: PgPool,
}

impl PgOnboardingStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type Row = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    i64,
);

fn row_to_record(r: Row) -> Result<TenantRecord, String> {
    let (
        tenant_id,
        idempotency_key,
        state,
        tier,
        plan,
        endpoint_url,
        key_id,
        steps_json,
        created_at,
        updated_at,
    ) = r;
    let steps: Vec<StepRecord> =
        serde_json::from_str(&steps_json).map_err(|e| format!("decode steps: {e}"))?;
    Ok(TenantRecord {
        tenant_id,
        idempotency_key,
        state,
        tier,
        plan,
        endpoint_url,
        key_id,
        created_at: created_at as u64,
        updated_at: updated_at as u64,
        steps,
    })
}

#[async_trait::async_trait]
impl OnboardingStore for PgOnboardingStore {
    async fn find_by_idempotency(&self, key: &str) -> Result<Option<TenantRecord>, String> {
        let row: Option<Row> = sqlx::query_as(
            "SELECT tenant_id, idempotency_key, state, tier, plan, endpoint_url, key_id, \
             steps::text, created_at, updated_at FROM onboarding_tenants \
             WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("select by idempotency: {e}"))?;
        row.map(row_to_record).transpose()
    }

    async fn get(&self, tenant_id: &str) -> Result<Option<TenantRecord>, String> {
        let row: Option<Row> = sqlx::query_as(
            "SELECT tenant_id, idempotency_key, state, tier, plan, endpoint_url, key_id, \
             steps::text, created_at, updated_at FROM onboarding_tenants \
             WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("select tenant: {e}"))?;
        row.map(row_to_record).transpose()
    }

    async fn save(&self, record: &TenantRecord) -> Result<(), String> {
        let steps =
            serde_json::to_string(&record.steps).map_err(|e| format!("encode steps: {e}"))?;
        sqlx::query(
            "INSERT INTO onboarding_tenants \
             (tenant_id, idempotency_key, state, tier, plan, endpoint_url, key_id, steps, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8::jsonb, $9, $10) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
               state = EXCLUDED.state, tier = EXCLUDED.tier, plan = EXCLUDED.plan, \
               endpoint_url = EXCLUDED.endpoint_url, key_id = EXCLUDED.key_id, \
               steps = EXCLUDED.steps, updated_at = EXCLUDED.updated_at",
        )
        .bind(&record.tenant_id)
        .bind(&record.idempotency_key)
        .bind(&record.state)
        .bind(&record.tier)
        .bind(&record.plan)
        .bind(&record.endpoint_url)
        .bind(&record.key_id)
        .bind(steps)
        .bind(record.created_at as i64)
        .bind(record.updated_at as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("upsert tenant: {e}"))?;
        Ok(())
    }
}
