# Research / Evaluation Only — BitVMX-CPU spike

> **Experimental BitVMX evaluation only; unaudited; not valid for settlement.**

This document records the deliberately narrow implementation for GitHub issue
#189. It is an evaluation harness, not a production BitVM3, BitVMX-GC, Groth16,
garbled-circuit, settlement, compliance, or gateway integration.

Current release, tag, license, network, upstream-blocker, and readiness status
is maintained in the canonical
[`BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md`](./BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md)
report. This file remains the local runner contract and test-provenance record.

## Decision and exact pin

Evaluate FairgateLabs/BitVMX-CPU only, at the exact source revision:

```text
d390832c8e0f2a01453e8ef4bf65dbe715fb9236
```

This pin is intentional: it is the evaluator's exact source revision and is
not a claim that the newer `v0.8.0` tag or the GitHub `v0.5.11` release is
compatible. The upstream default-branch/tag/release divergence and the
repository-metadata/README license contradiction remain unresolved.

GOATNetwork/bitvm2-gc, garbled circuits, Groth16, recursive proofs, and BitVM3
remain follow-up research, not dependencies of this spike. Upstream source and
binaries are not vendored because public license metadata and checked-in license
text are not treated as reconciled. Operators build the upstream emulator
externally and record the exact revision and executable hash locally.

## Architecture and threat model

`tools/bitvmx-eval` is an independent Rust workspace. The non-default
`bitvmx-eval` feature is required for the runnable binary. The production root
workspace, gateway adapters, API, compliance, auth, metrics, persistence, and
settlement paths do not depend on or invoke it.

The schema-v2 manifest requires fixed safety flags, lowercase executable and
fixture hashes, an exact revision sidecar, and an external network-deny marker.
Before canonicalization, every original input path component is checked for
symlinks and directory/regular-file type. Canonical path and file identity are
rechecked immediately before execution and before post-run collection. Reports
record pre/post hashes and identities; this is not a claim that a general
filesystem race is impossible at the kernel level.

Relative input paths are resolved from the manifest directory; absolute paths
are allowed but receive the same component/type/identity checks. The runner
closes stdin, clears the child environment, captures stdout/stderr
with an aggregate bound, monitors direct-child wall time/RSS, detects Linux
descendant processes, cleans up only verified child-owned processes, and parses
one anchored upstream result line. Artifact files are streamed through hard
per-file and aggregate byte limits and checked against manifest SHA-256 values.
Reports are published through a unique `create_new` temporary file, file sync,
atomic rename, and a Unix parent-directory sync.

Network isolation is **not** implemented by the runner. The caller must launch
it inside an approved external sandbox with network denied and set:

```text
BITVMX_EVAL_SANDBOX_ACTIVE=1
BITVMX_EVAL_SANDBOX_MODE=network-deny
```

These markers are only a preflight protocol; they do not prove that the caller
actually isolated the process.

## Manifest/report contract

The checked-in template is
[`tools/bitvmx-eval/manifests/bitvmx-cpu-eval-v2.json`](../../tools/bitvmx-eval/manifests/bitvmx-cpu-eval-v2.json).
The manifest schema is `bitvmx-eval-manifest-v2`; the report schema is
`bitvmx-eval-report-v2`.

Artifact entries require `name`, `path`, an expected lowercase `sha256`, and a
`max_size_bytes` value. `limits.max_artifact_bytes` and
`limits.max_total_artifact_bytes` provide hard per-artifact and aggregate
ceilings. Defaults are 64 MiB and 256 MiB; the hard artifact ceiling is 1 GiB.
Missing, non-regular, unreadable, oversized, aggregate-limit, and hash-mismatch
artifacts are explicit report failures. No unbounded artifact read is used.

Every report carries:

- the exact warning and fixed `experimental`/non-production flags;
- backend and exact upstream revision;
- the resource scope and whether a descendant process was detected;
- executable and fixture pre/post hashes, sizes, identities, and errors;
- exact pre/post revision-sidecar bytes as hex and exactness flags;
- expected/actual result class and return value, plus limit-step observations;
- command, arguments, wall/CPU/RSS measurements, and output hashes;
- artifact expectations, sizes, hashes, completeness, and errors;
- environment/tool versions, exit status, and failure details;
- `proof_size_bytes: null` and
  `proof_size_reason: "not_applicable_cpu_backend"`.

Once the report destination itself is valid and writable, execution and
post-run failures publish a durable failure report before returning rejection.
The report path cannot alias the executable, sidecar, fixture, or artifact by
canonical path or Unix device/inode identity.

## Resource policy and platform boundary

The Linux manifest must select exactly:

```text
linux-direct-child-only-with-descendant-detection-fail-closed
```

Metrics and limits apply to the direct child only. Linux `/proc` inspection
detects descendants and fails closed if one appears; it is not aggregate
process-tree accounting and not a cgroup guarantee. A dedicated process group
and start-time checks prevent killing an unrelated reused PID/group during
cleanup. Nonblocking Linux pipe readers plus a stop flag prevent rejection
cleanup from waiting indefinitely on inherited output descriptors.

Non-Linux execution is explicitly unavailable in this build. The schema reserves
`non-linux-direct-child-only-weaker-mode`, but the runner rejects it instead of
silently claiming equivalent cleanup or resource semantics.

The default direct-child RSS ceiling is 2.5 GiB, small/scaled timeouts are five
and ten minutes, and aggregate stdout/stderr capture defaults to 1 GiB. Values
can only be tightened in a manifest.

## Result parser and fail-closed cases

Exactly one complete result line is accepted across stdout/stderr:

```text
INFO Execution result: Halt(<u32>, <u64>)
Execution result: Halt(<u32>, <u64>)
INFO Execution result: LimitStepReached(<u64>)
Execution result: LimitStepReached(<u64>)
```

Any other line containing `Execution result:`, multiple candidates, malformed
fields, missing output, wrong class, unexpected return value, or unexpected
limit-step count rejects the evaluation. Other rejection classes include path
component/type violations, exact sidecar/hash/identity failures, process
timeout/RSS/output/descendant/cleanup failures, nonzero exits, and all artifact
failures. There is no `MockGroth16Verifier` fallback.

An expected `halt_failure` is only a deterministic result classification and
still has `cryptographic_verification: false`. A successful report is not proof,
settlement authorization, or compliance approval.

## Test provenance

The integration tests compile `tests/fixtures/helper.rs` into temporary
per-case executables. The helper is synthetic and is **not** upstream BitVMX
execution. No test downloads, vendors, or claims upstream execution. Coverage
includes positive success, expected halt failure, positive limit-reached
results, symlink and non-regular path bypasses, exact sidecar bytes, pre/post
hash and identity mutation/deletion, parser spoofing, same-class return-value
errors, sandbox markers, Linux descendant cleanup, resource limits, artifact
hash/size/missing/read failures, report aliasing, and repeated report writes.

## Graduation criteria

Do not promote this spike into gateway code until licensing is resolved, an
external build is reproducible, positive and negative vectors are independently
reviewed and deterministic, resource use fits the target environment, network
and aggregate process isolation are enforced rather than merely asserted, a
real cryptographic verification contract exists, and a separate security review
approves the intended role. Otherwise retain the **Research / Evaluation Only**
classification.
