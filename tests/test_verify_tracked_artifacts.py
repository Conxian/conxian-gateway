#!/usr/bin/env python3
"""Regression tests for tracked artifacts verification script."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest


SCRIPT = (
    Path(__file__).resolve().parents[1] / "scripts" / "verify_tracked_artifacts.py"
)


def load_script():
    spec = importlib.util.spec_from_file_location("verify_tracked_artifacts", SCRIPT)
    if spec is None or spec.loader is None:
        raise AssertionError("could not load tracked artifacts verifier script")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VERIFY_ARTIFACTS = load_script()


class VerifyTrackedArtifactsTests(unittest.TestCase):
    def test_current_repo_has_no_prohibited_tracked_artifacts(self) -> None:
        result = VERIFY_ARTIFACTS.main()
        self.assertEqual(result, 0)

    def test_detects_tracked_prohibited_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            # Initialize a temporary git repository
            subprocess.run(["git", "init"], cwd=repo_dir, check=True, capture_output=True)
            subprocess.run(["git", "config", "user.name", "Test"], cwd=repo_dir, check=True, capture_output=True)
            subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=repo_dir, check=True, capture_output=True)

            # Create a tracked file matching prohibited patterns (e.g., node_modules/index.js)
            node_modules = repo_dir / "node_modules"
            node_modules.mkdir()
            fake_file = node_modules / "dummy.js"
            fake_file.write_text("// dummy", encoding="utf-8")

            subprocess.run(["git", "add", "node_modules/dummy.js"], cwd=repo_dir, check=True, capture_output=True)

            result = subprocess.run(
                [sys.executable, str(SCRIPT)],
                cwd=repo_dir,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("ERROR: Prohibited artifact tracked:", result.stdout)
            self.assertIn("node_modules/dummy.js", result.stdout)


if __name__ == "__main__":
    unittest.main()
