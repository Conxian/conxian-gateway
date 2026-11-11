-- Genesis Credits — the utility-only contribution ledger.
--
-- Durable store for the safe gamified capital-formation mechanic (see
-- conxian-business/docs/CAPITAL_FORMATION_MODEL_2026_11_11.md). One row per
-- contributor holding the earned, non-transferable credit balance and its
-- auditable award history.
--
-- There is no monetary column and no transfer path: credits are earned for
-- verified work and redeemed only for platform utility. The balance and award
-- list are the entire state, so a restart never changes a contributor's
-- standing.
--
-- The mutation contract is serialized per contributor (SELECT ... FOR UPDATE on
-- the ledger row) and idempotent (the `_keys` tables), so concurrent awards
-- cannot lose an update or bypass the per-period cap, and a retried request
-- cannot mint or spend twice.
--
-- Redemption uses a transactional-outbox shape: the debit and the
-- `contribution_redeem_keys` row are committed together (grant_applied = false),
-- then the relay-credit grant is applied and the row marked applied. A retry
-- that finds grant_applied = false re-applies the grant, so a crash between the
-- debit commit and the grant is recoverable rather than an unreconciled grant.

CREATE TABLE IF NOT EXISTS contribution_ledgers (
    contributor       TEXT   PRIMARY KEY,
    balance           BIGINT NOT NULL DEFAULT 0,
    period_minted     BIGINT NOT NULL DEFAULT 0,
    period_started_at BIGINT NOT NULL DEFAULT 0,
    awards            JSONB  NOT NULL DEFAULT '[]'::jsonb,
    updated_at        BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_contribution_ledgers_updated_at
    ON contribution_ledgers (updated_at);

-- One row per applied award, keyed by the client idempotency key. The stored
-- result is returned verbatim on a retry, so the mint happens exactly once.
CREATE TABLE IF NOT EXISTS contribution_award_keys (
    idempotency_key TEXT   PRIMARY KEY,
    contributor     TEXT   NOT NULL,
    result          JSONB  NOT NULL,
    created_at      BIGINT NOT NULL
);

-- One row per applied redemption. The debit is committed with this row; the
-- relay-credit grant is applied afterwards and `grant_applied` is set. A retry
-- with the same key returns the stored receipt and re-applies the grant only
-- when `grant_applied` is still false.
CREATE TABLE IF NOT EXISTS contribution_redeem_keys (
    idempotency_key TEXT    PRIMARY KEY,
    contributor     TEXT    NOT NULL,
    result          JSONB   NOT NULL,
    grant_applied   BOOLEAN NOT NULL DEFAULT false,
    created_at      BIGINT  NOT NULL
);
