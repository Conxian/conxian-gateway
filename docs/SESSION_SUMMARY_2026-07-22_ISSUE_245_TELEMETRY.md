# Session Summary — Issue #245 Phase 4 Tracked Mempool Telemetry

**Date:** 2026-07-22
**Repository:** `Conxian/conxian-gateway`
**Issue:** [#245 — Evaluate impact on routing and fee markets](https://github.com/Conxian/conxian-gateway/issues/245)
**Prior audit:** [PR #275](https://github.com/Conxian/conxian-gateway/pull/275)
**Working branch:** `charlie/issue-245-tracked-mempool-telemetry`
**Verified base:** [`d7032ab621ad038f247566f820ac664a6c8c071c`](https://github.com/Conxian/conxian-gateway/commit/d7032ab621ad038f247566f820ac664a6c8c071c)

This summary records the bounded Phase 4 implementation context. It does not
claim that the issue is closed, that the branch is merged, or that the full
repository verification protocol has completed.

## Continuity and decision scorecard

- `git pull --ff-only origin main` completed successfully before branching;
  `main` was clean and synchronized at the verified base above.
- Prior issue #245 audit artifacts were present, including
  `docs/SESSION_SUMMARY_2026-07-22_ISSUE_245_AUDIT_AND_CI_222.md`, the dated
  gap analysis, cross-repository status, and BIP-110 evidence ledger.
- PR #275 was verified merged. No remote branch with the requested Phase 4
  name existed, so the exact requested branch name was created from `main`.
- The safe implementation decision is **observe persisted Gateway state,
  never infer network state**:

| Decision | Outcome |
|---|---|
| Ownership | Gateway aggregates only persisted `TrackedMempoolTx` records; Core, Wallet, and Nexus boundaries remain unchanged |
| API | Add an authenticated, schema-versioned read-only endpoint under `/api/v1` |
| Empty/error semantics | Empty tracked state is explicit and is not network-wide zero; unavailable persistence is not converted into zeros |
| Metrics | Use stable Gateway-scoped names and closed status/strategy labels; omit identifiers, addresses, node IDs, route IDs, and free-form errors |
| Fee behavior | No changes to fee-bump decisions, transaction construction, signing, broadcast, or recommendations |

## Implementation

- `internal/api/src/mempool_telemetry.rs`
  - Adds `MempoolTelemetryResponse`, status and strategy aggregates, pure
    `aggregate_tracked_mempool_transactions`, and bounded Prometheus rendering.
  - Reports schema version `1`, scope `gateway_tracked_transactions`,
    `network_mempool_observation: "not_configured"`, explicit empty semantics,
    every current `MempoolTxStatus`, replaceable/CPFP totals, current persisted
    attempt sums, current last-strategy observations, and a nullable timestamp
    derived from persisted evaluation/bump fields.
- `internal/api/src/routes.rs` and `internal/api/src/handlers.rs`
  - Add authenticated `GET /api/v1/bitcoin/mempool/telemetry`.
  - Extend public `/metrics` with the same bounded tracked-state aggregates.
- `internal/api/src/lib.rs` and `cmd/gateway/src/main.rs`
  - Wire the existing persistence backend into `AppState` without changing the
    orchestrator or listener decisions. Lightweight test harnesses may leave
    the backend unconfigured; production wiring supplies `FilePersistence`.
- `cmd/gateway/tests/api_tests.rs`
  - Adds route/auth coverage with a static persisted state and verifies that
    transaction IDs and free-form errors are not returned.
- Documentation updated:
  - `docs/research/BIP110_FEE_MARKET_AND_ROUTING_2026-07-22.md`
  - `docs/GAP_ANALYSIS_2026-07-22.md`
  - `docs/CROSS_REPO_STATUS.md`
  - `docs/research/KNOWLEDGE_MAP.md`

The BIP-110 evidence ledger now records the current Gateway base lineage, the
Phase 4 slice, explicit non-claims, and the qualified finding that stock
Bitcoin Core 31.0 at inspected source commit
`a2e074d66ac17ca7907909bbbb563e77185a45e5` contains no `REDUCED_DATA`
deployment; Core PRs #34929/#34930 remain closed and unmerged, and BIP
registry `Complete` remains distinct from activation.

## Verification performed

The following checks were run during this session:

- `cargo check -p conxian_api` — pass.
- `cargo check -p gateway --tests` — pass.
- `cargo clippy -p conxian_api --all-targets --all-features -- -D warnings` —
  pass.
- `cargo test -p conxian_api` — **43 passed**.
- `cargo test -p conxian_api mempool_telemetry -- --nocapture` — **8 passed**.
- `cargo test -p gateway --test api_tests mempool_telemetry -- --nocapture` —
  **2 passed**.
- `cargo test -p gateway --test api_tests` — **47 passed**.
- `cargo test -p gateway --tests` — all Gateway unit/integration targets passed
  (93 tests across the target set).
- `cargo fmt --all -- --check` — pass after formatting.
- `git diff --check` — pass.
- `python3 scripts/verify_contamination_guard.py` — pass; 62 production files
  scanned.

The full workspace clippy/test suites, mock-integration suite, Node suite,
health probe, and release/security checks have **not** been run in this Phase 4
session. Do not treat this summary as full verification.

## Remaining work and risks

- The issue remains open for Core/node deployment and preflight provenance,
  node/network mempool and fee telemetry, block/backlog quantiles, durable
  RBF/CPFP outcome history, route-confidence calibration, and any future fee
  model acceptance evidence.
- The persisted fields do not support historical per-strategy attempt totals;
  the implementation intentionally reports only the sum of current
  `bump_attempts` fields and current `last_bump_strategy` observations.
- The endpoint is unavailable if production persistence is not configured or
  cannot be loaded; the metrics surface exposes availability rather than
  fabricating aggregate zeros.
- A follow-up should run the mandatory repository verification protocol and
  inspect the final diff for scope and secret leakage before merge.

## Next-session commands

```bash
git pull --ff-only origin main
git fetch origin
git status --short --branch
git log --oneline -5
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace --features mock-integrations
pnpm install && pnpm build && pnpm test
python3 scripts/verify_contamination_guard.py
```

Re-check the branch/PR state and confirm the current `origin/main` SHA before
relying on any dated cross-repository snapshot. The final implementation commit
is intentionally not self-referenced here; resolve it with
`git log -1 --format=%H -- docs/SESSION_SUMMARY_2026-07-22_ISSUE_245_TELEMETRY.md`
after the branch is committed.
