#!/usr/bin/env python3
"""Regression tests for deterministic CycloneDX normalization."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "normalize_release_sbom.py"
VERSION = "0.1.4"
TARGET = "x86_64-unknown-linux-gnu"


def valid_document(workspace_root: Path) -> dict[str, object]:
    workspace_uri = workspace_root.resolve().as_uri()
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "metadata": {
            "component": {
                "name": "gateway",
                "version": VERSION,
                "bom-ref": f"path+{workspace_uri}/cmd/gateway#0.1.4",
            },
            "properties": [
                {"name": "cdx:rustc:sbom:target:triple", "value": TARGET}
            ],
        },
        "components": [
            {
                "name": "conxian_core",
                "version": VERSION,
                "type": "library",
                "bom-ref": f"path+{workspace_uri}/pkg/conxian-core#conxian_core@{VERSION}",
            }
        ],
    }


class NormalizeReleaseSbomTests(unittest.TestCase):
    def run_normalizer(self, directory: Path, document: object) -> subprocess.CompletedProcess[str]:
        input_path = directory / "input.json"
        output_path = directory / "nested" / "output.json"
        workspace_root = directory / "workspace"
        workspace_root.mkdir()
        input_path.write_text(json.dumps(document), encoding="utf-8")
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--input",
                str(input_path),
                "--output",
                str(output_path),
                "--workspace-root",
                str(workspace_root),
                "--version",
                VERSION,
                "--target",
                TARGET,
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_normalizes_workspace_uri_and_timestamp(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            directory = Path(raw_directory)
            result = self.run_normalizer(directory, valid_document(directory / "workspace"))
            self.assertEqual(result.returncode, 0, result.stderr)
            output = json.loads((directory / "nested" / "output.json").read_text(encoding="utf-8"))
            self.assertEqual(output["metadata"]["timestamp"], "1970-01-01T00:00:00.000000000Z")
            serialized = json.dumps(output)
            self.assertIn("file:///conxian-gateway", serialized)
            self.assertNotIn(str((directory / "workspace").resolve()), serialized)

    def test_rejects_arbitrary_root_shape_without_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            result = self.run_normalizer(Path(raw_directory), "script-string-shaped-document")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("root must be an object", result.stderr)
            self.assertNotIn("Traceback", result.stderr)

    def test_rejects_malformed_property_without_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            directory = Path(raw_directory)
            document = valid_document(directory / "workspace")
            document["metadata"]["properties"] = [None]
            result = self.run_normalizer(directory, document)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("property 0 must be an object", result.stderr)
            self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
