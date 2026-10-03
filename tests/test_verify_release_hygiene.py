#!/usr/bin/env python3
"""Regression tests for release hygiene verification script."""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest


SCRIPT = (
    Path(__file__).resolve().parents[1] / "scripts" / "verify_release_hygiene.py"
)


def load_script():
    spec = importlib.util.spec_from_file_location("verify_release_hygiene", SCRIPT)
    if spec is None or spec.loader is None:
        raise AssertionError("could not load release hygiene verifier script")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VERIFY_RELEASE = load_script()


class VerifyReleaseHygieneTests(unittest.TestCase):
    def test_current_repo_passes_release_hygiene(self) -> None:
        result = VERIFY_RELEASE.main()
        self.assertEqual(result, 0)

    def test_passes_when_version_matches_changelog(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            cargo_toml = repo_dir / "Cargo.toml"
            cargo_toml.write_text('[workspace.package]\nversion = "1.2.3"\n', encoding="utf-8")
            changelog = repo_dir / "CHANGELOG.md"
            changelog.write_text("# Changelog\n\n## [v1.2.3] - 2026-01-01\n- Initial release\n", encoding="utf-8")

            old_cwd = os.getcwd()
            try:
                os.chdir(repo_dir)
                result = VERIFY_RELEASE.main()
                self.assertEqual(result, 0)
            finally:
                os.chdir(old_cwd)

    def test_fails_when_cargo_toml_missing(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            changelog = repo_dir / "CHANGELOG.md"
            changelog.write_text("# Changelog\n\n## [v1.2.3]\n", encoding="utf-8")

            old_cwd = os.getcwd()
            try:
                os.chdir(repo_dir)
                result = VERIFY_RELEASE.main()
                self.assertEqual(result, 1)
            finally:
                os.chdir(old_cwd)

    def test_fails_when_changelog_missing(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            cargo_toml = repo_dir / "Cargo.toml"
            cargo_toml.write_text('[workspace.package]\nversion = "1.2.3"\n', encoding="utf-8")

            old_cwd = os.getcwd()
            try:
                os.chdir(repo_dir)
                result = VERIFY_RELEASE.main()
                self.assertEqual(result, 1)
            finally:
                os.chdir(old_cwd)

    def test_fails_when_version_mismatched(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            repo_dir = Path(raw_dir)
            cargo_toml = repo_dir / "Cargo.toml"
            cargo_toml.write_text('[workspace.package]\nversion = "1.2.4"\n', encoding="utf-8")
            changelog = repo_dir / "CHANGELOG.md"
            changelog.write_text("# Changelog\n\n## [v1.2.3]\n", encoding="utf-8")

            old_cwd = os.getcwd()
            try:
                os.chdir(repo_dir)
                result = VERIFY_RELEASE.main()
                self.assertEqual(result, 1)
            finally:
                os.chdir(old_cwd)


if __name__ == "__main__":
    unittest.main()
