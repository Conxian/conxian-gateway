# Session Summary — 2026-07-20 (Liquid #218 / #193 follow-up)

## Scope and guard conditions

This session aligned open PR #258 with current `main` as the narrow follow-up
to merged PR #257. No GitHub comments, reviews, issue edits, labels, or PR
metadata changes were made.

- Repository: `Conxian/conxian-gateway`
- Worktree branch: `charlie/issue-218-liquid-e2e`
- Base `origin/main` at session start: `4ead49f284197672f9b7e43e32359848e03708de`
- PR #258 head at session start: `809b86a31af44852c0dd0abc8060fb536230a2d7`
- PR #257 merged harness: `a0c3396aef00c62c6aaa1ae56da5b62fa92b2783`
- PR #260 canonical Dependency Review ref: `e5c58c9b25da1d2acc21499d5ea5d35564f0e07c`
- Normal merge commit created on this branch: `a394ec3ac974aa8d0e15cdab52eefca0157b739c`
- Workflow conflict resolution and host-daemon workflow hardening: included in merge commit `a394ec3ac974aa8d0e15cdab52eefca0157b739c`
- Harness hardening/removal commit: `b09a6e31a700ba3a3cdf0922b92358bd79700386`
- Final harness path/depth refinement commit: `eec6ad252c13dc3c853a30e0b03a54ffb839b890`
- Verifier/API coverage commit: `89a45d359c8db53d0256aa5e8e95d90e4faca07c`

The merge was a normal `--no-ff` merge; no rebase or force-push was used.
The workflow conflict was resolved in favor of the merged host-daemon
workflow from `main`, then retained the required follow-up hardening.

## Acceptance mapping

### Workflow and harness

- `.github/workflows/liquid-e2e.yml` keeps the host-daemon job, SHA-pinned
  actions, concurrency, Rust adapter test, daemon harness command, and
  always-uploaded artifact path.
- Both `pull_request` and `push` triggers have focused paths for the workflow,
  `tests/liquid/**`, Liquid adapter, compliance verifier, gateway API tests,
  and relevant Cargo manifests/lockfile.
- `workflow_dispatch.pegin_confirmation_depth` is documented and validated by
  the harness. Non-dispatch events use the explicit `100` fallback, preserving
  the default effective target of `max(102, 100 + 2) = 102` confirmations.
- `tests/liquid/liquid_peg_e2e.sh` validates strict decimal depth `2..1000`,
  configures Elements with it, verifies `getsidechaininfo`, keeps the 102
  compatibility floor, derives `max(102, configured_depth + 2)`, and rejects
  invalid or duplicate RPC/P2P ports.
- Artifact parents must resolve to an owned subdirectory inside repository
  `target/`; root, home, repository root, `target/`, symlinked, and non-owned
  paths are rejected. Each run creates a unique owned subdirectory and marker;
  arbitrary parents are never cleared and failure artifacts remain available
  to the workflow upload.
- `tests/liquid/install_daemons.sh` keeps checksum, cache, architecture, and
  version behavior while restricting cache/install paths to owned subdirectories
  under `target/`. Recursive deletion of an override requires the exact
  harness ownership marker; the canonical default install path is allowed by
  location and receives the marker after preparation.
- Duplicate PR #258 Compose harness files were removed:
  `scripts/liquid-e2e.sh`, `tests/liquid-e2e/README.md`, and
  `tests/liquid-e2e/docker-compose.yml`. The merged `tests/liquid/**` harness
  remains.

### Fail-closed production boundary and coverage

- `internal/engine/src/bitcoin/liquid_adapter.rs` remains fail-closed and its
  existing arbitrary-metadata rejection test is unchanged in spirit.
- `internal/compliance/src/verifier.rs` now tests exact payload delegation,
  true and false result propagation, adapter error propagation, and unknown
  chain errors with a recording/failing adapter.
- `cmd/gateway/tests/api_tests.rs` now verifies authenticated/payment-authorized
  HTTP `POST /api/v1/chains/liquid/verify` requests return HTTP 200 with
  `chain: liquid` and `verified: false` for both arbitrary `verified: true`
  metadata and empty metadata.

### Current status documentation

- `docs/CROSS_REPO_STATUS.md` separates the merged host-daemon harness from
  the still-unwired production Liquid proof backend.
- `docs/research/KNOWLEDGE_MAP.md` now labels Liquid
  `Harnessed / fail-closed proof boundary` rather than live proof verification.

## Targeted verification

Passed:

- `git pull --ff-only origin main` from the clean main checkout; main remained
  at `4ead49f284197672f9b7e43e32359848e03708de`.
- `bash -n tests/liquid/install_daemons.sh tests/liquid/liquid_peg_e2e.sh`
- Shell rejection assertions for invalid depth, invalid/duplicate ports,
  artifact paths outside `target/`, cache/install paths outside `target/`, and
  unmarked install overrides.
- Ruby structural YAML parsing and workflow checks, including matching trigger
  path filters and dispatch input.
- `cargo fmt --all -- --check`
- `cargo test -p conxian_engine liquid_adapter --lib` — 1 passed.
- `cargo test -p conxian_compliance verifier::tests --lib` — 4 passed.
- `cargo test -p gateway --test api_tests liquid` — 3 passed.
- `python3 scripts/verify_contamination_guard.py` — production paths clean,
  60 files scanned.
- `git diff --check` and duplicate-harness removal checks.

Intentionally not run in this targeted phase:

- Full workspace tests/clippy, `mock-integrations` tests, pnpm install/build/test,
  health-check process validation, and the real-daemon Liquid suite. These are
  reserved for the later full-verification phase.

## Non-claims and remaining state

- This work does not wire a production Liquid/Elements state-proof backend.
  `LiquidAdapter::verify_state_proof` remains fail-closed and does not trust
  caller-supplied metadata.
- The local harness does not prove Watchmen release, federation quorum, PAK
  policy, production timing, production federation coverage, or automatic
  Bitcoin release.
- PR #258 remains open for review; issue #218/#193 remains the tracking context
  for the local harness. The follow-up branch is ready for the later full
  verification phase after the normal push and CI run.
