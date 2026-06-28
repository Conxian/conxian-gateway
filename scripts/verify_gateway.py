#!/usr/bin/env python3
"""Conxian Gateway — Audit & Verification Script (G-16).

Performs end-to-end health and compliance checks against a running
Conxian Gateway instance.  Designed for CI/CD and operational monitoring.

Usage:
  python3 scripts/verify_gateway.py [--host HOST] [--port PORT] [--token TOKEN]

Environment variables:
  CONXIAN_HOST   — Gateway host (default: 127.0.0.1)
  CONXIAN_PORT   — Gateway port (default: 8080)
  CONXIAN_TOKEN  — Bearer token for authenticated endpoints
"""

from __future__ import annotations

import argparse
import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any

# ── Check helpers ──────────────────────────────────────────────


@dataclass
class CheckResult:
    name: str
    passed: bool
    detail: str = ""
    duration_ms: float = 0.0


@dataclass
class AuditReport:
    results: list[CheckResult] = field(default_factory=list)
    _start: float = field(default_factory=time.monotonic)

    def add(self, name: str, passed: bool, detail: str = "", duration_ms: float = 0.0) -> None:
        self.results.append(CheckResult(name, passed, detail, duration_ms))

    def summary(self) -> tuple[int, int]:
        passed = sum(1 for r in self.results if r.passed)
        failed = len(self.results) - passed
        return passed, failed


# ── HTTP helpers ────────────────────────────────────────────────


class GatewayClient:
    def __init__(self, host: str, port: int, token: str) -> None:
        self.base = f"http://{host}:{port}"
        self.token = token

    def _request(self, path: str, method: str = "GET", body: dict | None = None) -> tuple[int, dict[str, Any], float]:
        url = f"{self.base}{path}"
        data = json.dumps(body).encode() if body else None
        req = urllib.request.Request(url, data=data, method=method)
        req.add_header("Content-Type", "application/json")
        if self.token:
            req.add_header("Authorization", f"Bearer {self.token}")
        start = time.monotonic()
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                elapsed = (time.monotonic() - start) * 1000
                payload = json.loads(resp.read().decode())
                return resp.status, payload, elapsed
        except urllib.error.HTTPError as exc:
            elapsed = (time.monotonic() - start) * 1000
            try:
                body = json.loads(exc.read().decode())
            except Exception:
                body = {"error": str(exc)}
            return exc.code, body, elapsed
        except Exception as exc:
            elapsed = (time.monotonic() - start) * 1000
            return 0, {"error": str(exc)}, elapsed


# ── Checks ─────────────────────────────────────────────────────


def check_health(client: GatewayClient) -> CheckResult:
    status, body, dur = client._request("/health")
    ok = status == 200 and body.get("status") == "ok"
    return CheckResult("health_endpoint", ok, str(body), dur)


def check_metrics(client: GatewayClient) -> CheckResult:
    status, body, dur = client._request("/metrics")
    ok = status == 200 and isinstance(body, str)
    has_keys = ok and all(
        k in body for k in ("conxian_requests_total", "conxian_health_requests_total")
    )
    return CheckResult("prometheus_metrics", has_keys, "Prometheus format OK" if has_keys else "bad format", dur)


def check_version(client: GatewayClient) -> CheckResult:
    status, body, dur = client._request("/api/v1/version")
    ok = status == 200 and isinstance(body, str)
    return CheckResult("api_version", ok, str(body), dur)


def check_state_authenticated(client: GatewayClient) -> CheckResult:
    status, body, dur = client._request("/api/v1/state")
    ok = status == 200 and "height" in body
    return CheckResult("state_authenticated", ok, "authenticated" if ok else f"status={status}", dur)


def check_unauthorized(client: GatewayClient) -> CheckResult:
    """Verify that missing auth token returns 401."""
    saved = client.token
    client.token = ""
    status, body, dur = client._request("/api/v1/state")
    client.token = saved
    ok = status == 401
    return CheckResult("auth_enforced", ok, "401 returned" if ok else f"got {status}", dur)


def check_dlc_bond(client: GatewayClient) -> CheckResult:
    body_payload = {
        "bond_id": "verify-py-test",
        "amount_btc": 100_000,
        "interest_rate": 0.03,
        "maturity_date": 1750000000,
        "sovereign_alignment": True,
    }
    status, body, dur = client._request("/api/v1/dlc/bond", method="POST", body=body_payload)
    ok = status == 200 and "bond_id" in body
    return CheckResult("dlc_bond_creation", ok, str(body), dur)


def check_musig2(client: GatewayClient) -> CheckResult:
    body_payload = {
        "pubkeys": [
            "02aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            "03aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
        ]
    }
    status, body, dur = client._request("/api/v1/musig2/aggregate-keys", method="POST", body=body_payload)
    ok = status == 200 and "aggregated_pubkey" in body
    return CheckResult("musig2_aggregation", ok, str(body), dur)


ALL_CHECKS = [
    check_health,
    check_metrics,
    check_version,
    check_state_authenticated,
    check_unauthorized,
    check_dlc_bond,
    check_musig2,
]


# ── Main ───────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(description="Conxian Gateway Audit Script")
    parser.add_argument("--host", default=os.environ.get("CONXIAN_HOST", "127.0.0.1"))
    parser.add_argument("--port", type=int, default=int(os.environ.get("CONXIAN_PORT", "8080")))
    parser.add_argument("--token", default=os.environ.get("CONXIAN_TOKEN", "test-token"))
    parser.add_argument("--json", action="store_true", help="Output JSON report")
    args = parser.parse_args()

    client = GatewayClient(args.host, args.port, args.token)
    report = AuditReport()

    for check_fn in ALL_CHECKS:
        try:
            result = check_fn(client)
        except Exception as exc:
            result = CheckResult(check_fn.__name__, False, str(exc))
        report.add(result.name, result.passed, result.detail, result.duration_ms)
        icon = "✓" if result.passed else "✗"
        if not args.json:
            print(f"  {icon} {result.name:<30s} ({result.duration_ms:6.1f} ms)  {result.detail}")

    passed, failed = report.summary()
    total = passed + failed

    if args.json:
        print(json.dumps({
            "passed": passed,
            "failed": failed,
            "total": total,
            "results": [
                {"name": r.name, "passed": r.passed, "detail": r.detail, "duration_ms": r.duration_ms}
                for r in report.results
            ],
        }, indent=2))
    else:
        print(f"\n{'✓ ALL CHECKS PASSED' if failed == 0 else '✗ SOME CHECKS FAILED'}")
        print(f"  {passed}/{total} checks passed, {failed} failed")

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
