#!/usr/bin/env python3
"""Verify every mandatory production secret is declared in the deploy config.

Parses ``cmd/gateway/src/config.rs`` for ``get_mandatory_env("KEY", ...)`` calls
(the fail-closed secrets) and asserts each one is wired into the deploy configs
(``docker-compose.yml`` and, when present, ``render.yaml``). Conditional provider
keys can be omitted from Render only when their single mode declaration is a
literal disabled value. Missing/dynamic/duplicate modes retain the requirement.
Fails CI on absence so
a required secret can never silently fall back to a dev default in production.

Usage (from the gateway repo root):
  python3 scripts/verify_required_secrets.py

Exit code is non-zero when any mandatory secret is missing from a deploy config.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

from startup_secret_preflight import CONDITIONAL, GROUPED

ROOT = Path(__file__).resolve().parent.parent
CONFIG_RS = ROOT / "cmd" / "gateway" / "src" / "config.rs"
DEPLOY_CONFIGS = ["docker-compose.yml", "render.yaml"]

# get_mandatory_env("KEY", ...) — key may be on the following line.
MANDATORY_RE = re.compile(r'get_mandatory_env\(\s*"([A-Z0-9_]+)"', re.DOTALL)
# docker-compose environment list entries: "      - KEY=..." or "      - KEY:"
COMPOSE_ENV_RE = re.compile(r"^\s*-\s*([A-Z0-9_]+)\s*[:=]", re.MULTILINE)
# render.yaml env var blocks: "        - key: KEY"
RENDER_ENV_RE = re.compile(r"^\s*-?\s*key:\s*([A-Z0-9_]+)", re.MULTILINE)


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8") if path.exists() else ""


def mandatory_keys(config_rs: str) -> set[str]:
    keys = set(MANDATORY_RE.findall(config_rs))
    # The runtime accepts one logical API token with a legacy input alias.
    # New deploy configurations must still declare its canonical name.
    if re.search(r"let\s+api_token\s*=\s*resolve_api_token_with\(", config_rs):
        keys.add("API_TOKEN")
    return keys


def declared_keys(deploy_text: str, name: str) -> set[str]:
    if name == "docker-compose.yml":
        return set(COMPOSE_ENV_RE.findall(deploy_text))
    if name == "render.yaml":
        return set(RENDER_ENV_RE.findall(deploy_text))
    return set()


def required_keys(config_rs: str, deploy_text: str, name: str) -> set[str]:
    required = mandatory_keys(config_rs)
    if name != "render.yaml":
        return required
    declarations = RENDER_ENV_RE.findall(deploy_text)
    def explicitly_disabled(mode: str) -> bool:
        disabled = re.search(
            rf'^([ ]*)- key: {re.escape(mode)}[ \t]*\n\1[ ]+value: '
            r'(?:disabled|"disabled"|\'disabled\')[ \t]*(?:#.*)?$',
            deploy_text, re.MULTILINE,
        )
        return declarations.count(mode) == 1 and bool(disabled)

    for mode, secret in CONDITIONAL.items():
        if explicitly_disabled(mode):
            required.discard(secret)
    for secret, modes in GROUPED.items():
        if all(explicitly_disabled(mode) for mode in modes):
            required.discard(secret)
    return required


def main() -> int:
    if not CONFIG_RS.exists():
        print("config.rs is unavailable; required-secret contract cannot be verified.")
        return 1

    required = mandatory_keys(_read(CONFIG_RS))
    if not required:
        print("No mandatory secrets found — is config.rs readable?")
        return 1

    failures = 0
    for name in DEPLOY_CONFIGS:
        path = ROOT / name
        if not path.exists():
            if name == "docker-compose.yml":
                failures += 1
                print("docker-compose.yml: required deploy config unavailable")
                continue
            print(f"{name}: not present (skip)")
            continue
        text = _read(path)
        declared = declared_keys(text, name)
        applicable = required_keys(_read(CONFIG_RS), text, name)
        missing = sorted(applicable - declared)
        if missing:
            failures += 1
            print(f"{name}: MISSING mandatory secrets:")
            for key in missing:
                print(f"  - {key}")
        else:
            print(f"{name}: all {len(applicable)} applicable mandatory secrets declared")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

