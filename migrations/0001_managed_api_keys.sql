-- Managed SaaS Gateway — multi-tenant API key registry (Neon Postgres)
--
-- Minted/rotated by the Merchant-of-Record (Lemon Squeezy / Paddle) billing
-- webhook. The gateway process mirrors these keys into its in-memory
-- `TenantKeyRegistry` via `MANAGED_API_KEYS` (JSON map) or a Neon-backed
-- loader; this table is the source of truth for the billing lifecycle.

CREATE TABLE IF NOT EXISTS managed_api_keys (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       TEXT        NOT NULL,
    tier            TEXT        NOT NULL DEFAULT 'managed',
    api_key_hash    TEXT        NOT NULL,   -- SHA-256 hex of the live key (never store plaintext)
    status          TEXT        NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active', 'revoked', 'rotating')),
    plan            TEXT        NOT NULL DEFAULT 'monthly',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    rotated_at      TIMESTAMPTZ,
    revoked_at      TIMESTAMPTZ,
    UNIQUE (tenant_id)
);

-- Idempotent key rotation: revoke all prior keys for a tenant atomically.
CREATE INDEX IF NOT EXISTS idx_managed_api_keys_tenant_status
    ON managed_api_keys (tenant_id, status);

-- A key hash may map to at most one active tenant at a time.
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_api_keys_hash_active
    ON managed_api_keys (api_key_hash)
    WHERE status = 'active';

-- Billing events ledger (webhook idempotency + audit).
CREATE TABLE IF NOT EXISTS billing_webhook_events (
    id              TEXT        PRIMARY KEY, -- MoR event id (idempotency key)
    event_type      TEXT        NOT NULL,
    tenant_id       TEXT,
    payload         JSONB       NOT NULL,
    processed_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
