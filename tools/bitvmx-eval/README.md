# Research / Evaluation Only — `bitvmx-eval`

> **Experimental BitVMX evaluation only; unaudited; not valid for settlement.**

This directory contains an isolated evaluation lane for FairgateLabs/BitVMX-CPU.
It is deliberately **not** a BitVM3, BitVMX-GC, garbled-circuit, Groth16, proof,
settlement, compliance, or gateway integration. The tool executes an externally
built upstream CLI as a bounded subprocess and records a versioned local JSON
report. It never maps a result to `verified: true`.

## Scope and boundary

The crate has an independent Cargo workspace and lockfile. Its only runnable
binary target requires the non-default `bitvmx-eval` feature. The default build
and test path compiles no evaluator binary and runs no evaluator behavior.

The production gateway dependency graph is intentionally untouched:

- no dependency or source changes in the root workspace;
- no changes in `internal/engine`, `internal/compliance`, `internal/api`, or
  `cmd/gateway`;
- no HTTP route, authentication, Prometheus metric, persistence, adapter, or
  settlement path;
- only an operator-selected local report is written.

The execution boundary is:

```text
schema-v2 manifest
        |
        v
preflight: fixed paths + hashes + exact revision bytes + sandbox markers
        |
        v
direct argv subprocess (no shell) + stdin closed + bounded stdout/stderr
        |
        v
Linux direct-child metrics + descendant detection + bounded cleanup
        |
        v
strict result parser + bounded streamed artifact collection
        |
        v
atomic, durable success or failure report
```

## Exact upstream pin and external build

The only accepted upstream revision is:

```text
FairgateLabs/BitVMX-CPU
d390832c8e0f2a01453e8ef4bf65dbe715fb9236
```

Upstream source and binaries are not vendored or redistributed by this
repository. Build the binary outside this repository, inspect the upstream
license metadata, and retain the source checkout for provenance:

```bash
git clone https://github.com/FairgateLabs/BitVMX-CPU.git /tmp/BitVMX-CPU
git -C /tmp/BitVMX-CPU checkout --detach d390832c8e0f2a01453e8ef4bf65dbe715fb9236
cargo build --release --manifest-path /tmp/BitVMX-CPU/emulator/Cargo.toml
install -m 0755 /tmp/BitVMX-CPU/target/release/emulator ./manifests/bin/bitvmx-cpu
sha256sum ./manifests/bin/bitvmx-cpu
printf '%s\n' d390832c8e0f2a01453e8ef4bf65dbe715fb9236 \
  > ./manifests/bin/bitvmx-cpu.revision
```

The schema-v2 manifest must contain the actual lowercase executable SHA-256.
The `.revision` sidecar must contain **exactly** the 40-byte pinned commit,
followed by one newline byte. Whitespace variants, missing newlines, extra
newlines, and other encodings are rejected. The sidecar is an operator
provenance assertion, not a cryptographic proof that a binary was built from
that source; retain the executable and source checkout together.

The upstream repository's public metadata is not treated as settled licensing:
its README and repository metadata have differed from the checked-in license
text. Do not vendor or redistribute it until the discrepancy is resolved by the
project and reviewed by the appropriate owner.

## Manifest and invocation

Start from [`manifests/bitvmx-cpu-eval-v2.json`](manifests/bitvmx-cpu-eval-v2.json),
replace the operator-supplied executable, sidecar, fixture, and SHA-256 values,
then run the feature-gated tool:

```bash
BITVMX_EVAL_SANDBOX_ACTIVE=1 \
BITVMX_EVAL_SANDBOX_MODE=network-deny \
cargo run \
  --manifest-path tools/bitvmx-eval/Cargo.toml \
  --features bitvmx-eval -- \
  --manifest tools/bitvmx-eval/manifests/bitvmx-cpu-eval-v2.json \
  --report /tmp/bitvmx-eval/report.json
```

The report parent directory must already exist, must be a non-symlink directory,
and must be writable. Report paths containing `..` are rejected. The report
path must not alias the executable, revision sidecar, fixture, or any artifact;
both canonical paths and, on Unix, device/inode identity are checked.

Relative input paths are resolved relative to the manifest directory; absolute
paths are accepted but still undergo the same component/type/identity checks.
Before
canonicalization, every original path component is inspected with
`symlink_metadata`; symlink components, non-directory parents, and non-regular
final files are rejected. The runner re-checks the original path, canonical
path, regular-file type, and file identity immediately before execution and
again before post-run hashing or artifact collection. This is a bounded path
and identity check, not a claim of kernel-level elimination of every possible
filesystem race.

The runner constructs the fixed upstream `execute --elf ...` argument vector
itself; arbitrary shell text and arbitrary command strings are not accepted.

## Sandbox and resource scope

The environment markers are an admission preflight only. This tool does **not**
implement portable network namespaces, seccomp, containers, filesystem
read-only mounts, cgroups, or any other network sandbox. The caller must launch
the process inside an approved external sandbox that denies network access,
then set both markers inside that already-active sandbox. A marker by itself is
not evidence of isolation and must not be treated as one.

The supported Linux manifest profile names its exact accounting scope:

```text
linux-direct-child-only-with-descendant-detection-fail-closed
```

Wall time, CPU time, and RSS metrics/limits apply to the direct evaluator child
only. On Linux, `/proc` process-tree inspection detects descendants; any
detected descendant rejects the evaluation before the result can be accepted.
The child is placed in a dedicated process group, and cleanup verifies process
identity before killing observed descendants. This is **not** aggregate
process-tree resource accounting and is **not** a cgroup guarantee.

Non-Linux execution is explicitly unavailable in this build. The schema reserves
the clearly named `non-linux-direct-child-only-weaker-mode`, but the runner
refuses it rather than silently providing weaker cleanup or resource semantics.

The child receives a cleared environment with only a minimal `PATH`, `LANG=C`,
and `LC_ALL=C`. Standard input is closed. Linux output readers use nonblocking
pipes and a stop flag so rejection cleanup cannot leave reader threads blocked
indefinitely.

## Schema-v2 limits and artifacts

Manifest schema is `bitvmx-eval-manifest-v2`; report schema is
`bitvmx-eval-report-v2`. Artifact entries require all of:

```json
{
  "name": "trace",
  "path": "./reports/trace.bin",
  "sha256": "<64 lowercase hex characters>",
  "max_size_bytes": 16777216
}
```

`limits.max_artifact_bytes` is a hard per-artifact ceiling and
`limits.max_total_artifact_bytes` is a hard aggregate ceiling. If omitted, the
defaults are 64 MiB and 256 MiB respectively; neither can exceed the 1 GiB
hard cap. Artifact bytes are read and hashed in bounded chunks. The runner
never uses an unbounded `fs::read()` for artifacts. Missing, non-regular,
unreadable, oversized, aggregate-limit, and hash-mismatch artifacts are
classified in the report and reject the evaluation.

Captured stdout/stderr has a separate aggregate bound, defaulting to 1 GiB.
The default direct-child RSS ceiling is 2.5 GiB; small and scaled workloads
default to five and ten minutes. Manifest values can only tighten those
defaults.

## Result parser

Exactly one anchored result candidate is accepted across stdout and stderr:

```text
INFO Execution result: Halt(<u32>, <u64>)
Execution result: Halt(<u32>, <u64>)
INFO Execution result: LimitStepReached(<u64>)
Execution result: LimitStepReached(<u64>)
```

The complete line must match one of those shapes. Any other line containing
`Execution result:`, any multiple candidate lines, malformed fields, or an
unrecognized result rejects the evaluation. The expected result class and,
when applicable, exact return value and limit step count are checked.

## Report contract and failure durability

Every report carries the exact warning and fixed non-production flags; backend,
upstream revision, resource scope, and descendant-detection status; pre/post
executable and fixture hashes, sizes, identities, and errors; exact pre/post
revision-sidecar observations as hex; result class, command, arguments,
expected/actual return values, measurements, bounded output hashes, artifact
expectations/observations, OS/tool versions, exit status, and an explicit failure
class plus detailed observations.

After the report destination itself passes validation, execution and post-run
failures publish a durable failure report whenever that destination is writable.
Reports are serialized to a unique `create_new` temporary file, synced, renamed
into place, and followed by a parent-directory sync on Unix. A report path that
aliases an input is rejected before any report write so an input cannot be
overwritten.

The exact warning string is present in CLI help, normal CLI output, every
published report, and this documentation:

```text
Experimental BitVMX evaluation only; unaudited; not valid for settlement.
```

## Fail-closed behavior

The runner rejects malformed or unsupported manifests, wrong sandbox markers,
unsupported resource scopes, missing/non-regular/symlinked inputs, execute
permission failures, exact sidecar mismatches, input hash mismatches, identity
changes, process setup failures, timeouts, direct-child RSS/output limits,
descendant processes, cleanup failures, nonzero exits, malformed/unrecognized
output, unexpected result classes/return values/limit counts, and all artifact
path/read/size/hash failures. There is no mock verifier fallback.

An expected `halt_failure` is only a deterministic result classification and
still has `cryptographic_verification: false`. A successful report is not proof,
settlement authorization, or compliance approval.

## Tests and provenance

The integration tests compile [`tests/fixtures/helper.rs`](tests/fixtures/helper.rs)
from source into temporary per-case executables. The helper is synthetic and is
**not** upstream BitVMX execution. Tests cover positive success, expected halt
failure, positive limit-reached results, path-type/symlink bypasses, exact
sidecar bytes, hash and identity mutation/deletion, parser spoofing, unexpected
return values, sandbox markers, descendant cleanup, process limits, artifact
hash/size/missing/read failures, report aliasing, and repeated report publication.

Run the checks from this directory or the repository root:

```bash
cargo fmt --manifest-path tools/bitvmx-eval/Cargo.toml --all -- --check
cargo build --manifest-path tools/bitvmx-eval/Cargo.toml
cargo test --manifest-path tools/bitvmx-eval/Cargo.toml
cargo build --manifest-path tools/bitvmx-eval/Cargo.toml --features bitvmx-eval
cargo test --manifest-path tools/bitvmx-eval/Cargo.toml --features bitvmx-eval
cargo clippy --manifest-path tools/bitvmx-eval/Cargo.toml \
  --features bitvmx-eval --all-targets -- -D warnings
```

## Limitations and graduation criteria

This spike does not implement or claim BitVM3, BitVMX-GC, garbled-circuit
generation/verification, Groth16/DV-SNARK or recursive proof verification,
Bitcoin Script deployment, network isolation, cgroup or aggregate process-tree
resource guarantees, security audit guarantees, production settlement,
compliance approval, or gateway integration.

Graduation requires resolved upstream licensing, a reproducible external build,
independently reviewed source and binary provenance, stable positive and
negative vectors, resource behavior acceptable on the target CI class, stronger
sandbox enforcement and aggregate accounting, a defined cryptographic
verification contract, and a separate security review. Until those gates are
met, keep this lane labeled **Research / Evaluation Only**.
