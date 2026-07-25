# Session Summary — CON-1548 Phase 2

Date: 2026-07-25

Branch: `charlie/con-1548-transactional-persistence`

## Scope

Phase 2 hardens the file persistence transaction boundary and migrates the
Bitcoin listener, Stacks listener, and mempool orchestrator away from whole
state `load -> modify -> save` writes.

## Implemented on the branch

- `Persistence` now requires explicit `load_versioned` and
  `compare_and_swap` implementations; the non-transactional mutation fallback
  and `save` trait method are removed.
- Production checkpoint writers use bounded conflict-only CAS retries and
  mutate only their owned height field. Persistence failures do not advance
  listener in-memory checkpoints.
- Mempool RBF/CPFP work uses durable owner/expiry claims. External RPC calls run
  outside file locks and results commit only while the same owner retains the
  lease.
- Strict envelope parsing rejects incomplete/mixed envelope shapes, reserved
  keys in legacy state, unknown fields, and unsupported versions.
- File paths use a canonical parent identity and reject symlink, non-regular,
  and Unix hard-linked state targets. Unix opens use no-follow flags and verify
  opened descriptor metadata; the parent remains a trusted local directory.
- Post-rename parent-sync failure is a distinct unknown commit outcome and is
  never treated as a retryable revision conflict.
- Tests include true subprocess ownership exclusion/release and same-revision
  CAS contention, plus listener conflict/failure and mempool lease recovery.
- The canonical backend and tests live in `conxian-core`; Gateway consumes that
  implementation directly.
- Bitcoin and Stacks persist before updating in-memory state and publish Redis
  state roots only after the durable commit.
- Mempool claims use unique lease fencing tokens and record generations. RPC
  work has a deadline below the TTL; timeout requires reconciliation.

## Guarantee boundary

The file backend supports one Gateway owner per state path on a local Unix
filesystem. It does not support distributed filesystems or multi-host writers.
Mempool external side effects are at-least-once: a process crash after broadcast
but before result persistence can cause one repeat after lease expiry. Ambiguous
RPC outcomes and confirmed unknown persistence outcomes are not blindly retried.

## Status

Changes are local to the branch. No remote push or pull request was created in
this session.
