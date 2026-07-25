# Gateway File Persistence Topology

The production file backend supports **one Gateway process for each configured
state path**. Configure that path with `GATEWAY_STATE_PATH`; the reference
Compose topology uses `/usr/app/gateway_state.json` and one replica per volume.

At startup, the Gateway takes a process-lifetime advisory ownership lock in a
separate sibling file. A second Gateway configured with the same state path
fails closed. Each compare-and-swap transaction also takes a separate
cross-process transaction lock, reloads the current revision, checks the
caller's expected revision, and only then writes the next revision.

State is stored as strict format-versioned JSON containing a monotonic revision
and the `PersistentState` payload. A legacy plain `PersistentState` document
loads as revision `0` and is migrated on the first successful mutation.
Incomplete or mixed envelope shapes, reserved envelope keys in legacy data,
unknown fields, invalid JSON, unsupported versions, lock failures, and unsafe
state paths fail closed; they are never converted to default state.

All production writers use explicit compare-and-swap revisions. Bitcoin and
Stacks checkpoint updates reload and reapply only their owned height field for
at most four attempts, retrying revision conflicts only. Corruption, lock,
filesystem, and durability errors stop the update. Listener in-memory height
and `last_height` advance only after the durable state replacement succeeds.
The former non-transactional `save` mutation API is not part of the persistence
trait, so a legacy snapshot writer cannot overwrite a concurrent CAS update.

Writes use a unique temporary file in the normalized state directory, flush
and sync that file, atomically rename it over the state path, clean up failed
temporary files, and sync the parent directory on Unix. A parent-directory sync
failure after rename is reported as an **unknown commit outcome**, not a CAS
conflict. Callers must reload/reconcile it and must not enter the normal
conflict retry loop.

## Mempool fee-bump leases

Each orchestrator has a unique owner ID. Before any RBF or CPFP network call it
atomically claims the tracked transaction with an owner and expiry timestamp.
Network work happens outside the file lock. The result is committed only while
the same owner still holds the lease, and the commit mutates only that tracked
transaction. Active leases exclude other orchestrators; expired leases can be
reclaimed after a crashed process.

This is an **at-least-once**, not exactly-once, external side-effect boundary.
A crash after node submission but before the result commit can lead to one
repeat after lease expiry because the Bitcoin RPC adapters do not expose a
durable idempotency key. RPC errors are recorded as `BUMP_OUTCOME_UNKNOWN` and
are not retried automatically. A post-rename durability-confirmation error is
also not retried; the next tick reloads the committed record. Successfully
recorded `BUMP_BROADCASTED` and `BUMP_OUTCOME_UNKNOWN` records are skipped until
an explicit reconciliation path changes their status.

## Filesystem boundary

The configured parent directory is canonicalized once; relative and absolute
aliases therefore derive the same state, ownership-lock, transaction-lock, and
temporary-file identity. Existing state targets must be regular files. Symlink,
directory/device, and (on Unix) hard-linked targets are rejected.

This topology assumes a local Unix filesystem whose advisory locks, atomic
same-directory rename, file sync, and directory sync have normal local
semantics. It does **not** support NFS, SMB, distributed/shared network volumes,
or multi-host writers. Advisory locks protect cooperating Gateway processes,
not arbitrary programs that ignore the lock files. Operators must not scale a
file-backed state volume above one Gateway owner.

Tests exercise separately constructed backends and true subprocesses for
ownership exclusion/release and same-revision CAS contention. They do not prove
behavior on unsupported distributed filesystems or provide exactly-once Bitcoin
transaction broadcast semantics.
