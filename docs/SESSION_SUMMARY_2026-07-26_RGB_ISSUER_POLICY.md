# Session Summary — RGB #228 BIP340 Issuer Policy

**Date:** 2026-07-26
**Issue:** [#228](https://github.com/Conxian/conxian-gateway/issues/228)
**Branch:** `charlie/issue-228-bip340-issuer-policy`

## Implemented slice

- Added `Bip340IssuerPolicy`, an opt-in `IssuerSignatureValidator` backed by a
  strict version-1 JSON public-key allowlist.
- Bound each exact, case-sensitive printable-ASCII RGB identity of `1..=4096`
  bytes to one pinned BIP340 secp256k1 x-only public key.
- Required raw 64-byte signatures over the exact 32 callback bytes, with no
  rehashing, text encoding, algorithm inference, or accept-all fallback.
- Added bounded Unix file loading: 64 KiB maximum, regular files only, symlinks
  rejected, and opens use `O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC` before
  validating the opened descriptor is regular. Non-Unix file loading fails
  closed as unsupported until an equivalent handle-level no-follow
  implementation exists.
- Kept `RejectIssuerSignatures` as the runtime/default behavior. No HTTP import
  endpoint, environment setting, or production call site was invented.

## Coverage

Deterministic tests cover valid signatures; wrong identity, case, key, and
commitment; malformed signature/message lengths and bytes; direct callback
message use versus a second SHA-256; duplicate identities; unsupported schema
and algorithm; unknown fields; empty, 4096-byte, and oversized identities;
malformed keys; oversized files; and Unix symlink/non-regular-file rejection.
Existing stash tests remain the coverage for consignment preflight, fail-closed
default policy, transactional updates, and process ownership.

## Documentation

- Updated `docs/RFC_RGB_ADAPTER.md` with the exact Conxian BIP340 profile,
  library-only usage, file boundary, and remaining runtime/regtest limitations.
- Added `docs/research/RGB_BIP340_ISSUER_POLICY_2026-07-26.md` with verified
  upstream callback/type evidence, rejected auto-detection designs, and
  rotation/revocation follow-up.
- Corrected current status notes that still listed transactional existing-
  contract updates as open while preserving their dated historical context.

## Verification

- `cargo fmt --all -- --check` — passed.
- `cargo test -p conxian_engine --features rgb-native bitcoin::rgb_issuer_policy::tests -- --nocapture`
  — passed, 10 tests.
- `cargo test -p conxian_engine --features rgb-native bitcoin::rgb_stash::tests -- --test-threads=1`
  — passed, 42 tests; subprocess ownership checks also passed.
- `cargo check -p conxian_engine --all-targets` — passed without `rgb-native`.
- `cargo clippy -p conxian_engine --all-targets --features rgb-native -- -D warnings`
  — passed.
- `python3 scripts/verify_contamination_guard.py` — passed, 67 production
  files scanned.
- `git diff --check origin/main...HEAD` — passed.

## Remaining gaps

- Wire the policy only through an approved controlled import surface; default
  runtime behavior must remain `RejectIssuerSignatures`.
- Produce a complete independently reproducible, state-changing signed
  Bitcoin/RGB regtest fixture and end-to-end harness.
- Define operational policy distribution, atomic rotation/revocation, audit
  provenance, and restart behavior before enabling production imports.
