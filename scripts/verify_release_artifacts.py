#!/usr/bin/env python3
"""Verify the deterministic Gateway release artifact set."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import tarfile
from pathlib import Path
from typing import NoReturn


CHECKSUM_LINE = re.compile(r"^(?P<digest>[0-9a-f]{64})  (?P<name>[^/\n]+)$")
EXPECTED_TARGET = "x86_64-unknown-linux-gnu"
EXPECTED_SBOM_TIMESTAMP = "1970-01-01T00:00:00.000000000Z"


def fail(message: str) -> NoReturn:
    raise SystemExit(f"error: {message}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_archive(path: Path, version: str, target: str) -> None:
    if target != EXPECTED_TARGET:
        fail(f"unsupported release target {target!r}")
    raw_header = path.read_bytes()[:10]
    if raw_header[:2] != b"\x1f\x8b":
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
        names = {member.name for member in members}
        if names != expected_members:
            fail(f"release archive members are {sorted(names)!r}, expected {sorted(expected_members)!r}")
        for member in members:
            if member.name.startswith("/") or ".." in Path(member.name).parts:
                fail(f"release archive contains unsafe member {member.name!r}")
            if member.uid != 0 or member.gid != 0 or member.mtime != 0:
                fail(f"release archive member metadata is not normalized: {member.name!r}")

        root_member = archive.getmember(root)
        if not root_member.isdir():
            fail("release archive root entry is not a directory")

        metadata_member = archive.getmember(f"{root}/RELEASE-METADATA.txt")
        if not metadata_member.isfile():
            fail("release archive metadata entry is not a regular file")
        metadata = archive.extractfile(metadata_member)
        if metadata is None:
            fail("could not read release metadata")
        metadata_lines = set(metadata.read().decode("utf-8").splitlines())
        expected_metadata = {
            "artifact=conxian-gateway",
            f"version={version}",
            f"target={target}",
            "source-date-epoch=0",
        }
        if not expected_metadata.issubset(metadata_lines):
            fail("release metadata does not identify the expected artifact")

        binary_member = archive.getmember(f"{root}/gateway")
        if (
            not binary_member.isfile()
            or binary_member.size == 0
            or binary_member.mode & 0o111 == 0
        ):
            fail("release archive does not contain a non-empty Gateway binary")
        binary = archive.extractfile(binary_member)
        if binary is None:
            fail("could not read Gateway binary from release archive")
        header = binary.read(20)
        if header[:4] != b"\x7fELF":
            fail("Gateway binary is not an ELF executable")
        if header[4] != 2 or struct.unpack_from("<H", header, 18)[0] != 62:
            fail("Gateway binary is not an x86_64 ELF executable")


def verify_sbom(path: Path, version: str, target: str) -> None:
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("bomFormat") != "CycloneDX":
        fail("SBOM is not CycloneDX")
    if document.get("specVersion") != "1.5":
        fail("SBOM specVersion is not 1.5")
    metadata = document.get("metadata") or {}
    if metadata.get("timestamp") != EXPECTED_SBOM_TIMESTAMP:
        fail("SBOM timestamp is not normalized")
    component = metadata.get("component") or {}
    if component.get("name") != "gateway" or component.get("version") != version:
        fail("SBOM root component does not match the release")
    properties = metadata.get("properties") or []
    if not any(
        item.get("name") == "cdx:rustc:sbom:target:triple"
        and item.get("value") == target
        for item in properties
    ):
        fail("SBOM target triple does not match the release")
    if not document.get("components"):
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
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    directory = args.directory.resolve()
    if not directory.is_dir():
        fail(f"artifact directory does not exist: {directory}")

    archive_name = f"conxian-gateway-{args.version}-{args.target}.tar.gz"
    sbom_name = f"conxian-gateway-{args.version}.cdx.json"
    checksums_name = f"conxian-gateway-{args.version}.sha256"
    expected_files = {archive_name, sbom_name, checksums_name}
    actual_files = {path.name for path in directory.iterdir() if path.is_file()}
    if actual_files != expected_files:
        fail(f"artifact files are {sorted(actual_files)!r}, expected {sorted(expected_files)!r}")

    verify_archive(directory / archive_name, args.version, args.target)
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
