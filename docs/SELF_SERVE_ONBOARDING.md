# Self-serve onboarding API (Gateway #713)

Implements design §5.1–§5.3 of
`conxian-business/docs/operations/SELF_SERVE_CLOUD_PROVISIONING_2026_11_05.md`.

## What this is

A small, auditable control plane in front of the existing provisioning primitives.
It sequences the durable steps behind one authenticated call and one receipt so a
new tenant reaches `active` without an owner-held secret and without a human in
the loop. It does not replace the gateway/Neon/Cloudflare primitives and it is not
a second billing engine — key minting reuses the Merchant-of-Record `KeyStore`
(`mor_webhook`), so a key issued here authenticates through the same
managed-auth path as a webhook-minted key.

## Surface

| Method | Path | Auth | Purpose |
|---|---|---|---|
| `POST` | `/api/v1/onboard` | onboarding token | Provision a tenant; returns a receipt. Idempotent on `idempotency_key`. |
| `GET` | `/api/v1/onboard/{tenant_id}` | onboarding token | Current onboarding state + per-step detail. |
| `GET` | `/api/v1/tenants/{tenant_id}/limits` | managed key | Published quota / trial / always-free data. |
| `POST` | `/api/v1/tenants/{tenant_id}/keys` | managed key | Rotate the active key (new plaintext returned once). |
| `DELETE` | `/api/v1/tenants/{tenant_id}/keys/{key_id}` | managed key | Revoke the active key. |
| `POST` | `/api/v1/tenants/{tenant_id}/deploy` | onboarding token | Resume/re-run provisioning. |
| `GET` | `/api/v1/tenants/{tenant_id}/usage` | managed key | Quota for the current period. |

The gate is split by what the caller can hold:

- **Provisioning operations** (`onboard`, `status`, `deploy`) require the no-card
  **onboarding token** (`ONBOARDING_TOKEN`). They must be reachable *without* a
  tenant key, because a tenant that failed at the key step has none and would
  otherwise be unable to read its state or resume.
- **Tenant self-service** (`limits`, `usage`, `rotate`, `revoke`) requires the
  **managed tenant key**, and enforces that the authenticated tenant matches the
  path tenant (403 on mismatch). Verifying a minted key against `/limits` is the
  self-verification step: a 200 proves the key authenticates through the same
  managed-auth path as any other key.

## Design rules enforced

- **Idempotent** — a client-supplied `idempotency_key` makes a retry return the
  existing receipt; no second tenant is provisioned.
- **Resumable** — each step (`entitlement`, `endpoint`, `key`, `activation`)
  records its own state; a failure leaves the record resumable and
  `POST /deploy` re-runs from the first step that is not `done`.
- **Fail-closed on entitlement** — the entitlement row is written before a key is
  minted, so a crash between the two can never yield a working-but-unmetered
  endpoint. A key-store error marks the step `failed` and returns 503; it does not
  activate the tenant.
- **Receipt is the contract** — the receipt names the endpoint, the key id (a
  hash prefix, never the secret), the state and the steps. The plaintext key is
  returned exactly once, on first provision or rotation; it is never stored.

## State machine

```
requested ─► provisioning ─► active
                  │
                  └──► failed (resumable via POST /deploy)
active ─► terminated  (key revoked)
```

## Configuration

- `ONBOARDING_TOKEN` — the no-card evaluation credential. When unset the
  onboarding routes are **not mounted** (fail closed).
- `MOR_WEBHOOK_DATABASE_URL` — the durable Neon/Postgres store. Required; without
  it the routes are not mounted, so an entitlement is never granted from volatile
  state.
- `GATEWAY_PUBLIC_BASE_URL` — the public base the receipt derives the tenant
  endpoint from. Defaults to `https://api.conxian-labs.com`.

## Storage

- `migrations/0002_onboarding_tenants.sql` — the `onboarding_tenants` table. The
  `idempotency_key` is `UNIQUE`, so the database enforces idempotency even under
  concurrency. The per-step state is JSONB.
- Keys live in `managed_api_keys` (hash only), minted through the shared
  `KeyStore`.

## Client

The single onboarding command lives in the CLI:

```bash
export ONBOARDING_TOKEN=...            # never a flag; read from the environment only
conxian-cli onboard --gateway https://api.conxian-labs.com --tier sandbox --write-env .env.tenant
```

It issues the one `POST /v1/onboard` call, then **verifies the minted key** by
calling `GET /v1/tenants/{id}/limits` with it — a returned key is a working key,
not an untested string. `--write-env <path>` writes the key to a `0600` file
(create-new; an existing file is never overwritten). On an idempotent replay no
new key is minted and the command says so.

## Tests

- `internal/api/src/onboarding.rs` — unit tests for idempotency, resumability,
  fail-closed minting, rotation, revocation, the limits catalog, and the
  provisioning-vs-tenant auth split.
- `cmd/conxian-cli/src/main.rs` — unit tests for the onboard command: URL/tenant
  validation without network, an end-to-end provision+verify against a stand-in
  server, and fail-closed behavior when the minted key is rejected.
- `internal/api/tests/onboarding_pg.rs` — real PostgreSQL conformance for the
  durable store (round-trip, upsert, idempotency uniqueness), run by
  `.github/workflows/onboarding-conformance.yml`.

## Not claimed

Metering is not invented. `GET /usage` reports the published quota with
`metered: false` until the monthly meter is wired to the billing ledger (#1591).
A tenant reaching `active` in a test is not a production acceptance; that requires
runtime-bound evidence (see business #1724).
