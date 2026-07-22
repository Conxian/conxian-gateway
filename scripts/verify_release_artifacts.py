#!/usr/bin/env python3
"""Verify the deterministic Gateway release artifact set."""

from __future__ import annotations

import argparse
import hashlib
import json
import posixpath
import re
import struct
import tarfile
from collections import Counter
from pathlib import Path
from typing import Any, NoReturn


CHECKSUM_LINE = re.compile(r"^(?P<digest>[0-9a-f]{64})  (?P<name>[^/\n]+)$")
COMMIT = re.compile(r"^[0-9a-fA-F]{40}$")
METADATA_KEY = re.compile(r"^[a-z][a-z0-9-]*$")
EXPECTED_TARGET = "x86_64-unknown-linux-gnu"
EXPECTED_TOOLCHAIN = "1.96.0"
EXPECTED_SBOM_TIMESTAMP = "1970-01-01T00:00:00.000000000Z"


def fail(message: str) -> NoReturn:
    raise SystemExit(f"error: {message}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def validate_commit(value: str, field: str) -> str:
    if COMMIT.fullmatch(value) is None:
        fail(f"{field} is not a full 40-hex commit SHA")
    return value.lower()


def reject_unsafe_member_name(name: str) -> None:
    if (
        not name
        or "\x00" in name
        or "\\" in name
        or name.startswith(("/", "\\"))
        or posixpath.normpath(name) != name
        or any(part in {"", ".", ".."} for part in name.split("/"))
    ):
        fail(f"release archive contains unsafe member path {name!r}")


def verify_member_metadata(member: tarfile.TarInfo, expected_mode: int) -> None:
    if member.uid != 0 or member.gid != 0 or member.mtime != 0:
        fail(f"release archive member metadata is not normalized: {member.name!r}")
    if member.uname or member.gname:
        fail(f"release archive member names are not normalized: {member.name!r}")
    if member.mode != expected_mode:
        fail(f"release archive member mode is not normalized: {member.name!r}")


def read_metadata(member: tarfile.TarInfo, archive: tarfile.TarFile) -> dict[str, str]:
    metadata_stream = archive.extractfile(member)
    if metadata_stream is None:
        fail("could not read release metadata")
    try:
        lines = metadata_stream.read().decode("utf-8").splitlines()
    except UnicodeDecodeError:
        fail("release metadata is not valid UTF-8")

    metadata: dict[str, str] = {}
    for line in lines:
        if "=" not in line:
            fail(f"release metadata contains a malformed line: {line!r}")
        key, value = line.split("=", 1)
        if METADATA_KEY.fullmatch(key) is None or not value:
            fail(f"release metadata contains a malformed field: {line!r}")
        if key in metadata:
            fail(f"release metadata contains duplicate field {key!r}")
        metadata[key] = value
    return metadata


def verify_archive(path: Path, version: str, target: str, expected_commit: str) -> None:
    if target != EXPECTED_TARGET:
        fail(f"unsupported release target {target!r}")
    with path.open("rb") as stream:
        raw_header = stream.read(10)
    if len(raw_header) < 10 or raw_header[:2] != b"\x1f\x8b" or raw_header[2] != 8:
        fail("release archive is not gzip-compressed")
    if int.from_bytes(raw_header[4:8], "little") != 0:
        fail("release archive gzip timestamp is not reproducible")

    root = f"conxian-gateway-{version}-{target}"
    expected_members = {
        root,
        f"{root}/RELEASE-METADATA.txt",
        f"{root}/gateway",
    }
    with tarfile.open(path, mode="r:gz") as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        for member in members:
            reject_unsafe_member_name(member.name)
            if member.name not in expected_members:
                fail(f"release archive contains unexpected member {member.name!r}")
            if member.isdir():
                if member.name != root:
                    fail(f"release archive contains an unexpected directory member {member.name!r}")
                verify_member_metadata(member, 0o755)
            elif member.isreg():
                expected_mode = 0o644 if member.name.endswith("RELEASE-METADATA.txt") else 0o755
                if member.linkname:
                    fail(f"release archive regular member has a link target: {member.name!r}")
                verify_member_metadata(member, expected_mode)
            else:
                fail(f"release archive member has an unsafe type: {member.name!r}")

        counts = Counter(names)
        duplicate_names = sorted(name for name, count in counts.items() if count != 1)
        if duplicate_names:
            fail(f"release archive contains duplicate members: {duplicate_names!r}")
        if len(members) != len(expected_members) or set(names) != expected_members:
            fail(f"release archive members are {sorted(set(names))!r}, expected {sorted(expected_members)!r}")

        root_member = next(member for member in members if member.name == root)
        if not root_member.isdir():
            fail("release archive root entry is not a directory")

        metadata_member = next(member for member in members if member.name == f"{root}/RELEASE-METADATA.txt")
        if not metadata_member.isreg():
            fail("release archive metadata entry is not a regular file")
        expected_metadata = {
            "artifact": "conxian-gateway",
            "version": version,
            "target": target,
            "commit": expected_commit,
            "toolchain": EXPECTED_TOOLCHAIN,
            "source-date-epoch": "0",
        }
        metadata = read_metadata(metadata_member, archive)
        if metadata.get("commit") is not None:
            metadata["commit"] = validate_commit(metadata["commit"], "release metadata commit")
        if metadata != expected_metadata:
            fail("release metadata does not exactly identify the expected artifact")

        binary_member = next(member for member in members if member.name == f"{root}/gateway")
        if (
            not binary_member.isreg()
            or binary_member.size == 0
            or binary_member.mode & 0o111 == 0
        ):
            fail("release archive does not contain a non-empty Gateway binary")
        binary = archive.extractfile(binary_member)
        if binary is None:
            fail("could not read Gateway binary from release archive")
        header = binary.read(20)
        if len(header) < 20 or header[:4] != b"\x7fELF":
            fail("Gateway binary is not an ELF executable")
        if header[4] != 2 or struct.unpack_from("<H", header, 18)[0] != 62:
            fail("Gateway binary is not an x86_64 ELF executable")


def verify_sbom(path: Path, version: str, target: str) -> None:
    try:
        document: Any = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        fail("SBOM is not valid UTF-8 JSON")
    if not isinstance(document, dict):
        fail("SBOM root is not a JSON object")
    if document.get("bomFormat") != "CycloneDX":
        fail("SBOM is not CycloneDX")
    if document.get("specVersion") != "1.5":
        fail("SBOM specVersion is not 1.5")
    metadata = document.get("metadata") or {}
    if not isinstance(metadata, dict):
        fail("SBOM metadata is not an object")
    if metadata.get("timestamp") != EXPECTED_SBOM_TIMESTAMP:
        fail("SBOM timestamp is not normalized")
    component = metadata.get("component") or {}
    if not isinstance(component, dict):
        fail("SBOM root component is not an object")
    if component.get("name") != "gateway" or component.get("version") != version:
        fail("SBOM root component does not match the release")
    properties = metadata.get("properties")
    if not isinstance(properties, list):
        fail("SBOM properties are not an array")
    if not any(
        isinstance(item, dict)
        and item.get("name") == "cdx:rustc:sbom:target:triple"
        and item.get("value") == target
        for item in properties
    ):
        fail("SBOM target triple does not match the release")
    components = document.get("components")
    if not isinstance(components, list) or not components:
        fail("SBOM has no dependency components")


def verify_checksums(path: Path, expected: set[str], directory: Path) -> None:
    entries: dict[str, str] = {}
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines:
        fail("checksum manifest is empty")
    for line in lines:
        match = CHECKSUM_LINE.fullmatch(line)
        if match is None:
            fail(f"invalid checksum manifest line: {line!r}")
        name = match.group("name")
        if name in entries:
            fail(f"duplicate checksum entry for {name!r}")
        entries[name] = match.group("digest")
    if set(entries) != expected:
        fail(f"checksum entries are {sorted(entries)!r}, expected {sorted(expected)!r}")
    if list(entries) != sorted(entries):
        fail("checksum entries are not sorted by filename")
    for name, expected_digest in entries.items():
        actual_digest = sha256(directory / name)
        if actual_digest != expected_digest:
            fail(f"checksum mismatch for {name!r}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument(
        "--expected-commit",
        required=True,
        help="full 40-hex commit SHA recorded in RELEASE-METADATA.txt",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.directory.is_symlink() or not args.directory.is_dir():
        fail(f"artifact directory is not a regular directory: {args.directory}")
    directory = args.directory.resolve()
    expected_commit = validate_commit(args.expected_commit, "expected commit")

    archive_name = f"conxian-gateway-{args.version}-{args.target}.tar.gz"
    sbom_name = f"conxian-gateway-{args.version}.cdx.json"
    checksums_name = f"conxian-gateway-{args.version}.sha256"
    expected_files = {archive_name, sbom_name, checksums_name}
    entries = list(directory.iterdir())
    for entry in entries:
        if entry.is_symlink():
            fail(f"artifact directory contains a symlink: {entry.name!r}")
        if not entry.is_file():
            fail(f"artifact directory contains a non-file entry: {entry.name!r}")
        if entry.name not in expected_files:
            fail(f"artifact directory contains an unexpected entry: {entry.name!r}")
    actual_files = {path.name for path in entries}
    if actual_files != expected_files:
        fail(f"artifact files are {sorted(actual_files)!r}, expected {sorted(expected_files)!r}")

    verify_archive(directory / archive_name, args.version, args.target, expected_commit)
    verify_sbom(directory / sbom_name, args.version, args.target)
    verify_checksums(
        directory / checksums_name,
        {archive_name, sbom_name},
        directory,
    )
    print(f"verified release artifacts for {args.version} ({args.target})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
