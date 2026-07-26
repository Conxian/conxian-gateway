# Session Summary — RGB #228 Process-Lifetime Stash Ownership

**Date:** 2026-07-26

**Issue:** [#228](https://github.com/Conxian/conxian-gateway/issues/228)

## Delivered in this slice

- `StashResolver` now acquires a non-blocking exclusive advisory lock at
  `<RGB_STASH_PATH>/.conxian-rgb-owner.lock` before update-journal recovery,
  metadata/registry loading, `StockpileDir::load`, or stockpile/registry
  mutation.
- The ownership guard is retained as the resolver's final field so every other
  stash handle is dropped before ownership is released.
- Unix lock creation uses `O_NOFOLLOW`, `O_CLOEXEC`, owner-only `0600`
  permissions, regular-file validation, and hard-link rejection. Lock
  contention and unsafe/open errors fail startup closed without unlinking the
  lock file.
- Non-Unix builds reject RGB stash resolver startup before creating the stash
  root. Windows ownership safety is not claimed until a reviewed native
  no-follow/reparse-point-safe implementation exists.
- Ownership is acquired immediately after the root exists. Root and lock-file
  permission hardening occurs only after successful acquisition, so a losing
  startup cannot alter an already-owned root's mode.
- Tests cover true subprocess and deterministic same-process contention,
  release, root-mode preservation on failed acquisition, independent stash
  roots, directory/symlink/hard-link rejection, Unix permissions, and proof
  that a failed acquisition performs no journal recovery, metadata rewrite,
  registry rewrite, or stockpile mutation. The explicit non-Unix rejection test
  is cfg-gated because runtime execution requires a non-Unix target.
- The RFC and environment example now state the exact lock filename and the
  enforced single-process, local-filesystem deployment boundary.

## Verification

- `cargo fmt --all -- --check` — passed.
- `cargo test -p conxian_engine --features rgb-native rgb_stash --lib -- --test-threads=1`
  — passed: 42 tests, 0 failed.
- `cargo clippy -p conxian_engine --features rgb-native --lib --tests -- -D warnings`
  — passed.
- `git diff --check` — passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  passed.
- `cargo test --workspace` — passed.
- `cargo test --workspace --features mock-integrations` — passed.
- `pnpm install --frozen-lockfile && pnpm build && pnpm test` — passed.
- `GET /api/v1/health` — passed with HTTP 200 and exact
  `{"status":"ok"}`.
- `python3 scripts/verify_contamination_guard.py` — passed.
- Final `git diff --check` — passed.
- Final scoped diff reviewed for acquisition ordering, guard drop ordering,
  unsafe lock paths, permissions, and preservation of existing transactional
  semantics.

## Security boundaries preserved

- RGB dependency pins and pinned `rgb-std`/`rgb-persist-fs` behavior are
  unchanged.
- `RejectIssuerSignatures`, issuer-validator semantics, and Active-mode
  fail-closed behavior are unchanged.
- Existing unknown-contract staging and existing-contract copy-on-write,
  journal, backup, promotion, and recovery semantics are unchanged.
- The lock is a Unix advisory local-filesystem primitive; network/shared
  filesystems and non-Unix platforms are not supported for `RGB_STASH_PATH`.

## Still open for issue #228

- A production issuer-signature verification backend.
- A complete independently reproducible signed Bitcoin/RGB regtest fixture and
  end-to-end harness. Existing generated/replayed fixtures continue to prove
  filesystem and pinned API boundaries only; they are not a real
  state-changing signed RGB transition.
