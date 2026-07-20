# Research / Evaluation Only — BitVMX-CPU spike

> **Experimental BitVMX evaluation only; unaudited; not valid for settlement.**

This document records the approved, deliberately narrow implementation for
GitHub issue #189. It is an evaluation harness, not a production BitVM3 or
BitVMX-GC integration.

## Decision and exact pin

Evaluate FairgateLabs/BitVMX-CPU only, at the exact source revision:

```text
d390832c8e0f2a01453e8ef4bf65dbe715fb9236
```

GOATNetwork/bitvm2-gc, garbled circuits, Groth16, recursive proofs, and BitVM3
remain follow-up research, not dependencies of this spike. Upstream source and
binaries are not vendored because the repository's public license metadata and
checked-in license text are not treated as reconciled. Operators build the
upstream emulator externally, record the exact revision in a sidecar, and pin
the resulting executable SHA-256 in a local manifest.

## Architecture and threat model

`tools/bitvmx-eval` is an independent Rust workspace. The non-default
`bitvmx-eval` feature is required for the runnable binary. The production root
workspace, gateway adapters, API, compliance, auth, metrics, persistence, and
settlement paths do not depend on or invoke it.

The harness validates a versioned manifest, executable hash, revision sidecar,
fixture hash, and fixed safety flags before constructing a direct argv vector.
It closes stdin, clears the child environment, captures stdout/stderr with a
1 GiB aggregate bound, monitors wall time/RSS, and parses only the upstream
`Execution result: Halt(value, step)` and `LimitStepReached(step)` shapes.
Reports record measurements and hashes but never assert proof validity.

The primary threats are a wrong or replaced executable, corrupted fixture,
unbounded resource use, output parser confusion, accidental network access, and
result-class overclaiming. Hashes, revision sidecars, post-run rehashing,
timeouts, RSS/output limits, strict parsing, and fixed non-production report
flags address the first five partially. A path-level TOCTOU window and host
process-monitoring limitations remain.

Network isolation is **not** implemented portably by the runner. The caller
must launch it inside an approved external sandbox with network denied and set:

```text
BITVMX_EVAL_SANDBOX_ACTIVE=1
BITVMX_EVAL_SANDBOX_MODE=network-deny
```

The runner fails closed if either marker is absent or wrong. These markers are
only a preflight protocol; they do not prove that the caller actually isolated
the process. The exact behavior and platform caveat are documented in
[`tools/bitvmx-eval/README.md`](../../tools/bitvmx-eval/README.md).

## Manifest/report contract

The checked-in template is
[`tools/bitvmx-eval/manifests/bitvmx-cpu-eval-v1.json`](../../tools/bitvmx-eval/manifests/bitvmx-cpu-eval-v1.json).
Manifest schema is `bitvmx-eval-manifest-v1`; report schema is
`bitvmx-eval-report-v1`.

Every report carries:

- `experimental: true`, `production_supported: false`,
  `cryptographic_verification: false`;
- backend and exact upstream revision;
- executable and fixture identity/SHA-256 values;
- expected and actual result class;
- exact command and arguments;
- wall time, best-effort CPU timings, maximum RSS, and parseable executed steps;
- captured output and selected artifact sizes/SHA-256 hashes;
- OS/tool versions and process exit status;
- `proof_size_bytes: null` and
  `proof_size_reason: "not_applicable_cpu_backend"`.

The exact warning string must remain unchanged:

```text
Experimental BitVMX evaluation only; unaudited; not valid for settlement.
```

## Resource policy and fail-closed cases

The defaults are 2.5 GiB maximum RSS, five minutes for `small`, ten minutes
for `scaled`, and 1 GiB aggregate captured output. A manifest may request
stricter values, never looser ones. The harness rejects missing/non-regular
files, symlinks, hash or revision mismatches, malformed manifests or input,
fixture corruption, nonzero exits, timeout/RSS/output-limit violations,
malformed/unrecognized output, post-run file changes, and unexpected result
classes. It has no `MockGroth16Verifier` fallback and no success-to-verification
mapping.

## Reproducible external build and benchmark

Build the upstream emulator from a detached checkout of the exact pin, hash the
result, and create the required revision sidecar. Do not commit that binary.
See the command sequence and license caveat in the tool README.

For a benchmark, use one warmup and five small samples, then three samples for
an approximately 100K-step fixture. Run sequentially with `/usr/bin/time -v`.
Record median/min/max/stddev for wall and CPU time, RSS, steps, output/artifact
sizes, and hashes. Compare execute-only against trace/checkpoint modes and
measure the wrapper's process/serialization overhead separately. Trace bytes
must never be described as proof bytes.

## Graduation criteria

Do not promote this spike into gateway code until licensing is resolved, an
external build is reproducible, positive and negative vectors are independently
reviewed and deterministic, resource use fits the target environment, network
and process isolation are enforced rather than merely asserted, a real
cryptographic verification contract exists, and a separate security review
approves the intended role. Otherwise retain the **Research / Evaluation Only**
classification.
