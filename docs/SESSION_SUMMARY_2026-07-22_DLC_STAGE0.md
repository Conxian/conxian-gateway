# Session Summary — 2026-07-22 (DLC Stage 0 SDK Comparison)

## Continuity and branch

- Verified clean `main` after `git pull --ff-only origin main`.
- Verified base `origin/main` at `175ac209f24099c3ff0c4dcd5143ea955007c0d8`, the merge of PR #269.
- Reviewed the existing DLC evidence ledger, prior DLC session summary, gap analysis references, sprint status, and open issue #220.
- Created `charlie/issue-220-dlc-stage0-spike` from the verified `origin/main` without rewriting or force-pushing.

## Stage 0 outcome

- Added the canonical comparison at [`docs/research/DLC_STAGE0_SDK_COMPARISON_2026-07-22.md`](research/DLC_STAGE0_SDK_COMPARISON_2026-07-22.md).
- Added bounded standalone probes under `experiments/dlc-stage0/` for pinned `rust-dlc v0.8.0` and DDK `v1.1.2`, plus the caller-supplied `dlcspecs` vector probe.
- Decision remains gated: keep low-level `dlc` + `dlc-messages` as the preferred candidate, keep DDK as fallback, and make no Gateway dependency or runtime change.
- The official vector gate is not yet clear: seven enum/mixed vectors stop on `localPayout` versus `offerPayout`; six of seven directly parsed numerical vectors round-trip offer/accept/sign bytes, while the hyperbola offer differs.
- Issue #220 remains open; this record does not claim CET construction or production readiness.

## Verification status

- Standalone Rust 1.96.0 format, locked workspace check, clippy, and both probe runs passed.
- The fresh exact `dlcspecs` checkout at commit `9cd9148938c616690c79d99ec6f330e213c246c5` reproduced the documented result: 7 numerical vectors parsed, 7 enum/mixed vectors blocked by the `localPayout`/`offerPayout` schema mismatch, and 6 of 7 parsed vectors matched all offer/accept/sign bytes; the hyperbola offer differed.
- Rust 1.85.1 low-level probe check passed.
- DDK Rust 1.85.1 check failed as expected on dependency use of unstable `unsigned_is_multiple_of`; the source and dependency constraints were not weakened.
- Gateway format, clippy, workspace tests, mock-integration tests, Node build/tests, health check, contamination guard, diff check, and unchanged-root-manifest verification passed.
- This continuity artifact records the Stage 0 branch state only; issue #220 remains open and no Gateway dependency or runtime behavior changed.
