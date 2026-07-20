# Source-based test fixtures

These fixtures are intentionally source-only. `helper.rs` is compiled with the
test process's local `rustc` into a temporary per-case directory; no upstream
BitVMX-CPU binary or opaque executable is committed here.

The helper is synthetic and is **not** upstream BitVMX execution. It emits the
synthetic test contract's anchored `Execution result:` lines for positive cases
and deliberately exercises malformed/spoofed output, slow and memory-heavy
processes, oversized output, descendant processes, mutations/deletions, and
nonzero exits for fail-closed boundary tests.
