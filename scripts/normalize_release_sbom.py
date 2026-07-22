#!/usr/bin/env python3
"""Normalize the Gateway CycloneDX SBOM for release publication."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


TIMESTAMP = "1970-01-01T00:00:00.000000000Z"
STABLE_WORKSPACE_URI = "file:///conxian-gateway"


def normalize_workspace_references(value: Any, workspace_uri: str) -> Any:
    if isinstance(value, dict):
        return {
            key: normalize_workspace_references(item, workspace_uri)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [normalize_workspace_references(item, workspace_uri) for item in value]
    if isinstance(value, str):
        return value.replace(workspace_uri, STABLE_WORKSPACE_URI)
    return value


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--workspace-root", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    document = json.loads(args.input.read_text(encoding="utf-8"))

    if document.get("bomFormat") != "CycloneDX":
        raise SystemExit("error: SBOM is not CycloneDX")
    if document.get("specVersion") != "1.5":
        raise SystemExit("error: SBOM specVersion must be 1.5")

    metadata = document.get("metadata") or {}
    component = metadata.get("component") or {}
    if component.get("name") != "gateway":
        raise SystemExit("error: SBOM root component is not gateway")
    if component.get("version") != args.version:
        raise SystemExit(
            f"error: SBOM version {component.get('version')!r} != {args.version!r}"
        )

    properties = metadata.get("properties") or []
    target_property = next(
        (
            item
            for item in properties
            if item.get("name") == "cdx:rustc:sbom:target:triple"
        ),
        None,
    )
    if target_property is None or target_property.get("value") != args.target:
        raise SystemExit("error: SBOM target triple does not match the release target")
    if not document.get("components"):
        raise SystemExit("error: SBOM has no dependency components")

    document["metadata"]["timestamp"] = TIMESTAMP
    document = normalize_workspace_references(
        document,
        args.workspace_root.resolve().as_uri(),
    )

    serialized = json.dumps(document, indent=2, sort_keys=True) + "\n"
    if str(args.workspace_root.resolve()) in serialized:
        raise SystemExit("error: SBOM still contains an absolute workspace path")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(serialized, encoding="utf-8")
    print(
        f"normalized CycloneDX 1.5 SBOM: {args.output} "
        f"({len(document['components'])} components)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
