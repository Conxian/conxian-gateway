#!/usr/bin/env bash
set -euo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=tests/rgb/rgb_artifact_guard.sh
source "${SCRIPT_DIR}/rgb_artifact_guard.sh"

readonly TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/conxian-rgb-guard-test.XXXXXX")"
readonly UPLOAD_ROOT="${TEST_ROOT}/rgb-regtest-artifacts"
readonly QUARANTINE_ROOT="${TEST_ROOT}/.rgb-regtest-quarantine"
readonly WORK_ROOT="${TEST_ROOT}/work"
readonly SECRET_PATTERN="${WORK_ROOT}/secret.pattern"
readonly CAPTURE="${WORK_ROOT}/guard-output"
SECRET_VALUE="regtest-guard-fixture-$RANDOM-$RANDOM"

cleanup() {
    chmod -R u+rwX -- "$TEST_ROOT" 2>/dev/null || true
    rm -rf -- "$TEST_ROOT"
}
trap cleanup EXIT

mkdir -m 700 -p -- "$UPLOAD_ROOT" "$QUARANTINE_ROOT" "$WORK_ROOT"
(umask 077 && printf '%s\n' "$SECRET_VALUE" >"$SECRET_PATTERN")

new_run() {
    rm -rf -- "$UPLOAD_ROOT/run.test"
    mkdir -m 700 -- "$UPLOAD_ROOT/run.test"
}

assert_failed_closed() {
    local expected_reason="$1"
    [[ -f "$UPLOAD_ROOT/run.test/credential-leak-guard.txt" ]]
    grep -Fq -- 'failed: artifact run quarantined by fail-closed credential guard' \
        "$UPLOAD_ROOT/run.test/credential-leak-guard.txt"
    grep -Fq -- "reason=${expected_reason}" "$UPLOAD_ROOT/run.test/credential-leak-guard.txt"
    [[ "$(find -P "$UPLOAD_ROOT/run.test" -mindepth 1 -printf '.' | wc -c)" -eq 1 ]]
    ! grep -aFq -f "$SECRET_PATTERN" -- "$CAPTURE" "$UPLOAD_ROOT/run.test/credential-leak-guard.txt"
}

run_guard_expect_failure() {
    local expected_reason="$1"
    shift
    : >"$CAPTURE"
    if "$@" >"$CAPTURE" 2>&1; then
        printf '%s\n' "guard unexpectedly passed: ${expected_reason}" >&2
        return 1
    fi
    assert_failed_closed "$expected_reason"
}

new_run
printf 'prefix\0%s\0suffix' "$SECRET_VALUE" >"$UPLOAD_ROOT/run.test/readable.bin"
run_guard_expect_failure credential-content-match \
    rgb_guard_retained_artifacts "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT" "$SECRET_PATTERN" "$WORK_ROOT"

new_run
printf '%s\n' 'ordinary artifact' >"$UPLOAD_ROOT/run.test/prefix-${SECRET_VALUE}-suffix"
run_guard_expect_failure credential-path-match \
    rgb_guard_retained_artifacts "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT" "$SECRET_PATTERN" "$WORK_ROOT"

new_run
printf '%s\n' 'ordinary artifact' >"$UPLOAD_ROOT/run.test/.cookie"
run_guard_expect_failure forbidden-cookie-file \
    rgb_guard_retained_artifacts "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT" "$SECRET_PATTERN" "$WORK_ROOT"

new_run
printf '%s' "$SECRET_VALUE" >"$UPLOAD_ROOT/run.test/unreadable.bin"
chmod 000 "$UPLOAD_ROOT/run.test/unreadable.bin"
run_guard_expect_failure unreadable-file \
    rgb_guard_retained_artifacts "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT" "$SECRET_PATTERN" "$WORK_ROOT"

new_run
ln -s -- /dev/null "$UPLOAD_ROOT/run.test/unexpected-link"
run_guard_expect_failure unexpected-symlink \
    rgb_guard_retained_artifacts "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT" "$SECRET_PATTERN" "$WORK_ROOT"

new_run
mkfifo -- "$UPLOAD_ROOT/run.test/unexpected-fifo"
run_guard_expect_failure unexpected-filesystem-object \
    rgb_guard_retained_artifacts "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT" "$SECRET_PATTERN" "$WORK_ROOT"

new_run
mkdir -m 700 -- "$WORK_ROOT/failing-bin"
cat >"$WORK_ROOT/failing-bin/grep" <<'EOF'
#!/usr/bin/env bash
exit 2
EOF
chmod 700 "$WORK_ROOT/failing-bin/grep"
printf '%s\n' 'ordinary artifact' >"$UPLOAD_ROOT/run.test/ordinary.log"
run_guard_expect_failure credential-scanner-error \
    env PATH="$WORK_ROOT/failing-bin:$PATH" bash -c \
        'source "$1"; rgb_guard_retained_artifacts "$2" "$3" "$4" "$5"' \
        _ "${SCRIPT_DIR}/rgb_artifact_guard.sh" "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT" "$SECRET_PATTERN" "$WORK_ROOT"

new_run
printf '%s\n' 'sanitized diagnostic' >"$WORK_ROOT/debug.log"
cat >"$WORK_ROOT/failing-bin/cp" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
chmod 700 "$WORK_ROOT/failing-bin/cp"
run_guard_expect_failure diagnostic-copy-error \
    env PATH="$WORK_ROOT/failing-bin:$PATH" bash -c \
        'source "$1"; rgb_copy_guarded_diagnostic "$2" "$3" "$4" "$5"' \
        _ "${SCRIPT_DIR}/rgb_artifact_guard.sh" "$WORK_ROOT/debug.log" \
        "$UPLOAD_ROOT/run.test/bitcoin-debug.log" "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT"

new_run
printf '%s\n' 'ordinary artifact' >"$UPLOAD_ROOT/run.test/ordinary.log"
: >"$CAPTURE"
rgb_guard_retained_artifacts "$UPLOAD_ROOT/run.test" "$QUARANTINE_ROOT" "$SECRET_PATTERN" "$WORK_ROOT" \
    >"$CAPTURE" 2>&1
grep -Fq -- 'passed: artifact tree is readable and regular' \
    "$UPLOAD_ROOT/run.test/credential-leak-guard.txt"
! grep -aFq -f "$SECRET_PATTERN" -- "$CAPTURE" "$UPLOAD_ROOT/run.test/credential-leak-guard.txt"
[[ -f "$UPLOAD_ROOT/run.test/ordinary.log" ]]

printf '%s\n' 'RGB artifact guard regression scenarios passed'
