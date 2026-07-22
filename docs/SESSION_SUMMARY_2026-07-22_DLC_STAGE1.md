# Session Summary — 2026-07-22 (DLC Stage 1 Isolated Conformance)

## Continuity and branch

- Ran `git pull origin main`; the verified base was clean at
  `8dc80fa411eda04bb9d0ea5e55fa2d10098a5df6`.
- Reused devbox `dbx_33tphbBvNNGg13zSHuL4t` in
  `Conxian/conxian-gateway`.
- Created the focused branch
  `charlie/issue-220-dlc-stage1-conformance` from that base.
- Reviewed the existing Stage 0 experiment, canonical evidence, Stage 0
  comparison, gap analysis, cross-repo dashboard, sprint review, prior DLC
  session summaries, and issue #220 context before editing.

## Stage 1 outcome

- Added an explicit `--compatibility` mode to
  `experiments/dlc-stage0/rust-dlc-probe/src/bin/rust-dlc-stage0-vector-probe.rs`.
  It normalizes only evidenced `localPayout` keys in `offer_message.message`
  in memory; it does not rewrite canonical fixtures.
- Added normalization tests for the scoped mapping and ambiguous-key rejection.
- Added `rust-dlc-stage1-conformance`, a deterministic isolated binary/test
  covering valid oracle validation, wrong event/key/outcome rejection, invalid
  announcement/attestation signatures, and mutated-CET transaction binding.
- Captured the hyperbola mismatch at byte offset `104` (`0x01` expected,
  `0x40` actual), with the spec fixed-point versus upstream `f64` encoding
  classification.
- Added the detailed research record
  `docs/research/DLC_STAGE1_CONFORMANCE_2026-07-22.md` and updated only the
  minimal Stage 0, canonical evidence, gap-analysis, cross-repo, README, and
  session-continuity references needed to preserve the evidence chain.
- No Gateway workspace manifest, production endpoint, runtime path, key-custody
  path, or production dependency was changed.

## Reproducible results

- Direct pinned vectors: `14` total, `7` parsed, `7` blocked by
  `missing field \`offerPayout\``, `6/7` parsed numerical vectors matching all
  offer/accept/sign bytes.
- Compatibility mode: `14` parsed, `0` blocked, `13` complete
  offer/accept/sign byte sets matching, `28` normalized fields.
- Vector probe unit tests: `2 passed, 0 failed` on Rust `1.85.1` and `1.96.0`.
- Stage 1 conformance tests: `7 passed, 0 failed` on Rust `1.85.1` and
  `1.96.0`.
- Stage 1 report: `valid_oracle_boundary=1`, `oracle_rejection_cases=5`,
  `transaction_binding_rejection_cases=1`, `total passed=7 failed=0`.
- The pinned upstream event-ID comparison is not implemented in
  `OracleAttestation::validate`; the experiment wrapper enforces it and records
  that boundary explicitly.

## Verification scope

Passed targeted checks:

```text
cargo +1.96.0 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check
cargo +1.85.1 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check
cargo +1.96.0 test --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe
cargo +1.85.1 test --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe
cargo +1.96.0 test --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe --bin rust-dlc-stage1-conformance
cargo +1.85.1 test --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe --bin rust-dlc-stage1-conformance
cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe -- --vectors /tmp/dlcspecs-stage1/test/test_vectors
cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe -- --compatibility --vectors /tmp/dlcspecs-stage1/test/test_vectors
cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe --bin rust-dlc-stage1-conformance
```

The full workspace, Node, health-check, and production verification suites
were intentionally deferred to the separate verification phase. The
contamination guard passed before commit/push: `Production paths are clean.
(60 files scanned)`.

## Unresolved gates

- The hyperbola wire-format mismatch remains open and is not translated.
- Full authoritative offer/accept/sign/funding/CET/refund fixtures and expected
  bytes are not yet complete.
- Wrong nonce, malformed message, complete funding-output reconciliation,
  manager/provider, persistence, restart/recovery, transport, testnet, and
  production operations remain out of scope.
- The Gateway remains an HTTP oracle/event/key/outcome and UUID/mock-bond
  scaffold; no CET readiness or cryptographic production verification is
  claimed.

No PR or GitHub comment was opened or posted during this session.
