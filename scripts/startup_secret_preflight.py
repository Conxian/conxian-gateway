#!/usr/bin/env python3
"""Check startup-secret presence locally without exporting values or fingerprints.

Run only within the approved operator/secret boundary. This performs no network
or provider calls and does not authorize deployment, signing or settlement.
"""
import json
import os
from typing import Mapping

REQUIRED = ("API_TOKEN", "FIAT_WEBHOOK_SECRET", "SETTLEMENT_INGRESS_SECRET",
            "HMAC_SECRET", "OFFLINE_QUEUE_SECRET")
CONDITIONAL = {"RAMP_MODE": "RAMP_API_KEY", "ALCHEMY_PAY_MODE": "ALCHEMY_PAY_APP_ID",
               "INFOBIP_MODE": "INFOBIP_API_KEY"}
ENABLED_MODES = frozenset(("active", "shadow", "true", "1"))
LEGACY_API_TOKEN = "CONXIAN_GATEWAY_AUTH_TOKEN"


def check(environment: Mapping[str, str]) -> dict:
    required = list(REQUIRED)
    for mode, key in CONDITIONAL.items():
        if environment.get(mode, "").strip().lower() in ENABLED_MODES:
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
    if left and right and left == right:
        invalid.extend(("FIAT_WEBHOOK_SECRET", "SETTLEMENT_INGRESS_SECRET"))
    return {"schemaVersion": 1, "scope": "startup-secret-presence-only",
            "ready": not missing and not invalid, "requiredCount": len(required),
            "missing": sorted(set(missing)), "invalid": sorted(set(invalid))}


def main() -> int:
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
