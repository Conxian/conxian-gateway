#!/usr/bin/env python3
"""Check startup-secret presence locally without exporting values or fingerprints.

Run only within the approved operator/secret boundary. This performs no network
or provider calls and does not authorize deployment, signing or settlement.
"""
import argparse
import json
import os
from typing import Mapping

REQUIRED = ("API_TOKEN", "SETTLEMENT_INGRESS_SECRET", "OFFLINE_QUEUE_SECRET")
FIAT_MODES = ("RAMP_MODE", "STITCH_MODE", "OZOW_MODE", "PAPSS_MODE",
              "ALCHEMY_PAY_MODE", "BANXA_MODE")
GROUPED = {"FIAT_WEBHOOK_SECRET": FIAT_MODES, "HMAC_SECRET": ("INFOBIP_MODE",)}
CONDITIONAL = {"RAMP_MODE": "RAMP_API_KEY", "ALCHEMY_PAY_MODE": "ALCHEMY_PAY_APP_ID",
               "INFOBIP_MODE": "INFOBIP_API_KEY"}
ENABLED_MODES = frozenset(("active", "shadow", "true", "1"))
LEGACY_API_TOKEN = "CONXIAN_GATEWAY_AUTH_TOKEN"


def describe() -> dict:
    """Describe requirements without inspecting an environment or credentials."""
    return {"schemaVersion": 1, "scope": "startup-secret-contract-only",
            "minimumRequiredCount": len(REQUIRED), "required": list(REQUIRED),
            "conditional": dict(CONDITIONAL), "enabledModes": sorted(ENABLED_MODES),
            "conditionalAny": {key: list(modes) for key, modes in GROUPED.items()},
            "maximumConditionalRequiredCount": len(REQUIRED) + len(CONDITIONAL) + len(GROUPED),
            "aliases": {"API_TOKEN": [LEGACY_API_TOKEN]},
            "aliasPolicy": "canonical-absence-only",
            "environmentInspected": False, "runtimeReadinessEstablished": False}


def markdown_contract() -> str:
    lines = ["<!-- STARTUP-SECRET-CONTRACT:BEGIN -->",
             "### Startup secret requirements", "",
             f"The minimum runtime lane requires **{len(REQUIRED)} logical startup secrets**:",
             ", ".join(f"`{key}`" for key in REQUIRED) + ".", "",
             "| Enabled provider mode | Additional startup credential |",
             "| --- | --- |"]
    lines.extend(f"| `{mode}` | `{key}` |" for mode, key in CONDITIONAL.items())
    lines.extend(f"| {' or '.join(f'`{mode}`' for mode in modes)} | `{key}` |"
                 for key, modes in GROUPED.items())
    lines.extend(["", "A provider credential is required only for mode values "
                  + ", ".join(f"`{mode}`" for mode in sorted(ENABLED_MODES)) + ".",
                  "The legacy token alias is one logical requirement, not an extra secret.",
                  "Optional managed-auth/provider credentials and deployment, persistence, network,",
                  "signer and release acceptance remain separate operation-specific requirements.",
                  "", "Describe this source contract without inspecting the environment:",
                  "`python3 scripts/startup_secret_preflight.py --describe`.",
                  "Check injected secret presence only inside the approved operator boundary:",
                  "`python3 scripts/startup_secret_preflight.py`.",
                  "Neither command proves provisioning, secret strength or runtime readiness.",
                  "<!-- STARTUP-SECRET-CONTRACT:END -->"])
    return "\n".join(lines)


def check(environment: Mapping[str, str]) -> dict:
    required = list(REQUIRED)
    for mode, key in CONDITIONAL.items():
        if environment.get(mode, "").strip().lower() in ENABLED_MODES:
            required.append(key)
    for key, modes in GROUPED.items():
        if any(environment.get(mode, "").strip().lower() in ENABLED_MODES for mode in modes):
            required.append(key)
    missing, invalid = [], []
    for key in required:
        source_key = LEGACY_API_TOKEN if key == "API_TOKEN" and key not in environment else key
        value = environment.get(source_key, "").strip()
        if key == "API_TOKEN":
            try:
                value.encode("utf-8")
            except UnicodeError:
                invalid.append(key)
                continue
        if not value:
            missing.append(key)
        elif value.startswith("sentinel_") or (
                key == "API_TOKEN" and value == "CONXIAN_PROD_AUTH_TOKEN_REQUIRED"):
            invalid.append(key)
    left = environment.get("FIAT_WEBHOOK_SECRET", "").strip()
    right = environment.get("SETTLEMENT_INGRESS_SECRET", "").strip()
    if "FIAT_WEBHOOK_SECRET" in required and left and right and left == right:
        invalid.extend(("FIAT_WEBHOOK_SECRET", "SETTLEMENT_INGRESS_SECRET"))
    return {"schemaVersion": 1, "scope": "startup-secret-presence-only",
            "ready": not missing and not invalid, "requiredCount": len(required),
            "missing": sorted(set(missing)), "invalid": sorted(set(invalid))}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--describe", action="store_true",
                        help="describe the source contract without reading environment values")
    args = parser.parse_args()
    if args.describe:
        print(json.dumps(describe(), sort_keys=True))
        return 0
    try:
        result = check(os.environ)
    except Exception:
        # Do not serialize exception messages or any environment contents.
        print('{"schemaVersion":1,"scope":"startup-secret-presence-only","ready":false,"error":"preflight-unavailable"}')
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0 if result["ready"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
