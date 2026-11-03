# Deploy + Load (G4/G6) — operator runbook

The gateway has **two deployment modes** (see `docs/DEPLOY_MODES.md`):

- **Managed** — Render service `conxian-gateway` (`srv-dbji535g1s2s73fqgrtg`),
  branch `main`, served at `https://gateway.conxian.org`. This is the only public
  production instance and the CI deploy target (`.github/workflows/deploy.yml`).
- **Sovereign reference** — DO droplet `conxian-gateway-node` (161.35.222.194,
  systemd, bare binary). A self-managed-lane demonstration host, not a production
  target. Deployed only by `scripts/deploy-sovereign.sh`.

Both serve billing (`/api/v1/billing/quote`, `/balance`) with fail-closed
`/billing/verify` and `/billing/subscribe`.

## 0. Managed deploy (CI, current production)

Push to `main` (or `workflow_dispatch`) triggers `.github/workflows/deploy.yml`,
which asks Render to redeploy the service, waits for `live`, then smoke-checks
`gateway.conxian.org`. Requires the `RENDER_API_KEY` secret in the `production`
environment. No SSH, no image build in CI.

## 1. Sovereign reference deploy (systemd, operator only)

The droplet runs the gateway as a bare binary under systemd (no Docker on the
host). `scripts/deploy-sovereign.sh` builds the released source, ships it, swaps
the binary atomically and restarts:

```bash
scripts/deploy-sovereign.sh --host 161.35.222.194 --ref origin/main
```

- Unit: `/etc/systemd/system/conxian-gateway.service` → `ExecStart=/usr/local/bin/conxian-gateway`,
  `EnvironmentFile=/etc/conxian-gateway.env`, `WorkingDirectory=/srv/conxian/gateway`, listens `:3000`.
- Rollback: the script keeps `/usr/local/bin/conxian-gateway.old.<ts>`; move it back and restart.
- Smoke: `/api/v1/health`, `/api/v1/version`, `/api/v1/billing/quote` (POST `{"plan":"indie","term":"monthly"}`), `/api/v1/billing/balance`.
- The workspace binary name is `gateway`; it is renamed to `conxian-gateway` on the host.

> **Never point a public hostname at a sovereign reference host.** If a client
> wants Conxian to run it, it becomes a managed deployment (add a Render service
> or equivalent, then declare it in `docs/operations/deployment-inventory.json`).

## 2. Managed deploy secrets

`.github/workflows/deploy.yml` triggers the Render redeploy and smoke-checks the
public surface. Configure (Repo → Settings → Environments → `production`):

| Secret | Purpose |
|--------|---------|
| `RENDER_API_KEY` | Render API token used to trigger + poll the managed deploy |

No SSH secrets and no image build in CI: Render builds from the repo. The
managed service is `srv-dbji535g1s2s73fqgrtg`; its env vars live in the Render
dashboard (see `docs/DEPLOY_MODES.md`).

> Note: this is a single-host pattern. Before horizontal scaling, swap
> `InMemoryCreditStore` for Neon/Postgres and move to a registry + orchestrator.

## 3. Load test (`scripts/load_test.py`)

Stdlib-only; no deps.

```bash
# Baseline
python3 scripts/load_test.py --url http://localhost:3000 --concurrency 10 --duration 30

# 10x ramp (the hyperscale gate)
python3 scripts/load_test.py --url http://localhost:3000 --ramp --base 5 --duration 60
```

Passing bar: at 10x concurrency, **error rate < 0.1%** and **p95 < 200 ms**
(both are enforced by the script; non-zero exit code = gate failed).

## 4. Where this lands on the readiness tracker

- **G4 (elastic + observable)**: managed `deploy.yml` + smoke checks. Remaining: real
  cloud autoscaling, metrics/alerting (Prometheus `/metrics` already exists).
- **G6 (hyperscale test)**: `scripts/load_test.py`. Run it against a live
  deployment after G1.

## Deploy triggers

`deploy.yml` runs on push to `main` (paths: internal/cmd/pkg/Dockerfile/Cargo) and on manual dispatch. It is intentionally NOT triggered by dev/staged PRs.
