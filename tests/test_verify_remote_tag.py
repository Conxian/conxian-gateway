#!/usr/bin/env python3
"""Regression tests for the release tag TOCTOU guard."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
import urllib.error
import urllib.request


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "verify_remote_tag.py"
WORKFLOW = Path(__file__).resolve().parents[1] / ".github" / "workflows" / "release.yml"
COMMIT = "0123456789abcdef0123456789abcdef01234567"
TAG_OBJECT = "abcdef0123456789abcdef0123456789abcdef01"
TAG_OBJECT_2 = "fedcba9876543210fedcba9876543210fedcba98"


def load_script():
    spec = importlib.util.spec_from_file_location("verify_remote_tag", SCRIPT)
    if spec is None or spec.loader is None:
        raise AssertionError("could not load remote tag verifier")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VERIFY_REMOTE_TAG = load_script()


class VerifyRemoteTagTests(unittest.TestCase):
    def test_resolves_lightweight_tag(self) -> None:
        calls: list[str] = []

        def fetch(path: str) -> dict[str, object]:
            calls.append(path)
            return {"object": {"type": "commit", "sha": COMMIT}}

        actual = VERIFY_REMOTE_TAG.resolve_remote_tag(
            "Conxian/conxian-gateway",
            "v0.1.4",
            "test-token",
            fetch=fetch,
        )
        self.assertEqual(actual, COMMIT)
        self.assertEqual(calls, ["/repos/Conxian/conxian-gateway/git/ref/tags/v0.1.4"])

    def test_peels_annotated_tag_chain(self) -> None:
        responses = {
            "/repos/Conxian/conxian-gateway/git/ref/tags/v0.1.4": {
                "object": {"type": "tag", "sha": TAG_OBJECT}
            },
            f"/repos/Conxian/conxian-gateway/git/tags/{TAG_OBJECT}": {
                "object": {"type": "tag", "sha": TAG_OBJECT_2}
            },
            f"/repos/Conxian/conxian-gateway/git/tags/{TAG_OBJECT_2}": {
                "object": {"type": "commit", "sha": COMMIT}
            },
        }
        calls: list[str] = []

        def fetch(path: str) -> dict[str, object]:
            calls.append(path)
            return responses[path]

        self.assertEqual(
            VERIFY_REMOTE_TAG.resolve_remote_tag(
                "Conxian/conxian-gateway", "v0.1.4", "test-token", fetch=fetch
            ),
            COMMIT,
        )
        self.assertEqual(list(responses), calls)

    def test_rejects_remote_tag_commit_mismatch(self) -> None:
        with self.assertRaises(VERIFY_REMOTE_TAG.RemoteTagError) as context:
            VERIFY_REMOTE_TAG.verify_remote_tag(
                "Conxian/conxian-gateway",
                "v0.1.4",
                "fedcba9876543210fedcba9876543210fedcba98",
                "test-token",
                fetch=lambda _path: {"object": {"type": "commit", "sha": COMMIT}},
            )
        self.assertIn("expected immutable commit", str(context.exception))

    def test_rejects_unsupported_remote_object(self) -> None:
        with self.assertRaises(VERIFY_REMOTE_TAG.RemoteTagError) as context:
            VERIFY_REMOTE_TAG.resolve_remote_tag(
                "Conxian/conxian-gateway",
                "v0.1.4",
                "test-token",
                fetch=lambda _path: {"object": {"type": "tree", "sha": COMMIT}},
            )
        self.assertIn("unsupported object type", str(context.exception))

    def test_api_request_keeps_token_in_header_not_url(self) -> None:
        response = patch.object(VERIFY_REMOTE_TAG.urllib.request, "urlopen")
        with response as urlopen:
            payload = b'{"object":{"type":"commit","sha":"' + COMMIT.encode() + b'"}}'

            class FakeResponse:
                def __enter__(self):
                    return self

                def __exit__(self, *_args):
                    return False

                def read(self):
                    return payload

            urlopen.return_value = FakeResponse()
            result = VERIFY_REMOTE_TAG._api_get(
                "Conxian/conxian-gateway", "/repos/Conxian/conxian-gateway/git/ref/tags/v0.1.4", "secret-token"
            )
        request = urlopen.call_args.args[0]
        self.assertIsInstance(request, urllib.request.Request)
        self.assertNotIn("secret-token", request.full_url)
        self.assertEqual(request.get_header("Authorization"), "Bearer secret-token")
        self.assertEqual(result["object"]["sha"], COMMIT)

    def test_api_request_rejects_http_failure(self) -> None:
        error = urllib.error.HTTPError(
            "https://api.github.com/repos/Conxian/conxian-gateway/git/ref/tags/v0.1.4",
            404,
            "not found",
            {},
            None,
        )
        with patch.object(VERIFY_REMOTE_TAG.urllib.request, "urlopen", side_effect=error):
            with self.assertRaises(VERIFY_REMOTE_TAG.RemoteTagError) as context:
                VERIFY_REMOTE_TAG._api_get(
                    "Conxian/conxian-gateway",
                    "/repos/Conxian/conxian-gateway/git/ref/tags/v0.1.4",
                    "test-token",
                )
        self.assertIn("status 404", str(context.exception))

    def test_api_request_rejects_malformed_json(self) -> None:
        class FakeResponse:
            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

            def read(self):
                return b"{not-json"

        with patch.object(VERIFY_REMOTE_TAG.urllib.request, "urlopen", return_value=FakeResponse()):
            with self.assertRaises(VERIFY_REMOTE_TAG.RemoteTagError) as context:
                VERIFY_REMOTE_TAG._api_get(
                    "Conxian/conxian-gateway",
                    "/repos/Conxian/conxian-gateway/git/ref/tags/v0.1.4",
                    "test-token",
                )
        self.assertIn("unreadable response", str(context.exception))

    def test_encodes_tag_name_as_one_path_segment(self) -> None:
        calls: list[str] = []

        def fetch(path: str) -> dict[str, object]:
            calls.append(path)
            return {"object": {"type": "commit", "sha": COMMIT}}

        self.assertEqual(
            VERIFY_REMOTE_TAG.resolve_remote_tag(
                "Conxian/conxian-gateway",
                "release/foo#bar?x=1",
                "test-token",
                fetch=fetch,
            ),
            COMMIT,
        )
        self.assertEqual(
            calls,
            [
                "/repos/Conxian/conxian-gateway/git/ref/tags/"
                "release%2Ffoo%23bar%3Fx%3D1"
            ],
        )

    def test_rejects_annotated_tag_cycle(self) -> None:
        calls: list[str] = []

        def fetch(path: str) -> dict[str, object]:
            calls.append(path)
            if path.endswith("/git/ref/tags/v0.1.4"):
                return {"object": {"type": "tag", "sha": TAG_OBJECT}}
            return {"object": {"type": "tag", "sha": TAG_OBJECT}}

        with self.assertRaises(VERIFY_REMOTE_TAG.RemoteTagError) as context:
            VERIFY_REMOTE_TAG.resolve_remote_tag(
                "Conxian/conxian-gateway", "v0.1.4", "test-token", fetch=fetch
            )
        self.assertIn("contains a cycle", str(context.exception))
        self.assertEqual(len(calls), 2)

    def test_accepts_maximum_annotated_tag_depth(self) -> None:
        tag_count = VERIFY_REMOTE_TAG.MAX_TAG_OBJECTS
        tag_shas = [f"{index + 1:040x}" for index in range(tag_count)]
        responses: dict[str, dict[str, object]] = {}
        for index, tag_sha in enumerate(tag_shas):
            target: dict[str, str]
            if index + 1 == tag_count:
                target = {"type": "commit", "sha": COMMIT}
            else:
                target = {"type": "tag", "sha": tag_shas[index + 1]}
            responses[f"/repos/Conxian/conxian-gateway/git/tags/{tag_sha}"] = {"object": target}

        def fetch(path: str) -> dict[str, object]:
            if path.endswith("/git/ref/tags/v0.1.4"):
                return {"object": {"type": "tag", "sha": tag_shas[0]}}
            return responses[path]

        self.assertEqual(
            VERIFY_REMOTE_TAG.resolve_remote_tag(
                "Conxian/conxian-gateway", "v0.1.4", "test-token", fetch=fetch
            ),
            COMMIT,
        )

    def test_rejects_annotated_tag_depth_exhaustion(self) -> None:
        tag_count = VERIFY_REMOTE_TAG.MAX_TAG_OBJECTS + 1
        tag_shas = [f"{index + 1:040x}" for index in range(tag_count)]
        responses: dict[str, dict[str, object]] = {}
        for index, tag_sha in enumerate(tag_shas[:-1]):
            responses[f"/repos/Conxian/conxian-gateway/git/tags/{tag_sha}"] = {
                "object": {"type": "tag", "sha": tag_shas[index + 1]}
            }
        responses[f"/repos/Conxian/conxian-gateway/git/tags/{tag_shas[-1]}"] = {
            "object": {"type": "commit", "sha": COMMIT}
        }

        def fetch(path: str) -> dict[str, object]:
            if path.endswith("/git/ref/tags/v0.1.4"):
                return {"object": {"type": "tag", "sha": tag_shas[0]}}
            return responses[path]

        with self.assertRaises(VERIFY_REMOTE_TAG.RemoteTagError) as context:
            VERIFY_REMOTE_TAG.resolve_remote_tag(
                "Conxian/conxian-gateway", "v0.1.4", "test-token", fetch=fetch
            )
        self.assertIn("exceeds the 32-object peel limit", str(context.exception))

    def test_workflow_places_rechecks_immediately_before_publication(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        github_recheck = source.index("Recheck remote release tag immediately before GitHub Release")
        github_release = source.index("uses: softprops/action-gh-release", github_recheck)
        crates_recheck = source.index("Recheck remote release tag immediately before crates.io publication")
        crates_publish = source.index("run: cargo publish --locked --package gateway", crates_recheck)

        github_slice = source[github_recheck:github_release]
        crates_slice = source[crates_recheck:crates_publish]
        for section in (github_slice, crates_slice):
            self.assertIn("python3 scripts/verify_remote_tag.py", section)
            self.assertIn('--repository "${GITHUB_REPOSITORY}"', section)
            self.assertIn('--tag "${RELEASE_TAG}"', section)
            self.assertIn('--expected-commit "${EXPECTED_COMMIT}"', section)
            self.assertIn("GITHUB_TOKEN: ${{ github.token }}", section)
            self.assertNotIn("set -x", section)
        self.assertEqual(source.count("python3 scripts/verify_remote_tag.py"), 2)
        self.assertIn("commit: ${{ steps.identity.outputs.commit }}", source)


if __name__ == "__main__":
    unittest.main()
