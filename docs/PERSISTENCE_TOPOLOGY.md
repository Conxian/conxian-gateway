# Gateway File Persistence and Recovery

This is the operator source of truth for the production file backend, its
mempool fee-bump boundary, and Gateway process supervision. The canonical
implementation is `pkg/conxian-core/src/persistence.rs`; Gateway consumes it
through `internal/engine/src/persistence.rs`.

## Supported topology

Set `GATEWAY_STATE_PATH` to a state file on a trusted local filesystem. The
reference Compose path is `/usr/app/gateway_state.json`. The only supported
`GATEWAY_PERSISTENCE_MODE` is `exclusive-local-writer`: one Gateway process,
one canonical state path, one local volume. The following are unsupported:

- active-active or multiple Gateway writers for one state path;
- shared/RWX volumes, NFS, SMB/CIFS, or other distributed/network filesystems;
- copying, restoring, editing, or deleting persistence artifacts while a
  Gateway owns the path.

At startup, `FilePersistence::acquire_ownership` takes a process-lifetime
advisory lock. A separate transaction lock serializes reload/check/write CAS
transactions across cooperating local processes. These locks are not
distributed fencing and cannot protect against another program which ignores
them. Lock files may remain after a clean exit; ownership is the live advisory
lock, not the file's presence.

Linux startup classifies the canonical state-parent filesystem. Known shared
or network classes fail closed. Known local filesystems are allowlisted.
Unknown classes, and all non-Linux platforms where this classifier is not
implemented, require `GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM=true`. That
override means the operator accepts an unclassified filesystem; it does not
turn advisory locks into distributed leases, bypass a known network
classification, or add multi-host fencing.

## Trusted state directory and artifacts

The canonicalized parent directory must be owned and writable only by the
Gateway service account and trusted administrators. Enforce restrictive Unix
ownership and permissions through deployment configuration; the backend
validates object types and descriptors but does not repair directory modes.
For a state file named `gateway_state.json`, expect:

- `gateway_state.json`: the current strict JSON envelope;
- `.gateway_state.json.ownership.lock`: process ownership lock;
- `.gateway_state.json.transaction.lock`: per-transaction lock;
- `.gateway_state.json.tmp-<pid>-<sequence>`: unique same-directory write temp.

Existing state targets must be regular, non-symlink files and, on Unix, have a
single hard link. State, lock, temp, and directory opens use no-follow flags
where supported and validate opened descriptors. The parent is still a trusted
directory boundary, not a descriptor-relative sandbox against a user who can
continuously replace names inside it.

## Envelope, CAS, and writer ownership

The current envelope contains `format_version: 1`, a monotonic `revision`, and
`state`. A legacy plain `PersistentState` loads at revision `0` and migrates on
the first successful mutation. Mixed/incomplete envelopes, reserved envelope
keys in legacy data, unknown fields, invalid JSON, and unsupported versions
fail closed; they are never replaced with default state.

Production mutation uses `Persistence::load_versioned` plus
`compare_and_swap`. The deprecated `save` trait hook remains only for source
compatibility and fails closed. Field ownership is additive:

- `BitcoinListener` mutates only `bitcoin_height`;
- `StacksListener` mutates only `stacks_height`;
- `MempoolOrchestrator` mutates only `mempool_pending_txs`;
- the telemetry API reads persisted mempool records and does not write state.

Listeners retry revision conflicts only, up to their bounded CAS attempt
limit. They update in-memory heights only after a durable replacement succeeds,
and publish Redis state roots only after that commit. Corruption, lock, I/O,
lease, and durability-uncertain outcomes are not conflict retries.

The file backend is synchronous. Gateway construction, ownership acquisition,
initial load, runtime loads, CAS writes, and bounded transactional updates run
through `spawn_blocking` via the shared `AsyncPersistence` adapter. Tokio join
failures become persistence errors. Network RPC and Tokio locks stay outside
the blocking filesystem closure.

## Atomic replacement and crash residue

Each write creates a unique temp in the state directory, writes and flushes the
complete envelope, calls file `fsync`, atomically renames it over the state
path, then calls directory `fsync` on Unix. With the supported local-filesystem
assumptions, a crash exposes a complete old or complete new envelope, never a
partially overwritten state file.

A failure before rename is cleaned up on the ordinary error path. A process
crash can leave a pre-rename temp because no cleanup code runs. Startup does
**not** delete it automatically: a filename alone is insufficient proof that a
file is safe to remove without broadening the trusted-directory or ownership
boundary. Orphan temps do not participate in reads or CAS and cannot replace
the canonical state name. Subprocess tests crash immediately before and after
rename, restart the backend, verify complete old-or-new state, and prove a
pre-rename orphan does not block a later CAS.

To remove residue, stop every Gateway using the directory, verify no process
holds the ownership lock, back up or inspect the canonical envelope, and remove
only regular single-link files matching the exact sibling pattern
`.gateway_state.json.tmp-<pid>-<sequence>`. Never follow links, recursively
delete the directory, or remove the canonical state/lock files as temp cleanup.

Directory-sync failure after rename is reported as
`ConxianError::PersistenceCommitUnknown`: the target revision may already be
visible, but durability was not confirmed. Treat this as a
**persistence-durability-uncertain** stop-and-inspect condition, not as a CAS
conflict. Stop automatic mutation, retain the state directory, load and inspect
the envelope/revision while ownership is controlled, compare it with logs and
external effects, and reconcile before restart. Do not blindly replay the
operation.

## Mempool fee-bump lease and reconciliation

Before RBF or CPFP work, the orchestrator CAS-claims a tracked transaction with
its owner ID, a unique `lease_id` fencing token, a `record_generation`, and an
expiry. Any unexpired lease blocks another claim, including a concurrent tick
from the same owner. Completion succeeds only while owner, lease ID, and record
generation still match; stale completion cannot overwrite reconciliation or a
terminal record.

Bitcoin RPC runs outside file locks and under a deadline shorter than the lease
TTL. The model is **at-least-once**, not exactly-once: a crash after node
submission but before result persistence can expose a repeat opportunity after
lease expiry because the RPC adapters provide no durable idempotency key.
Timeouts and ambiguous RBF/CPFP errors are recorded as
`BUMP_OUTCOME_UNKNOWN`. `BUMP_BROADCASTED`, `BUMP_OUTCOME_UNKNOWN`, and
`CONFIRMED` records are not automatically claimed again.

For an expired lease or unknown outcome:

1. Stop the Gateway or otherwise ensure one controlled owner performs the
   reconciliation.
2. Query the configured Bitcoin node for the original transaction, any known
   replacement/child transaction, mempool conflicts, and confirmations.
3. Compare node evidence with the persisted `txid`, `replacement_txid`, status,
   strategy, attempt count, lease fields, and record generation.
4. Apply one explicit CAS reconciliation: mark confirmed/broadcasted when
   proven, or clear/advance the record for a reviewed retry only when node
   evidence proves no prior submission took effect.
5. Restart normal orchestration and confirm telemetry reflects the reconciled
   state.

Do not clear leases solely because time elapsed, reset unknown outcomes, or
blindly rebroadcast. A `PersistenceLeaseLost` error means another revision owns
or reconciled the record; reload rather than overwriting it.

## Supervision and shutdown

`cmd/gateway/src/supervisor.rs` retains the HTTP server, Bitcoin and Stacks
listeners, mempool orchestrator, treasury monitor, and NTT relayer as one
critical failure domain. An unexpected normal return, error, or panic from any
critical task is process-fatal and requests the same coordinated shutdown used
for SIGINT/SIGTERM. Therefore the HTTP server does not remain healthy after a
durable worker exits.

Shutdown asks every task to stop, lets Axum drain through graceful shutdown,
and waits up to 30 seconds. A task which returns normally after cancellation is
not treated as a new failure. Errors and panics remain fatal and visible. At
the deadline, remaining tasks are aborted and the process returns failure.
Persistence filesystem work remains on the blocking executor, so cancellation
does not hold an async lock while waiting on a blocking transaction.

## Backup and restore

Back up or restore only while the Gateway is stopped and the ownership lease is
free. Preserve the whole trusted directory (state plus lock artifacts and any
temp evidence) for incident analysis, but restore only a reviewed complete
canonical envelope into a correctly owned local directory. Validate JSON,
format version, revision, and expected checkpoint/mempool fields before
starting. Never restore onto a live owner or use backup synchronization as an
active-active replication mechanism.

## Operator telemetry

`GET /api/v1/bitcoin/mempool/telemetry` is a private route: it requires the
configured bearer token and currently passes through the private x402
middleware as well. It reports only Gateway-persisted tracked transactions, not
the node/network mempool, and omits transaction IDs and error strings.

When persistence is absent it returns stable HTTP `503` with
`tracked_mempool_state_not_configured`; when loading fails it returns stable
HTTP `503` with `tracked_mempool_state_unavailable`. The endpoint and response
types are currently server-side only; `packages/client-sdk` and
`packages/schemas` do not expose a generated client method or schema for it.

## Verification boundary

Tests retain true subprocess ownership exclusion/release and one-winner
same-revision CAS contention. Additional subprocess crash tests exercise both
sides of rename. These tests do not establish distributed-filesystem safety,
hostile-directory safety, exactly-once Bitcoin broadcast, or automatic
reconciliation.
