#!/usr/bin/env python3
"""Verify every mandatory production secret is declared in the deploy config.

Parses ``cmd/gateway/src/config.rs`` for ``get_mandatory_env("KEY", ...)`` calls
(the fail-closed secrets) and asserts each one is wired into the deploy configs
(``docker-compose.yml`` and, when present, ``render.yaml``). Fails CI on absence so
a required secret can never silently fall back to a dev default in production.

Usage (from the gateway repo root):
  python3 scripts/verify_required_secrets.py

Exit code is non-zero when any mandatory secret is missing from a deploy config.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

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
    return set(MANDATORY_RE.findall(config_rs))


def declared_keys(deploy_text: str, name: str) -> set[str]:
    if name == "docker-compose.yml":
        return set(COMPOSE_ENV_RE.findall(deploy_text))
    if name == "render.yaml":
        return set(RENDER_ENV_RE.findall(deploy_text))
    return set()


def main() -> int:
    if not CONFIG_RS.exists():
        print(f"config.rs not found at {CONFIG_RS} — skipping.")
        return 0

    required = mandatory_keys(_read(CONFIG_RS))
    if not required:
        print("No mandatory secrets found — is config.rs readable?")
        return 1

    failures = 0
    for name in DEPLOY_CONFIGS:
        path = ROOT / name
        if not path.exists():
            print(f"{name}: not present (skip)")
            continue
        declared = declared_keys(_read(path), name)
        missing = sorted(required - declared)
        if missing:
            failures += 1
            print(f"{name}: MISSING mandatory secrets:")
            for key in missing:
                print(f"  - {key}")
        else:
            print(f"{name}: all {len(required)} mandatory secrets declared")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
