# Gateway Redeploy Runbook — restore `/health` and `/billing/*`

Status: operational runbook · 2026-11-01 · Owner / ops execution
Related: `#466` (deployment qualification)

## 1. Symptom this fixes

The public gateway serves `/metrics` (200) but returns **404** for `/health`,
`/billing/subscribe`, `/billing/verify`, `/state`, and `/capabilities`. The billing and health
routes were introduced in **#488 (2026-10-09)**; the running binary predates that commit.
Result: no client can subscribe or verify payment. This runbook redeploys the current build.

## 2. Preconditions

- SSH access to the gateway host (owner-held).
- The current `dev` commit checked out and built.
- Runtime configuration present and current:
  - `LIGHTNING_INVOICE_CHECK_URL` (fail-closed verifier — #400)
  - LNbits API base + admin key, tenant key store, `DATABASE_URL` if persistent
  - `BIND_ADDR` / `PORT` matching the intended listener
- A recorded rollback artifact: the currently running image/binary tag.

## 3. Diagnose (record before changing anything)

```bash
HOST=${GATEWAY_HOST:?set GATEWAY_HOST}
curl -s -o /dev/null -w 'metrics=%{http_code}\n' "http://$HOST:3000/metrics"
curl -s -o /dev/null -w 'health=%{http_code}\n'  "http://$HOST:3000/health"
curl -s -o /dev/null -w 'subscribe=%{http_code}\n' -X POST "http://$HOST:3000/billing/subscribe"
# expect: metrics=200, health=404, subscribe=404  (stale build)
```

Record the running build identity (image digest or binary sha256) to compare after deploy.

## 4. Build the current commit

```bash
git -C <repo> fetch origin dev && git -C <repo> checkout origin/dev
cargo build --locked --release -p conxian-gateway   # or the project's packaged build target
```

## 5. Deploy

Use the project's existing deployment mechanism (docker compose baseline or `conxian-cli start`).
Restart with the new image/binary. Keep the previous artifact for rollback.

## 6. Verify (all must pass before declaring live)

```bash
curl -s -o /dev/null -w 'health=%{http_code}\n'  "http://$HOST:3000/health"                 # 200
curl -s -o /dev/null -w 'subscribe=%{http_code}\n' -X POST "http://$HOST:3000/billing/subscribe"  # 200 (invoice)
curl -s -X POST "http://$HOST:3000/billing/verify" -d '{}' -H 'content-type: application/json' \
  -o /dev/null -w 'verify=%{http_code}\n'                                                  # 4xx fail-closed (NOT 2xx without a real invoice)
curl -s "http://$HOST:3000/metrics" | grep -q conxian_requests_total && echo metrics-ok
```

A `verify` that returns success without a real Lightning invoice is a **fail-open regression** —
stop and roll back.

## 7. Rollback

Redeploy the recorded previous image/binary and re-run section 3 to confirm the prior state.

## 8. Evidence to attach to #466

- Before/after HTTP codes for `/health`, `/billing/subscribe`, `/billing/verify`.
- Build commit SHA deployed.
- Deployment timestamp and operator.
- Whether `/billing/verify` fails closed.

## 9. After redeploy

Only once section 6 passes is "live" an accurate public claim. Then publish the x402 listing
(see the agent-attraction plan) so an autonomous agent can discover and pay per call.
