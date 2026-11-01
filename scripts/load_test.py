#!/usr/bin/env python3
"""Load-test harness for the Conxian Gateway (G6 hyperscale gate).

Stdlib-only. Drives `/health` (GET) and `/billing/quote` (POST) concurrently and
reports requests/sec, error rate, and p50/p95/p99 latency. `--ramp` scales
concurrency 1x -> 10x in equal steps to prove headroom before saturation.

Usage:
    python3 scripts/load_test.py --url http://localhost:3000 --concurrency 10 --duration 30
    python3 scripts/load_test.py --url http://localhost:3000 --ramp --base 5 --duration 60

The "hyperscale-ready" definition: at 10x concurrency, error rate stays < 0.1%
and p95 latency stays within the SLO (default 200 ms).
"""
import argparse
import concurrent.futures
import json
import time
import urllib.error
import urllib.request

QUOTE_BODY = json.dumps({"plan": "indie", "term": "annual"}).encode()


def hit_health(base: str) -> tuple[float, bool]:
    start = time.perf_counter()
    try:
        with urllib.request.urlopen(f"{base}/health", timeout=10) as r:
            ok = r.status == 200
    except Exception:
        ok = False
    return (time.perf_counter() - start) * 1000.0, ok


def hit_quote(base: str) -> tuple[float, bool]:
    start = time.perf_counter()
    req = urllib.request.Request(
        f"{base}/billing/quote",
        data=QUOTE_BODY,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            ok = r.status == 200
    except Exception:
        ok = False
    return (time.perf_counter() - start) * 1000.0, ok


def run_phase(base: str, concurrency: int, duration: float) -> dict:
    stop = time.monotonic() + duration
    latencies: list[float] = []
    errors = 0
    calls = 0

    def worker():
        nonlocal errors, calls
        while time.monotonic() < stop:
            if calls % 5 == 0:
                latency, ok = hit_quote(base)
            else:
                latency, ok = hit_health(base)
            calls += 1
            latencies.append(latency)
            if not ok:
                errors += 1

    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
        futures = [ex.submit(worker) for _ in range(concurrency)]
        for fut in concurrent.futures.as_completed(futures):
            fut.result()

    elapsed = duration
    rps = calls / elapsed if elapsed else 0.0
    err_rate = (errors / calls * 100.0) if calls else 0.0

    def pct(q):
        if not latencies:
            return 0.0
        s = sorted(latencies)
        idx = min(len(s) - 1, int(round((q / 100.0) * (len(s) - 1))))
        return s[idx]

    return {
        "concurrency": concurrency,
        "calls": calls,
        "rps": round(rps, 1),
        "errors": errors,
        "error_pct": round(err_rate, 4),
        "p50_ms": round(pct(50), 1),
        "p95_ms": round(pct(95), 1),
        "p99_ms": round(pct(99), 1),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://localhost:3000")
    ap.add_argument("--concurrency", type=int, default=10)
    ap.add_argument("--duration", type=float, default=30.0)
    ap.add_argument("--ramp", action="store_true", help="ramp concurrency 1x -> 10x")
    ap.add_argument("--base", type=int, default=5, help="base concurrency when ramping")
    ap.add_argument("--slo-ms", type=float, default=200.0)
    args = ap.parse_args()

    if args.ramp:
        steps = [args.base * m for m in (1, 2, 4, 6, 8, 10)]
    else:
        steps = [args.concurrency]

    print(f"target={args.url} duration={args.duration}s slo={args.slo_ms}ms")
    print(f"{'conc':>4} {'calls':>7} {'rps':>8} {'err%':>7} {'p50':>8} {'p95':>8} {'p99':>8}")
    failed = False
    for concurrency in steps:
        res = run_phase(args.url, concurrency, args.duration)
        print(
            f"{res['concurrency']:>4} {res['calls']:>7} {res['rps']:>8} "
            f"{res['error_pct']:>7} {res['p50_ms']:>8} {res['p95_ms']:>8} {res['p99_ms']:>8}"
        )
        if res["error_pct"] >= 0.1:
            print(f"  !! error rate {res['error_pct']}% >= 0.1% (hyperscale gate) ")
            failed = True
        if res["p95_ms"] > args.slo_ms:
            print(f"  !! p95 {res['p95_ms']}ms exceeds SLO {args.slo_ms}ms ")
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
