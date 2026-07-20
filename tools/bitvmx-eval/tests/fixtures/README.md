# Source-based test fixtures

These fixtures are intentionally source-only. `helper.rs` is compiled with the
test process's local `rustc` into a temporary test directory; no upstream
BitVMX-CPU binary or opaque executable is committed here.

The helper emits the pinned upstream emulator's documented `Execution result:`
shape for positive cases and deliberately malformed, slow, memory-heavy,
oversized-output, and nonzero-exit cases for fail-closed boundary tests.
