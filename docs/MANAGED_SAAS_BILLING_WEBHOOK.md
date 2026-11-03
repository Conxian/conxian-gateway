# Managed SaaS Gateway — Billing Webhook & Key Lifecycle

The Managed tier is provisioned automatically when the Merchant-of-Record
(Lemon Squeezy or Paddle) emits a subscription event. The webhook handler is
the only component that **mints and rotates** managed API keys; the Gateway
runtime never stores plaintext keys or LLM/agent payloads (BYOK / thin
orchestrator invariant).

## Implementation

`internal/api/src/mor_webhook.rs` (provider-neutral verification, event parsing,
and key lifecycle) + `internal/api/src/mor_webhook_store.rs` (`sqlx`/Postgres
key store and idempotency ledger). The route is mounted at
`POST /api/v1/billing/mor/webhook` only when `MOR_PROVIDER`, `MOR_WEBHOOK_SECRET`
and `MOR_WEBHOOK_DATABASE_URL` are all configured; otherwise it is not
advertised. Signature verification happens over the raw request body before any
entitlement mutation, and every failure path is a 4xx that changes no state.

## Flow

```
MoR (Lemon Squeezy / Paddle)
  └─ webhook (HMAC-signed over the raw body)
       └─ handler
            ├─ verify signature (fail closed) + timestamp window (Paddle)
            ├─ parse provider payload → normalized event
            ├─ record billing_webhook_events (idempotency; replay = no-op)
            ├─ subscription_created  → mint key   → INSERT managed_api_keys (hash)
            ├─ subscription_updated  → rotate key → retire active → INSERT new hash
            └─ subscription_cancelled/expired/payment_failed → revoke active key
```

The minted key plaintext is returned to the caller exactly once and never
logged; only `SHA-256(key)` is persisted.

## Provider signature schemes

| Provider | Header | Signed string | Encoding |
|----------|--------|---------------|----------|
| Lemon Squeezy | `X-Signature` | raw body | HMAC-SHA256 hex |
| Paddle | `Paddle-Signature` | `{ts}:{raw_body}` | HMAC-SHA256 hex |

Paddle's `ts` is checked against a 5-second replay tolerance (its documented
default); multiple `h1` values are accepted so secret rotation does not drop
deliveries. Lemon Squeezy signs the raw body alone and documents no unique event
id, so its idempotency key is derived from `meta.event_name` + `data.id`.

## Neon schema

`migrations/0001_managed_api_keys.sql` — `managed_api_keys` (key hashes, never
plaintext) + `billing_webhook_events` (idempotency ledger). Target project:
**Gateway** (`noisy-cloud-41146057`). The tenant column is not globally unique;
a partial unique index enforces at most one `active` key per tenant so the
rotate flow works.

## Key minting / rotation rules

1. **Mint** — on `subscription_created`: generate `cxn_agent_<32-hex>`, store
   only `SHA-256(key)` in `api_key_hash`, mark `active`.
2. **Rotate** — on `subscription_updated` / manual `rotate`: retire the current
   row (`rotating`), insert a new `active` row. The gateway picks up the change
   via `MANAGED_API_KEYS`.
3. **Revoke** — on `cancelled`/`expired`/`payment_failed`: set `revoked`.
4. **Idempotency** — key the webhook on the MoR event `id`; a replay of an
   already-processed event is a no-op (enforced by the `billing_webhook_events`
   primary key). The event is recorded before provisioning so a crash cannot
   double-grant on retry.

## Gateway binding

The gateway process consumes `MANAGED_API_KEYS` (JSON `{"<api_key>":"<tenant>"}`)
and `FUSION_JWT_SECRET`. On startup `TenantKeyRegistry::from_env` loads these
into the in-memory registry; per-tenant rate limiting and Sentinel secret
filtering are enforced in `internal/api/src/tenant.rs`. The managed subscriber
surface is `https://api.conxian-labs.com/v1/agent/*`.

When `MOR_WEBHOOK_DATABASE_URL` is set, the managed-auth path also resolves a
presented `cxn_agent_*` bearer by its SHA-256 hash against the same durable store
the webhook writes to (`KeyStore::active_key_by_hash`). This means a key minted
by a webhook authenticates immediately, without mirroring the plaintext into
`MANAGED_API_KEYS` or restarting the process. `MANAGED_API_KEYS` remains the
fallback for keys provisioned out of band. A store lookup error denies the
request (fail closed).

## Environment

| Variable | Purpose |
|----------|---------|
| `MERCHANT_CHECKOUT_URL` | MoR checkout URL (credit-card path) |
| `MOR_PROVIDER` | `lemonsqueezy` or `paddle`; enables the webhook route |
| `MOR_WEBHOOK_SECRET` | MoR webhook signing secret (per destination) |
| `MOR_WEBHOOK_DATABASE_URL` | Durable Neon/Postgres URL for the key store |
| `MANAGED_API_KEYS` | JSON map of live managed keys (from Neon) |
| `FUSION_JWT_SECRET` | HS256 secret for short-lived Fusion tokens |
| `RATE_LIMIT_REQUESTS_PER_MINUTE` | Per-tenant request budget (default 120) |

> The webhook requires MoR credentials (webhook signing secret + Neon write
> role). These are operator-managed and intentionally out of scope for CI. The
> `mor-webhook-conformance` workflow verifies the store against a disposable
> `postgres:17` service container using `MOR_WEBHOOK_TEST_DATABASE_URL`; no
> production credential or live payment is involved. A real payer journey
> (payment → entitlement → metering → support) remains a separate acceptance
> gate under Business #1343 / #1591.
