# Research / Evaluation Only — `bitvmx-eval`

> **Experimental BitVMX evaluation only; unaudited; not valid for settlement.**

This directory contains an isolated evaluation lane for FairgateLabs/BitVMX-CPU.
It is deliberately **not** a BitVM3, BitVMX-GC, garbled-circuit, Groth16, proof,
settlement, compliance, or gateway integration. The tool executes an externally
built upstream CLI as a bounded subprocess and records a versioned local JSON
report. It never returns or maps a result to `verified: true`.

## Scope and boundary

The crate has an independent Cargo workspace and lockfile. Its only runnable
binary target requires the non-default `bitvmx-eval` feature. The default build
and test path compiles no evaluator binary and runs no evaluator behavior.

The production gateway dependency graph is intentionally untouched:

- no dependency or source changes in the root workspace;
- no changes to `internal/engine`, `internal/compliance`, `internal/api`, or
  `cmd/gateway`;
- no HTTP route, authentication, Prometheus metric, persistence, adapter, or
  settlement path;
- only local versioned reports and operator-selected artifact hashes are written.

The execution boundary is:

```text
versioned manifest
        |
        v
preflight: schema + hashes + revision sidecar + fixture + sandbox marker
        |
        v
direct argv subprocess (no shell) + stdin closed + bounded stdout/stderr
        |
        v
timeout/RSS/output monitoring + strict upstream result parser
        |
        v
versioned local report (always experimental and non-cryptographic)
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

The manifest must contain the actual lowercase SHA-256 from that build. The
`.revision` sidecar must contain exactly the pinned commit followed by a
newline. The sidecar is an operator provenance assertion, not a cryptographic
proof that a binary was built from that source; the executable hash and source
checkout must be retained together. A binary replacement is detected by a
second hash after execution, but a residual path-level TOCTOU risk remains.

The upstream repository's public metadata is not treated as settled licensing:
its README and repository metadata have differed from the checked-in license
text. Do not vendor or redistribute it until the license discrepancy is
resolved by the project and reviewed by the appropriate owner.

## Manifest and invocation

Start from [`manifests/bitvmx-cpu-eval-v1.json`](manifests/bitvmx-cpu-eval-v1.json),
replace the operator-supplied executable, sidecar, fixture, and SHA-256 values,
then run the feature-gated tool:

```bash
BITVMX_EVAL_SANDBOX_ACTIVE=1 \
BITVMX_EVAL_SANDBOX_MODE=network-deny \
cargo run \
  --manifest-path tools/bitvmx-eval/Cargo.toml \
  --features bitvmx-eval -- \
  --manifest tools/bitvmx-eval/manifests/bitvmx-cpu-eval-v1.json \
  --report /tmp/bitvmx-eval/report.json
```

Paths inside a manifest are resolved relative to the manifest directory. The
runner constructs the fixed upstream `execute --elf ...` argument vector itself;
arbitrary shell text and arbitrary command strings are not accepted.

The environment markers are an admission preflight only. This tool does **not**
implement portable network namespaces, seccomp, containers, filesystem
read-only mounts, or other sandbox enforcement. The caller must launch the
process inside an approved external sandbox that denies network access, then
set both markers inside that already-active sandbox. If either marker is absent
or different, the tool fails before spawning the evaluator. A marker by itself
is not evidence of isolation and must not be treated as one.

The child receives a cleared environment with only a minimal `PATH`, `LANG=C`,
and `LC_ALL=C`. Standard input is closed. On Unix, the child is placed in its
own process group so timeout, RSS, and output-limit aborts can kill the group;
other platforms use the direct-child kill primitive and provide weaker process
tree coverage.

## Report contract

Reports use `schema_version: "bitvmx-eval-report-v1"` and include, at minimum:

| Field | Meaning |
| --- | --- |
| `experimental`, `production_supported`, `cryptographic_verification` | Fixed to `true`, `false`, `false`. |
| `backend`, `upstream_revision` | `bitvmx-cpu` and the exact 40-character upstream pin. |
| `executable_sha256` | Hash checked before and after the subprocess. |
| `fixture` | Fixture ID, kind, resolved path, and checked SHA-256. |
| `expected_result_class`, `actual_result_class` | Strictly classified `halt_success`, `halt_failure`, or `limit_reached`; no verification meaning. |
| `executed_command`, `arguments` | The direct executable path and exact argv values. |
| `wall_time_ms`, CPU timings, `maximum_rss_bytes` | Best-effort process measurements; CPU/RSS may be `null` where the host cannot expose them. |
| `executed_steps` | Parsed from the upstream `Halt(_, step)` or `LimitStepReached(step)` result when available. |
| `outputs`, `artifacts` | Sizes and SHA-256 hashes; captured output is marked incomplete if a limit aborts it. |
| `environment` | OS, architecture, kernel where available, Rust/Cargo versions, wrapper version, profile, and CPU count. |
| `exit_status` and `failure` | Process status and explicit fail-closed reason. |
| `proof_size_bytes` and `proof_size_reason` | Always `null` and `not_applicable_cpu_backend`. Trace bytes are never proof bytes. |

The exact warning string is present in CLI help, normal CLI output, every
successful execution report, and this documentation:

```text
Experimental BitVMX evaluation only; unaudited; not valid for settlement.
```

## Fail-closed behavior

The runner rejects malformed or unsupported manifests, missing or non-regular
executables/fixtures, symlinks, missing execute permission, revision-sidecar
mismatches, executable or fixture hash mismatches, corrupted fixtures, missing
sandbox preflight markers, malformed input hex, nonzero exits, timeouts, RSS
limits, aggregate captured-output limits, output that is not the recognized
upstream result shape, post-run executable/fixture changes, and unexpected
result classes or return values. There is no mock verifier fallback.

Defaults are intentionally conservative and can only be tightened per
manifest:

- small workload timeout: **5 minutes**;
- scaled workload timeout: **10 minutes**;
- maximum RSS: **2.5 GiB**;
- aggregate captured stdout/stderr: **1 GiB**.

The wrapper does not interpret a successful halt as a proof or settlement
authorization. An expected `halt_failure` is merely a deterministic result
classification and still has `cryptographic_verification: false`.

## Tests and provenance

The integration tests compile [`tests/fixtures/helper.rs`](tests/fixtures/helper.rs)
from source into a temporary executable. They never download, vendor, or run an
upstream binary. The helper emits the upstream CLI's documented result shape
for two positive paths and intentionally exercises missing binary, revision and
hash mismatch, corrupted fixture, malformed output, nonzero exit, timeout, RSS
limit, output limit, malformed manifest, and unexpected-result paths.

Run the checks from this directory or the repository root:

```bash
cargo fmt --manifest-path tools/bitvmx-eval/Cargo.toml --all -- --check
cargo test --manifest-path tools/bitvmx-eval/Cargo.toml
cargo test --manifest-path tools/bitvmx-eval/Cargo.toml --features bitvmx-eval
cargo clippy --manifest-path tools/bitvmx-eval/Cargo.toml \
  --features bitvmx-eval --all-targets -- -D warnings
```

## Benchmark procedure

Benchmarks are manual and must run in the same externally network-denied
sandbox policy. Use one warmup, then five sequential small-fixture samples,
followed by three sequential 100K-step samples. Capture both wrapper reports
and `/usr/bin/time -v` output:

```bash
for sample in warmup 1 2 3 4 5; do
  /usr/bin/time -v \
    env BITVMX_EVAL_SANDBOX_ACTIVE=1 BITVMX_EVAL_SANDBOX_MODE=network-deny \
    cargo run --quiet --manifest-path tools/bitvmx-eval/Cargo.toml \
      --features bitvmx-eval -- \
      --manifest ./manifests/small.json \
      --report "./reports/small-${sample}.json"
done
```

Repeat with three `scaled` manifests whose fixture executes approximately
100,000 steps. Report median/min/max/stddev for wall time, user/system CPU
time, maximum RSS, executed steps, output/artifact sizes, and hashes. Keep the
warmup separate. Compare execute-only with trace/checkpoint configurations and
measure wrapper process/serialization overhead separately. `/usr/bin/time -v`
is an independent host measurement; the report's process metrics are
best-effort and may differ.

## Limitations and graduation criteria

This spike does not implement or claim:

- BitVM3 or BitVMX-GC;
- garbled-circuit generation or verification;
- Groth16, DV-SNARK, recursive proof, or cryptographic verification;
- Bitcoin Script deployment, challenge protocols, testnet/mainnet activity;
- network isolation, sandbox enforcement, or security audit guarantees;
- production settlement, compliance approval, or gateway integration.

Graduation requires resolved upstream licensing, a reproducible external build
process, independently reviewed source and binary provenance, stable positive
and negative vectors, resource behavior acceptable on the target CI class,
stronger sandbox enforcement, a defined cryptographic verification contract,
and a separate security review. Until all of those gates are met, keep this
lane labeled **Research / Evaluation Only**.
