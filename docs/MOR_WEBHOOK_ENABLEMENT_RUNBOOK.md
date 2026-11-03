# MoR billing webhook — production enablement runbook

Operator procedure to enable the Merchant-of-Record billing webhook on the
managed gateway (`srv-dbji535g1s2s73fqgrtg` → `https://gateway.conxian.org`).

The webhook route is mounted only when `MOR_PROVIDER`, `MOR_WEBHOOK_SECRET` and
`MOR_WEBHOOK_DATABASE_URL` are all set; otherwise it is not advertised (a POST
returns 404). Enabling is therefore a deliberate, reversible configuration
change, not a code change. See `MANAGED_SAAS_BILLING_WEBHOOK.md` for the design.

## Preconditions

1. The promotion `dev -> staged -> main` has landed on `main` (the deploy gate).
2. The operator holds the MoR signing secret for the target destination and a
   Neon **write** role for the Gateway project (`noisy-cloud-41146057`).
3. The managed key store migration is applied (step 1) — the store fails closed
   on a missing table, so an unmigrated database would reject every delivery.

## Step 0 — Lemon Squeezy store, product and checkout

The selected provider is **Lemon Squeezy** (`MOR_PROVIDER=lemonsqueezy`). Set the
store up so it produces exactly what the handler reads:

1. **Store** — create the store (name `Conxian Labs`, a unique slug, and the
   country of the registered legal entity).
2. **Product + variant** — one subscription product whose variant name is the
   tier (`managed`, `indie`, or `business`). The handler reads the tier from
   `data.attributes.variant_name`.
3. **Checkout custom data** — the buyer's checkout link must carry
   `checkout[custom][tenant_id]=<tenant_id>` (via the checkout URL query or the
   checkout API `checkout_data.custom`). The handler reads
   `meta.custom_data.tenant_id` to decide which tenant to mint a key for; a
   delivery with no tenant id is rejected (fail closed).
4. **Webhook destination** — add a destination pointing at
   `https://gateway.conxian.org/api/v1/billing/mor/webhook`, subscribed to
   `subscription_created`, `subscription_updated`, `subscription_cancelled`,
   `subscription_expired`, `subscription_payment_failed`, `subscription_resumed`,
   `subscription_paused`. Copy the destination **signing secret** for
   `MOR_WEBHOOK_SECRET`.

## Step 1 — apply the key-store migration

The gateway does not run migrations at boot. Apply
`migrations/0001_managed_api_keys.sql` to the Gateway Neon project once, on the
production branch:

```bash
# Against the Gateway project's production branch, as a write role.
psql "$GATEWAY_NEON_WRITE_URL" -f migrations/0001_managed_api_keys.sql
```

Creates `managed_api_keys` (key hashes only) and `billing_webhook_events`
(idempotency ledger). Re-running is safe (idempotent DDL).

## Step 2 — set the service environment

Set these on the Render service `srv-dbji535g1s2s73fqgrtg` (Dashboard → service →
Environment, or the Render API). Values are operator-managed and must be entered
through the protected channel, never committed:

| Variable | Value |
|----------|-------|
| `MOR_PROVIDER` | `lemonsqueezy` (selected) |
| `MOR_WEBHOOK_SECRET` | the MoR webhook signing secret for this destination |
| `MOR_WEBHOOK_DATABASE_URL` | Neon connection string for the key store (same database as step 1) |

Optionally set `MANAGED_API_KEYS` for any out-of-band keys; webhook-minted keys
do not need it (they resolve from the durable store by hash).

## Step 3 — deploy

The `Deploy Gateway` workflow triggers a Render deploy on a push to `main` that
touches `internal/**`, `cmd/**`, `pkg/**`, `Dockerfile`, `Cargo.toml` or
`Cargo.lock`; it can also be run manually (`workflow_dispatch`). After the env
vars are saved, redeploy so the process reads them.

## Step 4 — verify (read-only)

```bash
# Gateway is up.
curl -s -o /dev/null -w '%{http_code}\n' https://gateway.conxian.org/api/v1/health      # expect 200

# Webhook route is now advertised (405 = mounted, GET not allowed).
curl -s -o /dev/null -w '%{http_code}\n' https://gateway.conxian.org/api/v1/billing/mor/webhook  # expect 405 (was 404)

# An unsigned delivery is rejected without state change.
curl -s -o /dev/null -w '%{http_code}\n' -X POST https://gateway.conxian.org/api/v1/billing/mor/webhook \
  -H 'content-type: application/json' -d '{}'                                            # expect 4xx
```

A correctly signed `subscription_created` delivery mints `cxn_agent_<32-hex>`
and the key authenticates `GET /api/v1/agent/capabilities` (200) against the
durable store without an env reload.

## Rollback

Unset `MOR_PROVIDER` (or all three `MOR_*` vars) and redeploy. The route stops
being advertised; existing rows in `managed_api_keys` are retained and can be
revoked with a `subscription_cancelled` delivery or a direct `status='revoked'`
update.

## Boundaries

This runbook changes configuration only. It performs no live payment and no
value-bearing transaction. Enabling the webhook makes a real MoR payment mint a
usable entitlement; the end-to-end payer journey (payment → entitlement →
metering → support) remains a separate acceptance gate under Business
#1343 / #1591 and must be evidenced by a real operator/customer journey, not an
agent probe.
