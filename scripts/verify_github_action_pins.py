#!/usr/bin/env python3
"""Fail closed when GitHub workflows use mutable remote action references."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


USES_PATTERN = re.compile(r"^\s*(?:-\s*)?uses\s*:\s*(?P<value>.*)$")
FULL_COMMIT_PATTERN = re.compile(r"^[0-9a-fA-F]{40}$")
DOCKER_DIGEST_PATTERN = re.compile(r"^docker://\S+@sha256:[0-9a-fA-F]{64}$")


class PinError(ValueError):
    """Raised when a workflow contains a mutable or malformed uses reference."""


def strip_yaml_comment(value: str) -> str:
    """Remove an inline YAML comment without treating quoted hashes as comments."""
    quote: str | None = None
    escaped = False
    for index, character in enumerate(value):
        if quote == '"' and escaped:
            escaped = False
            continue
        if quote == '"' and character == "\\":
            escaped = True
            continue
        if character in {"'", '"'}:
            if quote is None:
                quote = character
            elif quote == character:
                quote = None
            continue
        if (
            character == "#"
            and quote is None
            and (index == 0 or value[index - 1].isspace())
        ):
            return value[:index].rstrip()
    if quote is not None:
        raise PinError("unterminated quoted uses value")
    return value.strip()


def parse_uses_value(raw_value: str) -> str:
    value = strip_yaml_comment(raw_value).strip()
    if not value:
        raise PinError("empty uses value")
    if value[0] in {"'", '"'}:
        if len(value) < 2 or value[-1] != value[0]:
            raise PinError("malformed quoted uses value")
        value = value[1:-1]
    if not value or any(character.isspace() for character in value):
        raise PinError("uses value must be a single scalar reference")
    return value


def validate_uses_reference(reference: str) -> None:
    if reference.startswith("./"):
        return
    if reference.startswith("docker://"):
        if not DOCKER_DIGEST_PATTERN.fullmatch(reference):
            raise PinError("docker:// references must use a full sha256 digest")
        return
    if "@" not in reference:
        raise PinError("remote references must include @<40-character commit SHA>")
    target, revision = reference.rsplit("@", 1)
    target_parts = target.split("/")
    if len(target_parts) < 2 or not all(target_parts):
        raise PinError("remote action or reusable workflow target is malformed")
    if not FULL_COMMIT_PATTERN.fullmatch(revision):
        raise PinError("remote references must use a full 40-character commit SHA")


def workflow_files(workflows_directory: Path) -> list[Path]:
    return sorted(
        path
        for path in workflows_directory.rglob("*")
        if path.is_file() and path.suffix in {".yml", ".yaml"}
    )


def scan_workflows(workflows_directory: Path) -> list[str]:
    errors: list[str] = []
    if not workflows_directory.is_dir():
        return [f"workflow directory does not exist: {workflows_directory}"]
    for workflow in workflow_files(workflows_directory):
        for line_number, line in enumerate(
            workflow.read_text(encoding="utf-8").splitlines(), 1
        ):
            match = USES_PATTERN.match(line)
            if match is None:
                continue
            try:
                reference = parse_uses_value(match.group("value"))
                validate_uses_reference(reference)
            except PinError as error:
                errors.append(f"{workflow}:{line_number}: {error}: {line.strip()}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--workflows-dir",
        type=Path,
        default=Path(".github/workflows"),
        help="workflow directory to scan (default: .github/workflows)",
    )
    args = parser.parse_args()
    errors = scan_workflows(args.workflows_dir)
    if errors:
        print("GitHub workflow pin verification failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(
        f"GitHub workflow pin verification passed ({len(workflow_files(args.workflows_dir))} files)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
