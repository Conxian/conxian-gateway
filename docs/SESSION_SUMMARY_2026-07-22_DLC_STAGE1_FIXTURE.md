# Session Summary — 2026-07-22 (DLC Stage 1 Deterministic Fixture)

## Continuity and scope

- Verified `origin/main` and `HEAD` were clean at
  `28dad48fe3df4f2735be199d5354ef02d4a320dc` before editing.
- Reused devbox `dbx_33trP9dbfueIwwprfx2Yf` in
  `Conxian/conxian-gateway`.
- Created branch `charlie/issue-220-stage1-fixture`.
- Kept all implementation and dependencies under `experiments/dlc-stage0/`;
  no root Gateway manifest, `internal/**`, `pkg/**`, or `cmd/**` path changed.

## Delivered milestone

- Added `rust-dlc-stage1-fixture`, a deterministic single-oracle,
  two-outcome enumerated fixture using fixed test-only keys, inputs, scripts,
  serial IDs, payouts, locktimes, and fee rate.
- Serialized concrete `OfferDlc`, `AcceptDlc`, and `SignDlc` messages and
  constructed funding, both CETs, refund, adaptor signatures, signed CETs, and
  signed refund artifacts with pinned `rust-dlc v0.8.0` APIs.
- Recorded stable message hashes, transaction IDs, final contract ID, CET count,
  and canonical digest. Added 13 local fail-closed rejection cases.
- Added the exact artifact and boundary record at
  `docs/research/DLC_STAGE1_FIXTURE_2026-07-22.md` and dated corrections to the
  earlier Stage 1 research records.

## Verification

- Rust `1.96.0`: isolated fixture format check, tests, binary, affected vector
  probe/conformance tests, and `clippy --bins -- -D warnings` passed.
- Rust `1.85.1`: isolated fixture format check, tests, binary, and affected
  vector probe/conformance tests passed.
- Rust 1.96/1.85 normal binary output was byte-for-byte identical:
  `stage1_fixture=passed`, `rejection_cases=13`.
- `python3 scripts/verify_contamination_guard.py` passed:
  `Production paths are clean. (60 files scanned)`.
- `git diff --check` passed.

## Boundary and follow-up

This is an isolated deterministic fixture milestone only. It is not Gateway
runtime/API integration, wallet/transport/persistence/node I/O, custody,
numeric or hyperbola support, public testnet evidence, or production readiness.
The `localPayout` → `offerPayout` normalization remains fixture-scoped to the
existing vector compatibility probe. Issue #220 remains open for authoritative
vector compatibility, manager/provider/state integration, and later security
and operations gates.

No push, pull request, GitHub comment, or GitHub reaction was made.
