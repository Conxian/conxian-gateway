#!/usr/bin/env bash
set -euo pipefail

THRESHOLD="${1:-90}"
OUTPUT_DIR="${2:-target/lightning-coverage}"
JSON_REPORT="$OUTPUT_DIR/llvm-cov.json"

mkdir -p "$OUTPUT_DIR"

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "Error: cargo-llvm-cov is required but not installed. To install, run: cargo install cargo-llvm-cov --version 0.8.7 --locked" >&2
  exit 1
fi

cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --all-features --json --output-path "$JSON_REPORT"
cargo llvm-cov --workspace --all-features --lcov --output-path "$OUTPUT_DIR/lcov.info"
cargo llvm-cov --workspace --all-features --html --output-dir "$OUTPUT_DIR/html"

python3 scripts/lightning_coverage_report.py \
  --input "$JSON_REPORT" \
  --threshold "$THRESHOLD" \
  --output-dir "$OUTPUT_DIR"
