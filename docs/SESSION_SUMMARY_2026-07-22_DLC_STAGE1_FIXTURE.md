# Session Summary — 2026-07-22 (DLC Stage 1 Deterministic Fixture)

## Continuity and scope

- Verified the required branch was clean at
  `f64bec4a14f73eb05d1016e21bd638877022d2dd` before editing; `origin/main`
  remained at `28dad48fe3df4f2735be199d5354ef02d4a320dc`.
- Reused devbox `dbx_33trP9dbfueIwwprfx2Yf` in
  `Conxian/conxian-gateway`.
- Used branch `charlie/issue-220-stage1-fixture`.
- Kept all implementation and dependencies under `experiments/dlc-stage0/`;
  no root Gateway manifest, `internal/**`, `pkg/**`, or `cmd/**` path changed.

## Delivered milestone

- Corrected the isolated fixture's blocking funding-input validity flaw. Each
  synthetic previous output is now a native P2WPKH output derived from its
  funding key; the empty redeem script, BIP143 script code, value, witness,
  address semantics, and `FundingInput` metadata agree.
- Added independent cryptographic verification for both assembled funding
  inputs, every signed CET, and the signed refund, including exact witness
  stack shape, script, value, and public-key checks.
- Serialized concrete `OfferDlc`, `AcceptDlc`, and `SignDlc` messages and
  constructed funding, fully assembled funding witnesses, both CETs, refund,
  adaptor signatures, signed CETs, and signed refund artifacts with pinned
  `rust-dlc v0.8.0` APIs.
- Added type-ID-aware full serialization round trips with equality assertions,
  semantic message-binding checks, parsed prev-tx/outpoint/vout reconciliation,
  and named/typed rejection assertions.
- Recorded stable message hashes, transaction IDs, final contract ID, CET count,
  and canonical digest (including the signed funding artifact). Added 13 local
  fail-closed rejection cases.
- Added the exact artifact and boundary record at
  `docs/research/DLC_STAGE1_FIXTURE_2026-07-22.md` and dated corrections to the
  earlier Stage 1 research records.

## Verification

- Rust `1.96.0`: isolated fixture format check, tests, binary, affected vector
  probe/conformance tests, and `clippy --bins -- -D warnings` passed.
- Rust `1.85.1`: isolated fixture format check, tests, binary, affected vector
  probe/conformance tests, and `clippy --bins -- -D warnings` passed.
- Rust 1.96/1.85 normal binary output was byte-for-byte identical:
  `stage1_fixture=passed`, `positive_artifacts=...signed_funding...`,
  `rejection_cases=13`.
- `python3 scripts/verify_contamination_guard.py` passed:
  `Production paths are clean. (60 files scanned)`.
- `git diff --check` passed.

## Corrected golden artifacts

- Offer message: `480` bytes,
  `02429a798cf33c6a15bb8cd738c55ad6d581303fbd2370a2d93c79d1c36b1e4c`.
- Accept message: `597` bytes,
  `2ecb9087438980c0dc8749bce7da9b34be644038fbce20af667da92c2d00984a`.
- Sign message: `535` bytes,
  `2d4bfb0d31aafc3aa57693f58830dd008f8637891dbd3ba625067dfbc72d6c91`.
- Funding txid: `f4f0d66c02a0491307f545692d7cbeef9aca095b43a63191bddcaebca08a3334`.
- CET txids: `7f724daedb20461ac379dd0784eaad7acbc11099818b2162a94cbb3b756e2a97`,
  `6d3042cc2050c7fc91889c1e34efd940c15f04875e56323296242178ef499ff1`.
- Refund txid: `43d205a919923fb600c96e777f65e234cfaee8a84b1d48b1d1dc695b82762199`.
- Final contract ID: `e5e1c77d13b1580216e454783c6daffe8bdb184a52b72080accdbfadb19b2225`.
- Canonical digest: `bf13afe7352577f1cd3e28ca92098cd247e5e607948ed1283b1fd9e66ead1f40`.

The corrected output is a self-contained deterministic regression vector, not
independent interoperability evidence.

## Boundary and follow-up

This is an isolated deterministic fixture milestone only. It is not Gateway
runtime/API integration, wallet/transport/persistence/node I/O, custody,
numeric or hyperbola support, public testnet evidence, or production readiness.
The `localPayout` → `offerPayout` normalization remains fixture-scoped to the
existing vector compatibility probe. Issue #220 remains open for authoritative
vector compatibility, manager/provider/state integration, and later security
and operations gates.

No push, pull request, GitHub comment, or GitHub reaction was made.
