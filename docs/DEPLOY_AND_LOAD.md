# Deploy + Load (G4/G6) — operator runbook

This wires the gateway's remaining hyperscale-readiness gaps: continuous
delivery to production and a repeatable load test. Both are **prepared but
require owner-held credentials** to run.

## 1. Continuous delivery (`deploy.yml`)

`.github/workflows/deploy.yml` builds the Docker image on every push to `main`
(and on manual dispatch), ships it to the production VPS, restarts the
container, and smoke-checks `/health` + `/billing/balance`.

### Secrets to configure (Repo → Settings → Environments → `production`)

| Secret | Purpose |
|--------|---------|
| `PROD_SSH_HOST` | Production host (e.g. `161.35.222.194`) |
| `PROD_SSH_USER` | SSH user (e.g. `root` or a deploy user) |
| `PROD_SSH_KEY` | Private SSH key authorized on the host |

### Host prerequisites (one-time)

```bash
mkdir -p /etc/conxian
cat > /etc/conxian/gateway.env <<'EOF'
LIGHTNING_INVOICE_CREATE_URL=https://lnbits.example/api/v1/payments
LIGHTNING_INVOICE_CHECK_URL=https://lnbits.example/api/v1/payments
LIGHTNING_INVOICE_API_KEY=<invoice key>
FUSION_JWT_SECRET=<random 32+ bytes>
RUST_LOG=info
EOF
```

Rollback: the workflow tags the previous image `conxian-gateway:previous`.
`docker run ... conxian-gateway:previous` to revert.

> Note: this is a single-host pattern. Before horizontal scaling, move to a
> registry (GHCR) + orchestrator and swap `InMemoryCreditStore` for Neon/Postgres.

## 2. Load test (`scripts/load_test.py`)

Stdlib-only; no deps.

```bash
# Baseline
python3 scripts/load_test.py --url http://localhost:3000 --concurrency 10 --duration 30

# 10x ramp (the hyperscale gate)
python3 scripts/load_test.py --url http://localhost:3000 --ramp --base 5 --duration 60
```

Passing bar: at 10x concurrency, **error rate < 0.1%** and **p95 < 200 ms**
(both are enforced by the script; non-zero exit code = gate failed).

## 3. Where this lands on the readiness tracker

- **G4 (elastic + observable)**: `deploy.yml` + smoke checks. Remaining: real
  cloud autoscaling, metrics/alerting (Prometheus `/metrics` already exists).
- **G6 (hyperscale test)**: `scripts/load_test.py`. Run it against a live
  deployment after G1.
