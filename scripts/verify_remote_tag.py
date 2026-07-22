#!/usr/bin/env python3
"""Verify that a GitHub tag still peels to the release commit.

This is a last-moment TOCTOU check for release publication.  It deliberately
uses the GitHub REST API rather than a local checkout so a force-updated remote
tag cannot pass based on stale repository state.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from typing import Any


COMMIT = re.compile(r"^[0-9a-fA-F]{40}$")
MAX_TAG_OBJECTS = 32
API_VERSION = "2022-11-28"


class RemoteTagError(RuntimeError):
    """Raised when the remote tag cannot be resolved safely."""


def validate_sha(value: Any, field: str) -> str:
    if not isinstance(value, str) or COMMIT.fullmatch(value) is None:
        raise RemoteTagError(f"{field} is not a full 40-hex SHA")
    return value.lower()


def _api_get(repository: str, path: str, token: str) -> dict[str, Any]:
    url = f"https://api.github.com{path}"
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": API_VERSION,
            "User-Agent": "conxian-gateway-release-tag-check",
        },
        method="GET",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as error:
        status = getattr(error, "code", "unavailable")
        raise RemoteTagError(f"GitHub tag API request failed with status {status}") from error
    except (OSError, json.JSONDecodeError) as error:
        raise RemoteTagError("GitHub tag API returned an unreadable response") from error

    if not isinstance(payload, dict):
        raise RemoteTagError("GitHub tag API returned a non-object response")
    return payload


def resolve_remote_tag(
    repository: str,
    tag: str,
    token: str,
    *,
    fetch: Callable[[str], dict[str, Any]] | None = None,
) -> str:
    """Resolve a lightweight or annotated tag to its peeled commit SHA."""

    if not repository or "/" not in repository or repository.startswith(("/", "-")):
        raise RemoteTagError("repository must be an owner/repository name")
    if not tag or "\x00" in tag or tag.startswith(("/", "-")):
        raise RemoteTagError("tag name is empty or unsafe")
    if not token:
        raise RemoteTagError("GitHub API token is empty")

    if fetch is None:
        fetch = lambda path: _api_get(repository, path, token)

    encoded_tag = urllib.parse.quote(tag, safe="")
    response = fetch(f"/repos/{repository}/git/ref/tags/{encoded_tag}")
    current: Any = response.get("object")
    if not isinstance(current, dict):
        raise RemoteTagError("GitHub tag ref response has no object")

    for _ in range(MAX_TAG_OBJECTS):
        object_type = current.get("type")
        object_sha = current.get("sha")
        if object_type == "commit":
            return validate_sha(object_sha, "remote tag commit")
        if object_type != "tag":
            raise RemoteTagError(f"remote tag resolved to unsupported object type {object_type!r}")
        tag_sha = validate_sha(object_sha, "annotated tag object")
        tag_object = fetch(f"/repos/{repository}/git/tags/{tag_sha}")
        current = tag_object.get("object")
        if not isinstance(current, dict):
            raise RemoteTagError("annotated tag object has no target object")

    raise RemoteTagError(f"remote tag exceeds the {MAX_TAG_OBJECTS}-object peel limit")


def verify_remote_tag(
    repository: str,
    tag: str,
    expected_commit: str,
    token: str,
    *,
    fetch: Callable[[str], dict[str, Any]] | None = None,
) -> str:
    expected = validate_sha(expected_commit, "expected commit")
    actual = resolve_remote_tag(repository, tag, token, fetch=fetch)
    if actual != expected:
        raise RemoteTagError(
            f"remote tag {tag} resolves to {actual}, expected immutable commit {expected}"
        )
    return actual


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True, help="GitHub owner/repository")
    parser.add_argument("--tag", required=True)
    parser.add_argument("--expected-commit", required=True)
    parser.add_argument(
        "--token-env",
        default="GITHUB_TOKEN",
        help="Environment variable containing the GitHub API token (default: GITHUB_TOKEN)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    token = os.environ.get(args.token_env, "")
    try:
        actual = verify_remote_tag(args.repository, args.tag, args.expected_commit, token)
    except RemoteTagError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"remote tag {args.tag} still resolves to expected commit {actual}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
