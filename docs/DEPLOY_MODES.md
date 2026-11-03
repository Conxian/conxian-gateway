# Gateway deployment modes — managed vs sovereign

The gateway is one artifact (`conxian-gateway`) with **two supported operating
models**. Both are legitimate; they must be labelled so a running instance is
never an unowned duplicate.

| | Managed | Sovereign |
|---|---|---|
| Who operates it | Conxian-Labs | The client (self-managed), or Conxian-Labs under a managed-deployment contract (on-site, cloud, co-located) |
| Provisioned by | Render service `conxian-gateway` (`srv-dbji535g1s2s73fqgrtg`), autoDeploy from `main` | `conxian-cli init` / `docker compose up` from the client's own host, or the operator script `scripts/deploy-sovereign.sh` |
| Public host | `gateway.conxian.org` (Cloudflare Worker `conxian-proxy` → Render) | The client's own hostname; the org reference instance is reachable only on its private `:3000` |
| TLS / WAF | Cloudflare (proxied) | Client's perimeter |
| Secrets | Render env vars | Client-held (`EnvironmentFile`, KMS/HSM) |
| CD | Render autoDeploy on `main`; `deploy.yml` triggers + smoke-checks the managed host | Client pipeline; `scripts/deploy-sovereign.sh` is the reference systemd path |
| Cost model | Conxian operates; billed as a managed tier | Client's own infrastructure cost |

## Canonical rule

> Every running gateway is either **managed** or **sovereign**. A host that is
> neither is an unowned duplicate and must be reconciled: labelled, given an
> owner, or retired.

The org reference instances:

- **Managed** — Render `conxian-gateway` → `https://gateway.conxian.org`. This is
  the only instance with a public branded hostname. CI deploys and smoke-checks
  **this** one.
- **Sovereign reference** — DigitalOcean droplet `conxian-gateway-node`
  (`161.35.222.194`, systemd `conxian-gateway`, `:3000`, no public DNS/TLS).
  This demonstrates the self-managed lane. It is **not** a production target and
  is **not** deployed by CI. Do not point a public hostname at it; to promote it
  to a managed deployment, follow the managed row above.

## Why this matters

Two unlabelled instances of the same service create a split-brain: a client or
operator cannot tell which host is authoritative, deployments land on the wrong
one, and cost/ownership is untracked. See the preflight gate in
`scripts/verify_deploy_preflight.py`, which fails when a provider service exists
that no deployment-mode record claims.

## Sovereign reference host (operator only)

```bash
# Build from the released source, ship, restart (see scripts/deploy-sovereign.sh)
scripts/deploy-sovereign.sh --host 161.35.222.194 --ref origin/main
```

The droplet runs the bare binary under systemd:

- Unit `conxian-gateway.service` → `ExecStart=/usr/local/bin/conxian-gateway`,
  `EnvironmentFile=/etc/conxian-gateway.env`, `WorkingDirectory=/srv/conxian/gateway`.
- Listen `:3000`. Rollback keeps the previous binary at
  `/usr/local/bin/conxian-gateway.old.<ts>`.

## Managed host (CI)

`.github/workflows/deploy.yml` triggers the Render deploy for
`srv-dbji535g1s2s73fqgrtg` after a `main` promotion and smoke-checks
`https://gateway.conxian.org/api/v1/health` + `/api/v1/billing/balance`. It
requires the `RENDER_DEPLOY_HOOK_URL` secret in the `production` environment.
