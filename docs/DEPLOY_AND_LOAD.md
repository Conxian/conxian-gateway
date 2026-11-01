# Deploy + Load (G4/G6) — operator runbook

Production status (2026-11-01): the gateway is **live** in two environments —
Render (`conxian-gateway`, branch `staged`, Docker) and the DigitalOcean droplet
`conxian-gateway-node` (161.35.222.194, systemd service, bare binary). Both serve
billing (`/api/v1/billing/quote`, `/balance`) with fail-closed `/billing/verify` and
`/billing/subscribe`. The droplet deploy is the systemd method below (§0); the
`deploy.yml` Docker path (§1) is the CI alternative for horizontal scaling.

## 0. Production droplet deploy (systemd, current)

The droplet runs the gateway as a bare binary under systemd (no Docker on the
host). Build from `staged`, upload, replace, restart:

```bash
# on a build host with Rust 1.98.1
git clone https://github.com/Conxian/conxian-gateway && cd conxian-gateway
git checkout origin/staged
cargo build --release --locked --bin gateway   # -> target/release/gateway

# upload + deploy to the droplet
scp target/release/gateway root@161.35.222.194:/tmp/conxian-gateway.new
ssh root@161.35.222.194 '
  cp -a /usr/local/bin/conxian-gateway /usr/local/bin/conxian-gateway.old.$(date +%s)
  chmod 755 /tmp/conxian-gateway.new
  mv /tmp/conxian-gateway.new /usr/local/bin/conxian-gateway
  systemctl restart conxian-gateway
'
```

- Unit: `/etc/systemd/system/conxian-gateway.service` → `ExecStart=/usr/local/bin/conxian-gateway`,
  `EnvironmentFile=/etc/conxian-gateway.env`, `WorkingDirectory=/srv/conxian/gateway`, listens `:3000`.
- Rollback: `mv /usr/local/bin/conxian-gateway.old.<ts> /usr/local/bin/conxian-gateway && systemctl restart conxian-gateway`.
- Smoke: `/api/v1/health`, `/api/v1/version`, `/api/v1/billing/quote` (POST `{"plan":"indie","term":"monthly"}`), `/api/v1/billing/balance`.
- The workspace binary name is `gateway`; it is renamed to `conxian-gateway` on the host.

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

## Deploy triggers

`deploy.yml` runs on push to `main` (paths: internal/cmd/pkg/Dockerfile/Cargo) and on manual dispatch. It is intentionally NOT triggered by dev/staged PRs.
