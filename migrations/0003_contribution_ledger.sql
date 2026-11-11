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

CREATE TABLE IF NOT EXISTS contribution_ledgers (
    contributor     TEXT   PRIMARY KEY,
    balance         BIGINT NOT NULL DEFAULT 0,
    period_minted   BIGINT NOT NULL DEFAULT 0,
    period_started_at BIGINT NOT NULL DEFAULT 0,
    awards          JSONB  NOT NULL DEFAULT '[]'::jsonb,
    updated_at      BIGINT NOT NULL DEFAULT 0
);

-- The per-period minted total and its window are the only mutable accounting
-- fields; the awards list is append-only history.
CREATE INDEX IF NOT EXISTS idx_contribution_ledgers_updated_at
    ON contribution_ledgers (updated_at);
