# Native persistent storage and decentralised recovery

Observed 25 October 2026. Proposed placement, not an accepted production topology.
Owners: Gateway #466/#547; Business #1390/#1423; encrypted recovery Business #1507.
Retain the 23 October cloud-decoupling KB and 24 October recovery/workspace research.
This document narrows those alternatives against the actual Gateway file contract.

## Preferred direction

Use one fenced Gateway writer on a trusted ext4/XFS volume. Evaluate replication
below the filesystem, with a stopped-writer encrypted recovery copy operated
independently of the primary host. DRBD/LINSTOR is the first native replicated-block
candidate; an already operated and qualified Ceph RBD cluster is a comparison.
These are self-operated distributed systems, not permissionless storage networks.
Separately evaluate Sia for network-distributed encrypted objects and Garage or
Tahoe-LAFS for independently operated recovery. No cluster or vendor has been
selected for production, and no new paid resources or token funding occurred.

The immediate supported baseline remains `exclusive-local-writer` on a qualified
local persistent volume. Managed storage may remain an optional lane while an
operator-controlled replacement proves the same contract. A missing Render API
operation is a management-plane limitation, not evidence that state must stay on
Render or that any remote mount is safe.

## What the code actually requires

`pkg/conxian-core/src/persistence.rs` uses process-lifetime advisory ownership,
revision CAS, same-directory temporary writes, file fsync, atomic rename and parent
fsync. A post-rename sync error is an unknown commit, not a retryable conflict.
`docs/PERSISTENCE_TOPOLOGY.md` remains the operational authority. Do not replace
these properties with eventual-consistency synchronization or a second writer.

The state file is not the complete durable footprint: the offline/replay queue
uses SQLite `offline_queue.db` relative to the process working directory. The
current container works in `/usr/app`, covered by its reference volume. Enabled
RGB/index and other stateful adapters need their own explicit path inventory.
Preserve SQLite WAL/SHM consistency, protected queue-key recovery and the mempool
unknown-outcome reconciliation boundary. Copying only one JSON file is insufficient.
Named SDK-pending database integrations and historical architecture diagrams do not
establish deployed persistence or remove the volume requirement.

## Candidate comparison

| Candidate | Natural role | Required proof / limitation |
|---|---|---|
| DRBD/LINSTOR + ext4/XFS | Replicated block device under the existing file backend | Single-primary ownership, synchronous protocol, quorum/fencing, stale-primary denial, private replication network, safe promotion/demotion, filesystem recovery and measured latency. Requires operator-controlled Linux hosts and kernel/device access; cannot be installed as an unprivileged Render sidecar. |
| Ceph RBD + ext4/XFS | Block volume from an existing qualified storage cluster | Default `exclusive-lock` transitions cooperatively between clients; it does not prevent two filesystems mounting the image. Explicit exclusive mapping and storage fencing/blocklisting plus host orchestration must be qualified. CephFS/shared RWX remains unsupported by the current Gateway file topology. |
| Akash persistent volume | Decentralised-marketplace compute placement | The volume belongs to the chosen provider/lease. Provider replacement requires external recovery; a marketplace label is not cross-provider replication or backup. Qualify the actual provider storage class and single attachment. |
| Sia s3d | Encrypted object recovery candidate | Current project material exposes an S3 interface and client-side encryption. The daemon still has local SQLite metadata and disk buffering; remote acknowledgement/upload completion and metadata/key recovery must be tested. It is not POSIX rename/fsync/flock storage. |
| Garage | Self-operated multi-site object recovery | Pin replication and `consistent` mode; degraded/dangerous modes alter guarantees. Check the exact client-required S3 operations and independent operator/access policy. Object replication is not live state-file ownership. |
| Tahoe-LAFS | Capability-encrypted, erasure-shared recovery | Protect read/write capabilities; test independent servers, introducer/bootstrap loss, repair, metadata and key recovery, deletion/retention and the selected client. Not a drop-in local filesystem. |
| IPFS/Filecoin public archives | Public artifacts and independently retained distribution | Keep the existing public-only allowlist and pinned-CID restore work. No private KB, secrets, operator records or deal files on public storage. Pinning, retrieval and confidentiality are separate. |
| PostgreSQL / decentralised SQL | A separately designed transactional backend | Reuse the Persistence contract only after actual CAS, single-owner/fencing, transaction failure, unknown commit and recovery conformance. Existing self-hostable Postgres research is retained; do not dual-write authoritative settlement state. |

## Native volume conformance, now executable

On the candidate host, use an existing scratch directory on the exact mounted
volume and the repository-pinned Rust toolchain:

```sh
python3 scripts/qualify_persistence_volume.py --directory /approved/mount/probe-parent
```

The command creates a fresh private child and makes the existing native Rust
persistence tests use it through TMPDIR. It never opens the live Gateway state,
changes storage policy, runs chain RPCs or provisions resources. It requires the
named ownership/CAS/crash/corruption/unknown-commit/atomic-replacement tests,
Cargo success and the final parent harness result. Child-harness summaries cannot
stand in for the parent result. Empty/skipped/missing cases fail. A timeout kills
the isolated process group. Logs remain mode0600 inside the private child; retain
failed evidence for inspection and remove only the known probe child after review.

Local result on 25 October: all 25 native persistence tests passed on this task's
filesystem; six driver failure/isolation tests passed. Source and Cargo.lock hashes
are recorded by the command. Initial driver parsing rejected nested child test
output; its parser and regression cases were corrected before this result. This
was not a DRBD/Ceph/Sia test or a persistent-host survival measurement. The output
always leaves production qualification, physical power-loss survival, distributed
fencing and independent restore false; those require real operational evidence.

## Phased acceptance and exit criteria

1. Inventory every durable path, owner, current data classification, format,
   revision, key, dependency and approved RPO/RTO. Pin source/image/dependencies,
   releases/licenses, actual operators and cost inputs; do not invent savings.
2. Run native conformance on the selected mounted filesystem with disposable
   state, then actual node/process restart and forced stop under approved scope.
3. For replicated block pilots, test replication interruption, quorum loss,
   blocked I/O, stale-primary reappearance, denied second mounts, safe promotion
   and restart. Test physical power interruption separately. A storage-class name
   or an ext4 filesystem magic does not prove fencing or stable media.
4. Stop/lock the application for coherent recovery snapshots. Test the complete
   state/queue footprint with ciphertext integrity, SQLite recovery, keys and
   explicit retention/deletion. Restore through an independent operator after the
   original account/host/gateway is unavailable. Record actual RPO/RTO and costs.
5. Independently review the exact capability and normal source/release route,
   verify deployed image/config/source, authorized/denied journeys and rollback.
   Preserve the old supported volume until the replacement is accepted. No live
   settlement/signing or automatic failover is authorized by a component pass.

Reopen a deferred option when new supported APIs, independent recovery evidence,
licence/support changes, measured cost or failure results alter its fit. Preserve
prior research and decision lineage rather than deleting alternatives.

## Current primary sources

Read 25 October 2026. Project documentation establishes its published contract,
not measured Conxian production results. Pin deployed releases before adopting.

- [DRBD 9 guide: replication, quorum and application I/O handling](https://linbit.com/drbd-user-guide/drbd-guide-9_0-en/).
- [Ceph exclusive lock transitions, exclusive mapping and blocklisting](https://docs.ceph.com/en/latest/rbd/rbd-exclusive-locks/) (development documentation; use the chosen stable release for implementation).
- [Akash provider-bound volume/recovery limitations](https://akash.network/explore/compare/akash-vs-aws/) and [storage/provider architecture](https://akash.network/docs/providers/architecture/overview/).
- [Sia s3d current interface](https://sia.storage/s3d) and [upstream local metadata/buffering implementation](https://github.com/SiaFoundation/s3d). These refine the older June-roadmap-only assessment; full client compatibility still needs tests.
- [Garage replication/quorum configuration](https://garagehq.deuxfleurs.fr/documentation/reference-manual/configuration/) and [project features](https://garagehq.deuxfleurs.fr/documentation/reference-manual/features/).
- [Tahoe-LAFS architecture, capabilities and bootstrap](https://tahoe-lafs.readthedocs.io/en/latest/architecture.html).
