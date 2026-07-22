#!/usr/bin/env python3
"""Deterministic regression tests for the release artifact verifier."""

from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "verify_release_artifacts.py"
VERSION = "0.1.4"
TARGET = "x86_64-unknown-linux-gnu"
COMMIT = "0123456789abcdef0123456789abcdef01234567"
OTHER_COMMIT = "fedcba9876543210fedcba9876543210fedcba98"
ROOT = f"conxian-gateway-{VERSION}-{TARGET}"
ARCHIVE_NAME = f"{ROOT}.tar.gz"
SBOM_NAME = f"conxian-gateway-{VERSION}.cdx.json"
CHECKSUMS_NAME = f"conxian-gateway-{VERSION}.sha256"


def metadata_text(commit: str) -> str:
    return (
        "artifact=conxian-gateway\n"
        f"version={VERSION}\n"
        f"target={TARGET}\n"
        f"commit={commit}\n"
        "toolchain=1.96.0\n"
        "source-date-epoch=0\n"
    )


def valid_sbom() -> dict[str, object]:
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "metadata": {
            "timestamp": "1970-01-01T00:00:00.000000000Z",
            "component": {"name": "gateway", "version": VERSION},
            "properties": [
                {
                    "name": "cdx:rustc:sbom:target:triple",
                    "value": TARGET,
                }
            ],
        },
        "components": [{"name": "serde", "version": "1.0.0"}],
    }


def write_archive(
    path: Path,
    *,
    commit: str = COMMIT,
    members: list[tuple[str, str, bytes | str | None, int]] | None = None,
) -> None:
    gateway = b"\x7fELF" + bytes([2]) + b"\0" * 13 + (62).to_bytes(2, "little") + b"gateway"
    default_members: list[tuple[str, str, bytes | str | None, int]] = [
        (ROOT, "directory", None, 0o755),
        (f"{ROOT}/RELEASE-METADATA.txt", "file", metadata_text(commit).encode(), 0o644),
        (f"{ROOT}/gateway", "file", gateway, 0o755),
    ]
    members = default_members if members is None else members

    compressed = io.BytesIO()
    with gzip.GzipFile(fileobj=compressed, mode="wb", mtime=0) as gzip_stream:
        with tarfile.open(fileobj=gzip_stream, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for name, kind, value, mode in members:
                info = tarfile.TarInfo(name)
                info.uid = 0
                info.gid = 0
                info.uname = ""
                info.gname = ""
                info.mtime = 0
                info.mode = mode
                if kind == "directory":
                    info.type = tarfile.DIRTYPE
                    archive.addfile(info)
                elif kind == "file":
                    data = value if isinstance(value, bytes) else str(value or "").encode()
                    info.type = tarfile.REGTYPE
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))
                elif kind == "symlink":
                    info.type = tarfile.SYMTYPE
                    info.linkname = str(value or "gateway")
                    archive.addfile(info)
                elif kind == "fifo":
                    info.type = tarfile.FIFOTYPE
                    archive.addfile(info)
                else:
                    raise AssertionError(f"unknown fixture member kind: {kind}")
    path.write_bytes(compressed.getvalue())


def write_fixture(
    directory: Path,
    *,
    archive_commit: str = COMMIT,
    sbom: dict[str, object] | None = None,
    archive_members: list[tuple[str, str, bytes | str | None, int]] | None = None,
) -> None:
    archive = directory / ARCHIVE_NAME
    sbom_path = directory / SBOM_NAME
    write_archive(archive, commit=archive_commit, members=archive_members)
    sbom_path.write_text(
        json.dumps(valid_sbom() if sbom is None else sbom, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    checksum_lines = []
    for path in sorted((archive, sbom_path), key=lambda item: item.name):
        checksum_lines.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}")
    (directory / CHECKSUMS_NAME).write_text("\n".join(checksum_lines) + "\n", encoding="utf-8")


class VerifyReleaseArtifactsTests(unittest.TestCase):
    def run_verifier(self, directory: Path, expected_commit: str = COMMIT) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--directory",
                str(directory),
                "--version",
                VERSION,
                "--target",
                TARGET,
                "--expected-commit",
                expected_commit,
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def fixture(self) -> tempfile.TemporaryDirectory[str]:
        return tempfile.TemporaryDirectory()

    def test_valid_fixture(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            result = self.run_verifier(directory)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_unexpected_directory(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            (directory / "unexpected").mkdir()
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("non-file entry", result.stderr)

    def test_rejects_symlinked_expected_file(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            original = directory / SBOM_NAME
            outside = directory.parent / "release-verifier-sbom.json"
            outside.write_bytes(original.read_bytes())
            original.unlink()
            os.symlink(outside, original)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("symlink", result.stderr)

    def test_rejects_duplicate_tar_member(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(
                directory,
                archive_members=[
                    (ROOT, "directory", None, 0o755),
                    (f"{ROOT}/RELEASE-METADATA.txt", "file", metadata_text(COMMIT).encode(), 0o644),
                    (f"{ROOT}/gateway", "file", b"\x7fELF\x02" + b"\0" * 15, 0o755),
                    (f"{ROOT}/gateway", "file", b"\x7fELF\x02" + b"\0" * 15, 0o755),
                ],
            )
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("duplicate", result.stderr)

    def test_rejects_unsafe_archive_path(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(
                directory,
                archive_members=[
                    (ROOT, "directory", None, 0o755),
                    (f"{ROOT}/RELEASE-METADATA.txt", "file", metadata_text(COMMIT).encode(), 0o644),
                    (f"{ROOT}/gateway", "file", b"\x7fELF\x02" + b"\0" * 15, 0o755),
                    (f"{ROOT}/../escape", "file", b"escape", 0o644),
                ],
            )
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unsafe member path", result.stderr)

    def test_rejects_unsafe_archive_member_type(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(
                directory,
                archive_members=[
                    (ROOT, "directory", None, 0o755),
                    (f"{ROOT}/RELEASE-METADATA.txt", "file", metadata_text(COMMIT).encode(), 0o644),
                    (f"{ROOT}/gateway", "symlink", "../../outside", 0o755),
                ],
            )
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unsafe type", result.stderr)

    def test_rejects_short_elf_header(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(
                directory,
                archive_members=[
                    (ROOT, "directory", None, 0o755),
                    (f"{ROOT}/RELEASE-METADATA.txt", "file", metadata_text(COMMIT).encode(), 0o644),
                    (f"{ROOT}/gateway", "file", b"\x7fELF\x02", 0o755),
                ],
            )
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not an ELF executable", result.stderr)

    def test_rejects_malformed_metadata_commit(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            malformed = "not-a-commit"
            write_fixture(directory, archive_commit=malformed)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("release metadata commit", result.stderr)

    def test_rejects_mismatched_expected_commit(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            result = self.run_verifier(directory, expected_commit=OTHER_COMMIT)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("exactly identify", result.stderr)

    def test_rejects_extra_artifact(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            (directory / "extra.txt").write_text("unexpected", encoding="utf-8")
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unexpected entry", result.stderr)

    def test_rejects_missing_artifact(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            (directory / SBOM_NAME).unlink()
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("artifact files", result.stderr)

    def test_rejects_checksum_mismatch(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            changed = valid_sbom()
            changed["components"] = [{"name": "changed", "version": "1.0.0"}]
            (directory / SBOM_NAME).write_text(json.dumps(changed) + "\n", encoding="utf-8")
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("checksum mismatch", result.stderr)

    def test_rejects_malformed_sbom(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            (directory / SBOM_NAME).write_text("{not-json", encoding="utf-8")
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not valid UTF-8 JSON", result.stderr)


if __name__ == "__main__":
    unittest.main()
