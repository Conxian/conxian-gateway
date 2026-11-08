-- Self-serve onboarding tenants (Gateway #713).
--
-- Durable store for the onboarding lane: one row per tenant, keyed by a
-- client-supplied idempotency key so a retried POST /v1/onboard returns the
-- existing receipt instead of provisioning a second tenant. The per-step state
-- is stored as JSONB so a partial provision is resumable.
--
-- The API key itself is never stored here; it lives (as a SHA-256 hash) in
-- managed_api_keys, minted through the shared KeyStore so a key issued here
-- authenticates through the same managed-auth path as a webhook-minted key.

CREATE TABLE IF NOT EXISTS onboarding_tenants (
    tenant_id       TEXT PRIMARY KEY,
    idempotency_key TEXT        NOT NULL UNIQUE,
    state           TEXT        NOT NULL DEFAULT 'requested'
                    CHECK (state IN ('requested','provisioning','active',
                                     'failed','suspended','terminated')),
    tier            TEXT        NOT NULL DEFAULT 'sandbox',
    plan            TEXT        NOT NULL DEFAULT 'sandbox',
    endpoint_url    TEXT,
    key_id          TEXT,
    steps           JSONB       NOT NULL DEFAULT '[]'::jsonb,
    created_at      BIGINT      NOT NULL,
    updated_at      BIGINT      NOT NULL
);

-- The idempotency lookup is the hot path for a retried onboard call.
CREATE UNIQUE INDEX IF NOT EXISTS idx_onboarding_tenants_idempotency
    ON onboarding_tenants (idempotency_key);
