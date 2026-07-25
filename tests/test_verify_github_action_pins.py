#!/usr/bin/env python3
"""Regression tests for immutable GitHub workflow action references."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest


SCRIPT = (
    Path(__file__).resolve().parents[1] / "scripts" / "verify_github_action_pins.py"
)
COMMIT = "0123456789abcdef0123456789abcdef01234567"
DIGEST = "a" * 64


def load_script():
    spec = importlib.util.spec_from_file_location("verify_github_action_pins", SCRIPT)
    if spec is None or spec.loader is None:
        raise AssertionError("could not load GitHub action pin verifier")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VERIFY_PINS = load_script()


class VerifyGithubActionPinsTests(unittest.TestCase):
    def test_accepts_commit_pins_local_actions_and_docker_digests(self) -> None:
        references = [
            f"actions/checkout@{COMMIT}",
            f"Conxian/.github/.github/workflows/reusable.yml@{COMMIT}",
            "./.github/actions/local-action",
            f"docker://alpine@sha256:{DIGEST}",
        ]
        for reference in references:
            with self.subTest(reference=reference):
                VERIFY_PINS.validate_uses_reference(reference)

    def test_rejects_mutable_or_incomplete_remote_references(self) -> None:
        references = [
            "actions/checkout@v4",
            "actions/checkout@main",
            "actions/checkout@0123456789abcdef",
            "actions/checkout",
            "${{ github.repository }}/action@main",
            "docker://alpine:3.20",
            "docker://alpine@sha256:abcd",
        ]
        for reference in references:
            with (
                self.subTest(reference=reference),
                self.assertRaises(VERIFY_PINS.PinError),
            ):
                VERIFY_PINS.validate_uses_reference(reference)

    def test_scans_yml_and_yaml_with_quotes_and_inline_comments(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            workflows = Path(raw_directory)
            (workflows / "nested").mkdir()
            (workflows / "accepted.yml").write_text(
                f"jobs:\n  one:\n    steps:\n      - uses: actions/checkout@{COMMIT} # v7\n"
                "      - uses: './.github/actions/local#fragment' # local\n",
                encoding="utf-8",
            )
            (workflows / "nested" / "rejected.yaml").write_text(
                'jobs:\n  one:\n    steps:\n      - uses: "actions/setup-node@v4" # mutable\n',
                encoding="utf-8",
            )
            errors = VERIFY_PINS.scan_workflows(workflows)
        self.assertEqual(len(errors), 1)
        self.assertIn("rejected.yaml:4", errors[0])
        self.assertIn("40-character commit SHA", errors[0])

    def test_cli_fails_closed_for_mutable_reference(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            workflows = Path(raw_directory)
            (workflows / "workflow.yml").write_text(
                "jobs:\n  one:\n    uses: owner/reusable/.github/workflows/test.yml@feature\n",
                encoding="utf-8",
            )
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--workflows-dir", str(workflows)],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("workflow pin verification failed", result.stderr)


if __name__ == "__main__":
    unittest.main()
