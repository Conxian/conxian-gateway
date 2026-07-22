#!/usr/bin/env python3
"""Verify the deterministic Gateway release artifact set."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import posixpath
import re
import struct
import tarfile
import zlib
from collections import Counter
from pathlib import Path
from typing import Any, NoReturn


CHECKSUM_LINE = re.compile(r"^(?P<digest>[0-9a-f]{64})  (?P<name>[^/\n]+)$")
COMMIT = re.compile(r"^[0-9a-fA-F]{40}$")
METADATA_KEY = re.compile(r"^[a-z][a-z0-9-]*$")
EXPECTED_TARGET = "x86_64-unknown-linux-gnu"
EXPECTED_TOOLCHAIN = "1.96.0"
EXPECTED_SBOM_TIMESTAMP = "1970-01-01T00:00:00.000000000Z"
USTAR_BLOCK_SIZE = 512
USTAR_MAGIC = b"ustar\x0000"
USTAR_ZERO_BLOCK = b"\x00" * USTAR_BLOCK_SIZE
STABLE_WORKSPACE_URI = "file:///conxian-gateway"


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


def parse_ustar_number(field: bytes, field_name: str) -> int:
    value = field.rstrip(b"\x00 ")
    if not value or any(byte < ord("0") or byte > ord("7") for byte in value):
        fail(f"release archive has an invalid USTAR {field_name} field")
    return int(value, 8)


def read_single_gzip_member(path: Path) -> bytes:
    """Read exactly one reproducible gzip member without concatenation support."""

    try:
        compressed = path.read_bytes()
    except OSError as error:
        fail(f"release archive gzip stream could not be opened: {error}")

    if len(compressed) < 10 or compressed[:2] != b"\x1f\x8b" or compressed[2] != 8:
        fail("release archive is not gzip-compressed")
    if int.from_bytes(compressed[4:8], "little") != 0:
        fail("release archive gzip timestamp is not reproducible")

    decompressor = zlib.decompressobj(wbits=16 + zlib.MAX_WBITS)
    try:
        payload = decompressor.decompress(compressed)
        payload += decompressor.flush()
    except zlib.error as error:
        fail(f"release archive gzip stream could not be opened: {error}")
    if not decompressor.eof:
        fail("release archive gzip stream is truncated")
    if decompressor.unused_data or decompressor.unconsumed_tail:
        fail("release archive must contain exactly one gzip member with no trailing data")
    return payload


def verify_ustar_headers(path: Path) -> tuple[list[tuple[bytes, int]], bytes]:
    """Scan raw tar blocks so tarfile cannot hide extension headers."""

    headers: list[tuple[bytes, int]] = []
    payload = read_single_gzip_member(path)
    offset = 0
    while offset < len(payload):
        header = payload[offset : offset + USTAR_BLOCK_SIZE]
        if len(header) != USTAR_BLOCK_SIZE:
            fail("release archive has a truncated USTAR header")
        offset += USTAR_BLOCK_SIZE
        if header == USTAR_ZERO_BLOCK:
            trailer = payload[offset : offset + USTAR_BLOCK_SIZE]
            if trailer != USTAR_ZERO_BLOCK:
                fail("release archive does not end with two USTAR zero blocks")
            offset += USTAR_BLOCK_SIZE
            if offset != len(payload):
                fail("release archive contains data after the USTAR trailer")
            break

        if header[257:265] != USTAR_MAGIC:
            fail("release archive contains a non-USTAR header")
        if header[148:156] == b"        ":
            fail("release archive USTAR header has a blank checksum")
        stored_checksum = parse_ustar_number(header[148:156], "checksum")
        calculated_checksum = sum(header[:148]) + sum(b" " * 8) + sum(header[156:])
        if stored_checksum != calculated_checksum:
            fail("release archive USTAR header checksum is invalid")

        typeflag = header[156:157]
        if typeflag not in {b"0", b"5"}:
            fail(
                "release archive member has an unsafe type or non-regular "
                f"USTAR extension: {typeflag!r}"
            )
        if header[157:257] != b"\x00" * 100:
            fail("release archive regular/directory member contains a link target")
        if header[265:329] != b"\x00" * 64 or header[329:345] != b"\x00" * 16:
            fail("release archive member metadata contains non-normalized USTAR fields")
        if header[345:500] != b"\x00" * 155:
            fail(
                "release archive contains a USTAR prefix, GNU sparse indicator, "
                "or other non-USTAR extension"
            )
        if header[500:512] != b"\x00" * 12:
            fail("release archive contains non-USTAR header padding")

        size = parse_ustar_number(header[124:136], "size")
        if typeflag == b"5" and size != 0:
            fail("release archive directory member has non-zero data size")
        headers.append((typeflag, size))

        data_blocks = (size + USTAR_BLOCK_SIZE - 1) // USTAR_BLOCK_SIZE
        data_end = offset + data_blocks * USTAR_BLOCK_SIZE
        if data_end > len(payload):
            fail("release archive member data is truncated")
        offset = data_end

    if not headers:
        fail("release archive contains no USTAR members")
    return headers, payload


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
    root = f"conxian-gateway-{version}-{target}"
    expected_members = {
        root,
        f"{root}/RELEASE-METADATA.txt",
        f"{root}/gateway",
    }
    raw_headers, tar_payload = verify_ustar_headers(path)
    with tarfile.open(fileobj=io.BytesIO(tar_payload), mode="r:") as archive:
        members = archive.getmembers()
        if len(members) != len(raw_headers):
            fail("release archive tar parser member count differs from raw USTAR headers")
        names = [member.name for member in members]
        for member, (raw_type, raw_size) in zip(members, raw_headers):
            reject_unsafe_member_name(member.name)
            if member.name not in expected_members:
                fail(f"release archive contains unexpected member {member.name!r}")
            if member.type != raw_type or member.size != raw_size:
                fail(f"release archive member metadata disagrees with its USTAR header: {member.name!r}")
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


def require_safe_string(value: Any, field: str, *, allow_empty: bool = False) -> str:
    if not isinstance(value, str) or (not allow_empty and not value):
        fail(f"SBOM {field} must be a string")
    if any(ord(character) < 0x20 or ord(character) == 0x7F for character in value):
        fail(f"SBOM {field} contains an unsafe control character")
    return value


def validate_component(component: Any, context: str, bom_refs: dict[str, tuple[str, str]]) -> None:
    if not isinstance(component, dict):
        fail(f"SBOM {context} component is not an object")
    bom_ref = require_safe_string(component.get("bom-ref"), f"{context} bom-ref")
    if not bom_ref.strip():
        fail(f"SBOM {context} bom-ref is empty")
    if bom_ref != bom_ref.strip():
        fail(f"SBOM {context} bom-ref is malformed")
    name = require_safe_string(component.get("name"), f"{context} name")
    version = require_safe_string(component.get("version"), f"{context} version")
    require_safe_string(component.get("type"), f"{context} type")
    if bom_ref in bom_refs:
        fail(f"SBOM contains duplicate component bom-ref {bom_ref!r}")
    bom_refs[bom_ref] = (name, version)

    for field in ("author", "description", "purl", "scope"):
        if field in component:
            require_safe_string(component[field], f"{context} {field}")

    licenses = component.get("licenses")
    if licenses is not None:
        if not isinstance(licenses, list):
            fail(f"SBOM {context} licenses are not an array")
        for index, license_entry in enumerate(licenses):
            if not isinstance(license_entry, dict):
                fail(f"SBOM {context} license {index} is not an object")
            for field in ("id", "name", "expression"):
                if field in license_entry:
                    require_safe_string(license_entry[field], f"{context} license {index} {field}")

    hashes = component.get("hashes")
    if hashes is not None:
        if not isinstance(hashes, list):
            fail(f"SBOM {context} hashes are not an array")
        for index, hash_entry in enumerate(hashes):
            if not isinstance(hash_entry, dict):
                fail(f"SBOM {context} hash {index} is not an object")
            require_safe_string(hash_entry.get("alg"), f"{context} hash {index} alg")
            require_safe_string(hash_entry.get("content"), f"{context} hash {index} content")

    external_references = component.get("externalReferences")
    if external_references is not None:
        if not isinstance(external_references, list):
            fail(f"SBOM {context} externalReferences are not an array")
        for index, reference in enumerate(external_references):
            if not isinstance(reference, dict):
                fail(f"SBOM {context} external reference {index} is not an object")
            require_safe_string(reference.get("type"), f"{context} external reference {index} type")
            require_safe_string(reference.get("url"), f"{context} external reference {index} url")

    nested_components = component.get("components")
    if nested_components is not None:
        if not isinstance(nested_components, list):
            fail(f"SBOM {context} components are not an array")
        for index, nested in enumerate(nested_components):
            validate_component(nested, f"{context}.components[{index}]", bom_refs)


def load_workspace_packages(path: Path) -> dict[str, tuple[str, str]]:
    try:
        metadata: Any = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        fail("locked Cargo metadata is not valid UTF-8 JSON")
    if not isinstance(metadata, dict):
        fail("locked Cargo metadata root is not an object")

    workspace_root = require_safe_string(metadata.get("workspace_root"), "Cargo metadata workspace_root")
    workspace_uri = Path(workspace_root).resolve().as_uri()
    packages = metadata.get("packages")
    workspace_members = metadata.get("workspace_members")
    if not isinstance(packages, list) or not isinstance(workspace_members, list) or not workspace_members:
        fail("locked Cargo metadata has an invalid workspace package inventory")

    packages_by_id: dict[str, dict[str, Any]] = {}
    for index, package in enumerate(packages):
        if not isinstance(package, dict):
            fail(f"locked Cargo metadata package {index} is not an object")
        package_id = require_safe_string(package.get("id"), f"Cargo metadata package {index} id")
        package_name = require_safe_string(package.get("name"), f"Cargo metadata package {index} name")
        package_version = require_safe_string(package.get("version"), f"Cargo metadata package {index} version")
        if package_id in packages_by_id:
            fail(f"locked Cargo metadata contains duplicate package id {package_id!r}")
        packages_by_id[package_id] = {
            "name": package_name,
            "version": package_version,
        }

    if any(not isinstance(member, str) for member in workspace_members):
        fail("locked Cargo metadata workspace_members must contain only strings")
    if len(set(workspace_members)) != len(workspace_members):
        fail("locked Cargo metadata contains duplicate workspace members")

    expected: dict[str, tuple[str, str]] = {}
    for member in workspace_members:
        package = packages_by_id.get(member)
        if package is None:
            fail(f"locked Cargo metadata workspace member is missing from packages: {member!r}")
        normalized_ref = member.replace(workspace_uri, STABLE_WORKSPACE_URI)
        if normalized_ref in expected:
            fail(f"locked Cargo metadata normalized workspace refs collide: {normalized_ref!r}")
        expected[normalized_ref] = (package["name"], package["version"])
    return expected


def verify_sbom(path: Path, version: str, target: str, cargo_metadata_path: Path) -> None:
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
    if isinstance(document.get("version"), bool) or not isinstance(document.get("version"), int):
        fail("SBOM document version is not an integer")
    if document["version"] < 1:
        fail("SBOM document version is invalid")
    if "serialNumber" in document:
        require_safe_string(document["serialNumber"], "serialNumber")

    metadata = document.get("metadata")
    if not isinstance(metadata, dict):
        fail("SBOM metadata is not an object")
    if metadata.get("timestamp") != EXPECTED_SBOM_TIMESTAMP:
        fail("SBOM timestamp is not normalized")
    component = metadata.get("component")
    if not isinstance(component, dict):
        fail("SBOM root component is not an object")

    properties = metadata.get("properties")
    if not isinstance(properties, list):
        fail("SBOM properties are not an array")
    property_names: set[str] = set()
    for index, item in enumerate(properties):
        if not isinstance(item, dict):
            fail(f"SBOM property {index} is not an object")
        property_name = require_safe_string(item.get("name"), f"property {index} name")
        require_safe_string(item.get("value"), f"property {index} value")
        if property_name in property_names:
            fail(f"SBOM contains duplicate property {property_name!r}")
        property_names.add(property_name)
    if not any(
        item.get("name") == "cdx:rustc:sbom:target:triple" and item.get("value") == target
        for item in properties
    ):
        fail("SBOM target triple does not match the release")

    tools = metadata.get("tools")
    if tools is not None:
        if not isinstance(tools, list):
            fail("SBOM metadata tools are not an array")
        for index, tool in enumerate(tools):
            if not isinstance(tool, dict):
                fail(f"SBOM metadata tool {index} is not an object")
            for field in ("vendor", "name", "version"):
                if field in tool:
                    require_safe_string(tool[field], f"metadata tool {index} {field}")

    bom_refs: dict[str, tuple[str, str]] = {}
    validate_component(component, "metadata", bom_refs)
    if component.get("name") != "gateway" or component.get("version") != version:
        fail("SBOM root component does not match the release")

    components = document.get("components")
    if not isinstance(components, list) or not components:
        fail("SBOM has no dependency components")
    for index, dependency_component in enumerate(components):
        validate_component(dependency_component, f"components[{index}]", bom_refs)

    expected_workspace = load_workspace_packages(cargo_metadata_path)
    gateway_refs = [
        bom_ref for bom_ref, package in expected_workspace.items() if package[0] == "gateway"
    ]
    if len(gateway_refs) != 1:
        fail("locked Cargo metadata must contain exactly one gateway workspace package")
    gateway_ref = gateway_refs[0]
    if component.get("bom-ref") != gateway_ref:
        fail("SBOM root component bom-ref does not match locked gateway metadata")

    top_level_workspace_refs = {
        dependency_component["bom-ref"]
        for dependency_component in components
        if dependency_component["bom-ref"].startswith("path+file://")
    }
    expected_non_gateway_refs = set(expected_workspace) - {gateway_ref}
    if top_level_workspace_refs != expected_non_gateway_refs:
        fail(
            "SBOM workspace component refs do not exactly match locked Cargo metadata: "
            f"{sorted(top_level_workspace_refs)!r} != {sorted(expected_non_gateway_refs)!r}"
        )
    represented_workspace = {
        gateway_ref: (component["name"], component["version"]),
        **{
            dependency_component["bom-ref"]: (
                dependency_component["name"],
                dependency_component["version"],
            )
            for dependency_component in components
            if dependency_component["bom-ref"] in expected_non_gateway_refs
        },
    }
    if represented_workspace != expected_workspace:
        fail("SBOM workspace package names and versions do not match locked Cargo metadata")

    dependencies = document.get("dependencies")
    if not isinstance(dependencies, list) or not dependencies:
        fail("SBOM dependencies are not a non-empty array")
    dependency_refs: set[str] = set()
    for index, dependency in enumerate(dependencies):
        if not isinstance(dependency, dict):
            fail(f"SBOM dependency {index} is not an object")
        dependency_ref = require_safe_string(dependency.get("ref"), f"dependency {index} ref")
        if dependency_ref in dependency_refs:
            fail(f"SBOM contains duplicate dependency ref {dependency_ref!r}")
        if dependency_ref not in bom_refs:
            fail(f"SBOM dependency ref does not resolve to a component: {dependency_ref!r}")
        dependency_refs.add(dependency_ref)
        depends_on = dependency.get("dependsOn", [])
        if not isinstance(depends_on, list):
            fail(f"SBOM dependency {index} dependsOn is not an array")
        for target_ref in depends_on:
            target_ref = require_safe_string(target_ref, f"dependency {index} dependsOn entry")
            if target_ref not in bom_refs:
                fail(f"SBOM dependency target does not resolve to a component: {target_ref!r}")
    if not set(expected_workspace).issubset(dependency_refs):
        fail("SBOM dependency inventory is missing a locked workspace package")
    # cargo-cyclonedx emits target descriptors nested under the metadata root;
    # the dependency graph consists of that root plus document-level components.
    graph_component_refs = {
        component["bom-ref"],
        *(dependency_component["bom-ref"] for dependency_component in components),
    }
    if dependency_refs != graph_component_refs:
        missing_refs = sorted(graph_component_refs - dependency_refs)
        orphan_refs = sorted(dependency_refs - graph_component_refs)
        fail(
            "SBOM dependency graph does not exactly represent top-level components: "
            f"missing={missing_refs!r}, orphan={orphan_refs!r}"
        )


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
    parser.add_argument(
        "--cargo-metadata",
        required=True,
        type=Path,
        help="locked cargo metadata JSON generated from the release checkout",
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
    verify_sbom(directory / sbom_name, args.version, args.target, args.cargo_metadata)
    verify_checksums(
        directory / checksums_name,
        {archive_name, sbom_name},
        directory,
    )
    print(f"verified release artifacts for {args.version} ({args.target})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
