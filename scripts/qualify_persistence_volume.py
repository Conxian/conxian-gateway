#!/usr/bin/env python3
"""Run Gateway's real file-backend tests on a selected volume, without live state."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = (
    'stale_revision_conflict_preserves_committed_state',
    'post_rename_sync_failure_reports_unknown_commit_without_retrying',
    'subprocess_ownership_exclusion_and_release',
    'subprocess_same_revision_cas_has_one_winner',
    'process_crash_restart_observes_complete_old_or_new_envelope',
    'corrupt_and_unsupported_state_files_are_rejected',
    'cas_atomically_replaces_state_and_leaves_no_temporary_file',
)


def run_native(command, **options):
    timeout = options.pop('timeout')
    options.pop('check', None)
    process = subprocess.Popen(command, start_new_session=True, **options)
    try:
        output, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        # Cargo and its test/compiler children share a fresh process group.
        # Never leave them writing after reporting the probe timed out.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.communicate(timeout=5)
        raise
    return subprocess.CompletedProcess(command, process.returncode, output)


def assess(returncode, output):
    # Crash/ownership tests launch child test harnesses. Their output interrupts
    # parent test lines and includes additional one-test summaries. Only the final
    # outer harness result, coupled with Cargo's exit code, can settle the suite.
    started = set(re.findall(r'^test persistence::tests::(\w+) \.\.\.(?: ok)?\s*$', output, re.M))
    last_line = output.rstrip().splitlines()[-1] if output.strip() else ''
    summary = re.match(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', last_line)
    passed = bool(summary) and int(summary[1]) >= len(REQUIRED)
    passed = passed and summary.groups()[1:] == ('0', '0')
    return returncode == 0 and passed and set(REQUIRED) <= started, sorted(set(REQUIRED) - started)


def qualify(directory, timeout):
    report = {'schema_version': 1, 'component_tests_passed': False,
              'production_qualified': False, 'power_loss_verified': False,
              'distributed_fencing_verified': False, 'independent_restore_verified': False}
    if os.name != 'posix' or not directory.is_dir():
        return {**report, 'error': 'existing_unix_volume_directory_required'}
    cargo = shutil.which('cargo')
    if not cargo:
        return {**report, 'error': 'repo_pinned_rust_toolchain_required'}
    try:
        report['persistence_source_sha256'] = hashlib.sha256(
            (ROOT / 'pkg/conxian-core/src/persistence.rs').read_bytes()).hexdigest()
        report['cargo_lock_sha256'] = hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest()
    except OSError:
        return {**report, 'error': 'native_source_or_lockfile_unavailable'}
    # Keep probe files/evidence for inspection. Never open the live state directory's files.
    try:
        work = Path(tempfile.mkdtemp(prefix='.conxian-volume-conformance-', dir=directory.resolve()))
    except OSError:
        return {**report, 'error': 'cannot_create_isolated_probe_directory'}
    report['evidence_directory'] = str(work)
    command = [cargo, 'test', '--locked', '--package', 'conxian_core', '--lib',
               'persistence::tests::', '--', '--test-threads=1', '--color', 'never']
    env = {**os.environ, 'TMPDIR': str(work), 'CARGO_TERM_COLOR': 'never'}
    try:
        result = run_native(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, timeout=timeout, check=False)
    except (OSError, subprocess.TimeoutExpired):
        return {**report, 'error': 'native_suite_unavailable_or_timed_out'}
    # Logs stay in the private, newly created probe directory, not in public reports.
    log_path = work / 'native-suite.log'
    with log_path.open('x', encoding='utf-8') as log:
        os.chmod(log_path, 0o600)
        log.write(result.stdout)
    passed, missing = assess(result.returncode, result.stdout)
    return {**report, 'component_tests_passed': passed,
            'native_exit_code': result.returncode, 'missing_required_tests': missing}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True,
                        help='Existing mounted directory; only a new private child is used.')
    parser.add_argument('--timeout', type=int, default=900)
    args = parser.parse_args()
    if not 1 <= args.timeout <= 3600:
        parser.error('--timeout must be between 1 and 3600 seconds')
    report = qualify(args.directory, args.timeout)
    print(json.dumps(report, sort_keys=True))
    return 0 if report['component_tests_passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
