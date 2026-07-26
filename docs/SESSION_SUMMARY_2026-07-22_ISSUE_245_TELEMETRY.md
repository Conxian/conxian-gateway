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
- **Historical state as of July 22, 2026:** `cmd/gateway/src/persistence.rs`
  - `FilePersistence` now serializes in-process `load()`/`save()` calls through
    a backend-owned mutex, writes each save to a unique temporary file in the
    state-file directory, flushes and syncs that file, atomically renames it
    over the state path, best-effort syncs the parent directory, and removes a
    temporary file if the save fails. Missing files still load as the default
    `PersistentState`.
- `internal/api/src/handlers.rs`
  - The telemetry JSON and Prometheus handlers run persistence `load()` calls
    with `tokio::task::spawn_blocking`, preserving the stable 503 error codes
    while keeping synchronous filesystem I/O off the async executor.
- `cmd/gateway/tests/api_tests.rs`
  - Adds route coverage for missing and failing persistence, persisted and
    unavailable Prometheus telemetry, content type, closed status/strategy
    samples, and omission of transaction IDs, aggregate zeros, and free-form
    backend errors.

The July 22 persistence hardening was intentionally bounded: the mutex coordinates
calls sharing the same `FilePersistence` instance within one process, and the
rename provides atomic replacement of the state path on the supported Unix
deployment. It is not a multi-process writer-coordination layer and does not
make a caller's separate `load` → modify → `save` sequence transactional.
Current CON-1548 guarantees are documented in `docs/PERSISTENCE_TOPOLOGY.md`.
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

The bounded remediation follow-up also passed:

- `cargo test -p gateway --bin gateway persistence -- --nocapture` — **4
  passed**, including missing-file behavior, temporary-file cleanup, atomic
  replacement, and concurrent complete-state loads.
- `cargo test -p gateway --test api_tests mempool_telemetry -- --nocapture` —
  **4 passed**, including stable missing/failing persistence 503 responses.
- `cargo test -p gateway --test api_tests prometheus_metrics -- --nocapture` —
  **4 passed**, including persisted and unavailable route-level metrics.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass.
- `cargo test --workspace` — pass.
- `cargo test --workspace --features mock-integrations` — pass.
- `pnpm install && pnpm build && pnpm test` — pass across the Node workspaces;
  the existing Next.js middleware deprecation and test-server auth-secret
  warnings did not fail the build or tests.
- Simulated gateway startup from `/tmp` plus
  `GET http://127.0.0.1:18080/api/v1/health` — HTTP 200 with `status: "ok"`.

Release/security checks outside the required contamination guard were not
part of this bounded follow-up.

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
- **Historical July 22 limitation:** `FilePersistence` protected only same-process calls through the shared
  backend; separate processes can still race, and load-modify-save sequences
  can still lose updates because the `Persistence` trait remains unchanged.
- Before merge, re-check the final PR checks and retain the explicit
  single-process/non-transactional persistence limits above.
  The replacement CON-1548 boundary is documented in `docs/PERSISTENCE_TOPOLOGY.md`.

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
