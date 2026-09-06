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

    def _create_temp_git_repo(self, repo_dir: Path) -> None:
        subprocess.run(["git", "init"], cwd=repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Test"], cwd=repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=repo_dir, check=True, capture_output=True)

    def test_detects_tracked_prohibited_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            self._create_temp_git_repo(repo_dir)

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

    def test_detects_tracked_sensitive_env_and_key_files(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            self._create_temp_git_repo(repo_dir)

            # Create sensitive env and key files
            env_file = repo_dir / ".env.production"
            env_file.write_text("SECRET=12345", encoding="utf-8")
            key_file = repo_dir / "server.key"
            key_file.write_text("-----BEGIN PRIVATE KEY-----", encoding="utf-8")

            subprocess.run(["git", "add", ".env.production", "server.key"], cwd=repo_dir, check=True, capture_output=True)

            result = subprocess.run(
                [sys.executable, str(SCRIPT)],
                cwd=repo_dir,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("ERROR: Prohibited environment file tracked: .env.production", result.stdout)
            self.assertIn("ERROR: Prohibited artifact tracked: server.key", result.stdout)

    def test_allows_tracked_env_example_template(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            self._create_temp_git_repo(repo_dir)

            # Create an allowed template .env.example
            env_example = repo_dir / ".env.example"
            env_example.write_text("API_PORT=3000", encoding="utf-8")

            subprocess.run(["git", "add", ".env.example"], cwd=repo_dir, check=True, capture_output=True)

            result = subprocess.run(
                [sys.executable, str(SCRIPT)],
                cwd=repo_dir,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0)
            self.assertIn("No prohibited artifacts tracked.", result.stdout)

    def test_detects_tracked_runtime_state_and_test_results(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            self._create_temp_git_repo(repo_dir)

            # Create runtime state file and test result directory
            state_file = repo_dir / "gateway_state.json"
            state_file.write_text('{"state": "test"}', encoding="utf-8")
            test_results = repo_dir / "test-results"
            test_results.mkdir()
            results_file = test_results / "junit.xml"
            results_file.write_text("<xml/>", encoding="utf-8")

            subprocess.run(["git", "add", "gateway_state.json", "test-results/junit.xml"], cwd=repo_dir, check=True, capture_output=True)

            result = subprocess.run(
                [sys.executable, str(SCRIPT)],
                cwd=repo_dir,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("ERROR: Prohibited artifact tracked: gateway_state.json", result.stdout)
            self.assertIn("ERROR: Prohibited artifact tracked: test-results/junit.xml", result.stdout)


if __name__ == "__main__":
    unittest.main()
