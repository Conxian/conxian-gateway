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
CARGO_METADATA_NAME = "cargo-metadata.json"
WORKSPACE_ROOT = "/fixture/conxian-gateway"
USTAR_BLOCK_SIZE = 512
USTAR_ZERO_BLOCK = b"\x00" * USTAR_BLOCK_SIZE
WORKSPACE_PACKAGES = [
    ("gateway", "cmd/gateway", "path+file:///fixture/conxian-gateway/cmd/gateway#0.1.4"),
    ("conxian_api", "internal/api", "path+file:///fixture/conxian-gateway/internal/api#conxian_api@0.1.4"),
    (
        "conxian_compliance",
        "internal/compliance",
        "path+file:///fixture/conxian-gateway/internal/compliance#conxian_compliance@0.1.4",
    ),
    (
        "conxian_engine",
        "internal/engine",
        "path+file:///fixture/conxian-gateway/internal/engine#conxian_engine@0.1.4",
    ),
    (
        "conxian_core",
        "pkg/conxian-core",
        "path+file:///fixture/conxian-gateway/pkg/conxian-core#conxian_core@0.1.4",
    ),
]


def stable_workspace_ref(reference: str) -> str:
    return reference.replace("path+file:///fixture/conxian-gateway", "path+file:///conxian-gateway")


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
    gateway_ref = stable_workspace_ref(WORKSPACE_PACKAGES[0][2])
    workspace_components = [
        {
            "type": "library",
            "bom-ref": stable_workspace_ref(reference),
            "name": name,
            "version": VERSION,
        }
        for name, _path, reference in WORKSPACE_PACKAGES[1:]
    ]
    registry_ref = "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0"
    all_dependency_refs = [gateway_ref, *(item["bom-ref"] for item in workspace_components), registry_ref]
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "timestamp": "1970-01-01T00:00:00.000000000Z",
            "component": {
                "type": "application",
                "bom-ref": gateway_ref,
                "name": "gateway",
                "version": VERSION,
                "components": [
                    {
                        "type": "application",
                        "bom-ref": "path+file:///conxian-gateway/cmd/gateway#0.1.4 bin-target-0",
                        "name": "gateway-bin",
                        "version": VERSION,
                    }
                ],
            },
            "properties": [
                {
                    "name": "cdx:rustc:sbom:target:triple",
                    "value": TARGET,
                }
            ],
        },
        "components": [
            *workspace_components,
            {"type": "library", "bom-ref": registry_ref, "name": "serde", "version": "1.0.0"},
        ],
        "dependencies": [{"ref": reference, "dependsOn": []} for reference in all_dependency_refs],
    }


def valid_cargo_metadata() -> dict[str, object]:
    return {
        "workspace_root": WORKSPACE_ROOT,
        "workspace_members": [reference for _name, _path, reference in WORKSPACE_PACKAGES],
        "packages": [
            {"id": reference, "name": name, "version": VERSION}
            for name, _path, reference in WORKSPACE_PACKAGES
        ],
    }


def write_archive(
    path: Path,
    *,
    commit: str = COMMIT,
    members: list[tuple[str, str, bytes | str | None, int]] | None = None,
    archive_format: int = tarfile.USTAR_FORMAT,
    add_pax_header: bool = False,
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
        with tarfile.open(fileobj=gzip_stream, mode="w", format=archive_format) as archive:
            for name, kind, value, mode in members:
                info = tarfile.TarInfo(name)
                info.uid = 0
                info.gid = 0
                info.uname = ""
                info.gname = ""
                info.mtime = 0
                info.mode = mode
                if add_pax_header:
                    info.pax_headers = {"comment": "adversarial fixture"}
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
    payload = gzip.decompress(compressed.getvalue())
    while payload.endswith(USTAR_ZERO_BLOCK):
        payload = payload[:-USTAR_BLOCK_SIZE]
    write_gzip_payload(path, payload + USTAR_ZERO_BLOCK * 2)


def write_gzip_payload(path: Path, payload: bytes) -> None:
    compressed = io.BytesIO()
    with gzip.GzipFile(fileobj=compressed, mode="wb", mtime=0) as gzip_stream:
        gzip_stream.write(payload)
    path.write_bytes(compressed.getvalue())


def mutate_ustar_header(path: Path, offset: int, value: int) -> None:
    raw = bytearray(gzip.decompress(path.read_bytes()))
    header = raw[:512]
    header[offset] = value
    header[148:156] = b"        "
    checksum = sum(header)
    header[148:156] = f"{checksum:06o}\x00 ".encode("ascii")
    raw[:512] = header
    write_gzip_payload(path, raw)


def write_sidecars(
    directory: Path,
    *,
    sbom: dict[str, object] | None = None,
    cargo_metadata: dict[str, object] | None = None,
) -> None:
    archive = directory / ARCHIVE_NAME
    sbom_path = directory / SBOM_NAME
    sbom_path.write_text(
        json.dumps(valid_sbom() if sbom is None else sbom, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    checksum_lines = []
    for path in sorted((archive, sbom_path), key=lambda item: item.name):
        checksum_lines.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}")
    (directory / CHECKSUMS_NAME).write_text("\n".join(checksum_lines) + "\n", encoding="utf-8")
    metadata_path = directory.parent / f"{directory.name}-{CARGO_METADATA_NAME}"
    metadata_path.write_text(
        json.dumps(valid_cargo_metadata() if cargo_metadata is None else cargo_metadata, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def write_workflow_archive(path: Path, directory: Path, commit: str = COMMIT) -> None:
    stage_directory = directory.parent / f"{directory.name}-stage"
    stage_directory.mkdir()
    stage_root = stage_directory / ROOT
    stage_root.mkdir()
    gateway = b"\x7fELF" + bytes([2]) + b"\0" * 13 + (62).to_bytes(2, "little") + b"gateway"
    (stage_root / "gateway").write_bytes(gateway)
    (stage_root / "gateway").chmod(0o755)
    (stage_root / "RELEASE-METADATA.txt").write_text(metadata_text(commit), encoding="utf-8")
    tar_result = subprocess.run(
        [
            "tar",
            "--format=ustar",
            "--sort=name",
            "--mtime=UTC 1970-01-01",
            "--owner=0",
            "--group=0",
            "--numeric-owner",
            "--blocking-factor=1",
            "-C",
            str(stage_directory),
            "-cf",
            "-",
            ROOT,
        ],
        check=True,
        capture_output=True,
    )
    gzip_result = subprocess.run(
        ["gzip", "-n"],
        input=tar_result.stdout,
        check=True,
        capture_output=True,
    )
    path.write_bytes(gzip_result.stdout)


def write_fixture(
    directory: Path,
    *,
    archive_commit: str = COMMIT,
    sbom: dict[str, object] | None = None,
    archive_members: list[tuple[str, str, bytes | str | None, int]] | None = None,
    archive_format: int = tarfile.USTAR_FORMAT,
    add_pax_header: bool = False,
    cargo_metadata: dict[str, object] | None = None,
) -> None:
    archive = directory / ARCHIVE_NAME
    write_archive(
        archive,
        commit=archive_commit,
        members=archive_members,
        archive_format=archive_format,
        add_pax_header=add_pax_header,
    )
    write_sidecars(
        directory,
        sbom=sbom,
        cargo_metadata=cargo_metadata,
    )


class VerifyReleaseArtifactsTests(unittest.TestCase):
    def run_verifier(self, directory: Path, expected_commit: str = COMMIT) -> subprocess.CompletedProcess[str]:
        metadata_path = directory.parent / f"{directory.name}-{CARGO_METADATA_NAME}"
        try:
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
                    "--cargo-metadata",
                    str(metadata_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
        finally:
            metadata_path.unlink(missing_ok=True)

    def fixture(self) -> tempfile.TemporaryDirectory[str]:
        return tempfile.TemporaryDirectory()

    def test_valid_fixture(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            result = self.run_verifier(directory)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_exact_workflow_archive_command_passes(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_workflow_archive(directory / ARCHIVE_NAME, directory)
            write_sidecars(directory)
            payload = gzip.decompress((directory / ARCHIVE_NAME).read_bytes())
            self.assertEqual(payload[-2 * USTAR_BLOCK_SIZE :], USTAR_ZERO_BLOCK * 2)
            self.assertNotEqual(payload[-3 * USTAR_BLOCK_SIZE : -2 * USTAR_BLOCK_SIZE], USTAR_ZERO_BLOCK)
            workflow = (Path(__file__).resolve().parents[1] / ".github" / "workflows" / "release.yml").read_text(
                encoding="utf-8"
            )
            self.assertIn("--blocking-factor=1", workflow)
            result = self.run_verifier(directory)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_raw_zero_bytes_after_gzip(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            archive = directory / ARCHIVE_NAME
            archive.write_bytes(archive.read_bytes() + b"\x00")
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("exactly one gzip member", result.stderr)

    def test_rejects_additional_empty_gzip_member(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            archive = directory / ARCHIVE_NAME
            archive.write_bytes(archive.read_bytes() + gzip.compress(b"", mtime=0))
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("exactly one gzip member", result.stderr)

    def test_rejects_concatenated_nonempty_gzip_member(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            archive = directory / ARCHIVE_NAME
            archive.write_bytes(archive.read_bytes() + gzip.compress(b"not-a-tar", mtime=0))
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("exactly one gzip member", result.stderr)

    def test_rejects_additional_tar_zero_blocks(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            archive = directory / ARCHIVE_NAME
            payload = gzip.decompress(archive.read_bytes())
            write_gzip_payload(
                archive,
                payload[:-2 * USTAR_BLOCK_SIZE]
                + USTAR_ZERO_BLOCK
                + payload[-2 * USTAR_BLOCK_SIZE :],
            )
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("data after the USTAR trailer", result.stderr)

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

    def test_rejects_pax_headers(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory, archive_format=tarfile.PAX_FORMAT, add_pax_header=True)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unsafe type", result.stderr)

    def test_rejects_gnu_sparse_style_indicator(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            mutate_ustar_header(directory / ARCHIVE_NAME, 345, 1)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("sparse indicator", result.stderr)

    def test_rejects_non_ustar_header(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            mutate_ustar_header(directory / ARCHIVE_NAME, 257, ord("g"))
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("non-USTAR", result.stderr)

    def test_rejects_nul_regular_type(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            write_fixture(directory)
            mutate_ustar_header(directory / ARCHIVE_NAME, 156, 0)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unsafe type", result.stderr)

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
            changed["components"][0]["description"] = "changed"
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

    def test_rejects_fabricated_workspace_component(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["components"].append(
                {
                    "type": "library",
                    "bom-ref": "path+file:///conxian-gateway/fabricated#fake@9.9.9",
                    "name": "fabricated",
                    "version": "9.9.9",
                }
            )
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("workspace component refs", result.stderr)

    def test_rejects_missing_workspace_component(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["components"].pop(0)
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("workspace component refs", result.stderr)

    def test_rejects_mismatched_workspace_component_version(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["components"][0]["version"] = "9.9.9"
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("workspace package names and versions", result.stderr)

    def test_rejects_mismatched_locked_metadata(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            metadata = valid_cargo_metadata()
            metadata["packages"][1]["version"] = "9.9.9"
            write_fixture(directory, cargo_metadata=metadata)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("workspace package names and versions", result.stderr)

    def test_rejects_dangling_dependency_reference(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["dependencies"][0]["dependsOn"] = ["missing-bom-ref"]
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("does not resolve", result.stderr)

    def test_rejects_duplicate_component_bom_ref(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["components"].append(dict(sbom["components"][0]))
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("duplicate component bom-ref", result.stderr)

    def test_rejects_orphan_component_not_in_dependency_graph(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["components"].append(
                {
                    "type": "library",
                    "bom-ref": "registry+https://github.com/rust-lang/crates.io-index#orphan@1.0.0",
                    "name": "orphan",
                    "version": "1.0.0",
                }
            )
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("dependency graph", result.stderr)

    def test_rejects_malformed_component_bom_ref(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["components"][0]["bom-ref"] = f" {sbom['components'][0]['bom-ref']}"
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("bom-ref is malformed", result.stderr)

    def test_rejects_malformed_component_field(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["components"][0]["name"] = {"arbitrary": "shape"}
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("name must be a string", result.stderr)

    def test_rejects_arbitrary_component_shape(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["components"][0] = "script-string-shaped-component"
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("component is not an object", result.stderr)

    def test_rejects_non_string_property_value(self) -> None:
        with self.fixture() as raw_directory:
            directory = Path(raw_directory)
            sbom = valid_sbom()
            sbom["metadata"]["properties"][0]["value"] = [TARGET]
            write_fixture(directory, sbom=sbom)
            result = self.run_verifier(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("property 0 value must be a string", result.stderr)


if __name__ == "__main__":
    unittest.main()
