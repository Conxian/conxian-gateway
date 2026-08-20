#!/usr/bin/env bash
# ==============================================================================
# Model Context Protocol (MCP) Agentic Test Runner
#
# Provides structured JSON output and standardized execution commands for
# MCP agentic test runners and autonomous verification loops.
# ==============================================================================

set -euo pipefail

TARGET_TEST="wiremock_simulation_tests"
PACKAGE="gateway"
JSON_OUTPUT=true
RUN_ALL=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --test)
      TARGET_TEST="$2"
      shift 2
      ;;
    --package)
      PACKAGE="$2"
      shift 2
      ;;
    --all)
      RUN_ALL=true
      shift
      ;;
    --no-json)
      JSON_OUTPUT=false
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

START_TIME=$(date +%s)

if [ "$RUN_ALL" = true ]; then
  TEST_CMD="cargo test --workspace --features mock-integrations"
else
  TEST_CMD="cargo test --test $TARGET_TEST -p $PACKAGE --features mock-integrations -- --nocapture"
fi

OUTPUT_FILE=$(mktemp)
STATUS="PASSED"
RC=0

if ! $TEST_CMD > "$OUTPUT_FILE" 2>&1; then
  STATUS="FAILED"
  RC=1
fi

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

TOTAL_TESTS=$(grep -E "test result: .*" "$OUTPUT_FILE" | grep -oE "[0-9]+ passed" | grep -oE "[0-9]+" || echo "0")
FAILED_TESTS=$(grep -E "test result: .*" "$OUTPUT_FILE" | grep -oE "[0-9]+ failed" | grep -oE "[0-9]+" || echo "0")

if [ "$JSON_OUTPUT" = true ]; then
  cat << JSON_EOF
{
  "mcp_runner_version": "1.0.0",
  "status": "$STATUS",
  "target_package": "$PACKAGE",
  "target_test": "$TARGET_TEST",
  "duration_seconds": $DURATION,
  "summary": {
    "total_passed": ${TOTAL_TESTS:-0},
    "total_failed": ${FAILED_TESTS:-0}
  },
  "exit_code": $RC
}
JSON_EOF
else
  cat "$OUTPUT_FILE"
fi

rm -f "$OUTPUT_FILE"
exit $RC
