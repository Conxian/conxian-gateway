import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('volume', Path(__file__).parents[1] / 'qualify_persistence_volume.py')
volume = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(volume)


def success():
    return '\n'.join(f'test persistence::tests::{name} ... ok' for name in volume.REQUIRED) + '\ntest result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s\n'


class VolumeConformanceTests(unittest.TestCase):
    def test_native_timeout_terminates_isolated_process_group(self):
        with self.assertRaises(subprocess.TimeoutExpired):
            volume.run_native([sys.executable, '-c', 'import time; time.sleep(30)'],
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              text=True, timeout=0.1, check=False)

    def test_nested_child_harnesses_cannot_replace_outer_result(self):
        nested = success().replace(
            f'test persistence::tests::{volume.REQUIRED[2]} ... ok',
            f'test persistence::tests::{volume.REQUIRED[2]} ... \n'
            'running 1 test\ntest persistence::tests::subprocess_worker ... ok\n'
            'test result: ok. 1 passed; 0 failed; 0 ignored;\n\nok')
        self.assertTrue(volume.assess(0, nested)[0])
        self.assertFalse(volume.assess(0, nested + 'test result: ok. 1 passed; 0 failed; 0 ignored;\n')[0])
        self.assertFalse(volume.assess(0, nested + 'test result: FAILED. 6 passed; 1 failed;\n')[0])

    def test_success_requires_exact_native_cases_and_never_qualifies_release(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(volume.shutil, 'which', return_value='/cargo'):
            live = Path(directory) / 'gateway_state.json'
            live.write_text('do not read or change')
            def run(command, **options):
                self.assertIn('--locked', command)
                probe = Path(options['env']['TMPDIR'])
                self.assertEqual(probe.parent, Path(directory))
                self.assertNotEqual(probe, Path(directory))
                self.assertEqual(probe.stat().st_mode & 0o777, 0o700)
                return subprocess.CompletedProcess(command, 0, success())
            with patch.object(volume, 'run_native', side_effect=run):
                report = volume.qualify(Path(directory), 10)
            self.assertTrue(report['component_tests_passed'])
            self.assertFalse(report['production_qualified'])
            self.assertFalse(report['distributed_fencing_verified'])
            self.assertFalse(report['power_loss_verified'])
            self.assertEqual(live.read_text(), 'do not read or change')
            self.assertEqual((Path(report['evidence_directory']) / 'native-suite.log').stat().st_mode & 0o777, 0o600)

    def test_empty_skipped_missing_or_failed_suite_never_passes(self):
        for output in ('', 'test result: ok. 0 passed; 0 failed; 0 ignored;',
                       success().replace('0 ignored', '1 ignored'),
                       success().replace(volume.REQUIRED[0], 'some_other_test'),
                       success().replace('7 passed', '1 passed')):
            self.assertFalse(volume.assess(0, output)[0])
        self.assertFalse(volume.assess(1, success())[0])

    def test_invalid_directory_runs_nothing(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(volume, 'run_native') as run:
            report = volume.qualify(Path(directory) / 'missing', 10)
            self.assertFalse(report['component_tests_passed'])
            run.assert_not_called()

    def test_native_failure_and_timeout_are_failures(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(volume.shutil, 'which', return_value='/cargo'):
            for effect in (OSError('unavailable'), subprocess.TimeoutExpired('cargo', 10)):
                with patch.object(volume, 'run_native', side_effect=effect):
                    report = volume.qualify(Path(directory), 10)
                    self.assertFalse(report['component_tests_passed'])
                    self.assertIn('error', report)
