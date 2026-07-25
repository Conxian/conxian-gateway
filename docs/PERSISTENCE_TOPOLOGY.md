# Gateway File Persistence Topology

The production file backend supports **one Gateway process for each configured
state path**. Configure that path with `GATEWAY_STATE_PATH`; the reference
Compose topology uses `/usr/app/gateway_state.json` and one replica per volume.

At startup, the Gateway takes a process-lifetime advisory ownership lock in a
separate sibling file. A second Gateway configured with the same state path
fails closed. Each compare-and-swap transaction also takes a separate
cross-process transaction lock, reloads the current revision, checks the
caller's expected revision, and only then writes the next revision.

State is stored as format-versioned JSON containing a monotonic revision and
the `PersistentState` payload. A legacy plain `PersistentState` document loads
as revision `0` and is migrated to the current envelope on the first successful
mutation. Invalid JSON, malformed envelopes, unsupported format versions,
revision conflicts, and lock failures are errors; they are not converted to
default state.

Writes use a unique temporary file in the state directory, flush and sync that
file, atomically rename it over the state path, clean up failed temporary
files, and sync the parent directory on the supported Unix deployment.

## Filesystem boundary

This topology assumes a local Unix filesystem whose advisory locks, atomic
same-directory rename, file sync, and directory sync have their normal local
semantics. It does **not** claim correctness for NFS, SMB, shared network
volumes, path aliases that bypass the sibling lock files, or distributed
multi-host writers. Operators must give every process referring to the same
state the same configured path and must not scale the file-backed service above
one replica per state volume.

Phase 1 introduces the versioned transaction boundary but leaves existing
listener and orchestrator `load`/`save` compatibility call sites in place.
Because legacy `save` has no caller-supplied revision, that compatibility path
cannot detect a change that happened before `save` began; it only surfaces a
race detected by its internal CAS. Phase 2 must migrate every state mutation to
explicit revision-aware update loops so stale snapshots cannot overwrite newer
fields.
