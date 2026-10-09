# Managed SaaS Gateway — Billing Webhook & Key Lifecycle

The Managed tier is provisioned automatically when the Merchant-of-Record
(Lemon Squeezy or Paddle) emits a subscription event. The webhook handler is
the only component that **mints and rotates** managed API keys; the Gateway
runtime never stores plaintext keys or LLM/agent payloads (BYOK / thin
orchestrator invariant).

## Flow

```
MoR (Lemon Squeezy / Paddle)
  └─ webhook (HMAC-signed)
       └─ handler
            ├─ verify signature + idempotency key
            ├─ subscription_created  → mint key  → INSERT managed_api_keys (hash)
            ├─ subscription_updated  → rotate key → UPDATE (rotating) → INSERT new hash
            ├─ subscription_cancelled/expired → revoke
            └─ publish {api_key -> tenant_id} into MANAGED_API_KEYS (Neon)
```

## Neon schema

`migrations/0001_managed_api_keys.sql` — `managed_api_keys` (key hashes, never
plaintext) + `billing_webhook_events` (idempotency ledger). Target project:
**Gateway** (`noisy-cloud-41146057`).

## Key minting / rotation rules

1. **Mint** — on `subscription_created`: generate `cxn_agent_<32-hex>`, store
   only `SHA-256(key)` in `api_key_hash`, mark `active`.
2. **Rotate** — on `subscription_updated` / manual `rotate`: transition the
   current row to `rotating`, insert a new `active` row, then delete/revoke the
   old key. The gateway picks up the change via `MANAGED_API_KEYS`.
3. **Revoke** — on `cancelled`/`expired`/`payment_failed`: set `revoked`.
4. **Idempotency** — key the webhook on the MoR event `id`; a replay of an
   already-processed event is a no-op (enforced by `billing_webhook_events`
   primary key).

## Gateway binding

The gateway process consumes `MANAGED_API_KEYS` (JSON `{"<api_key>":"<tenant>"}`)
and `FUSION_JWT_SECRET`. On startup `TenantKeyRegistry::from_env` loads these
into the in-memory registry; per-tenant rate limiting and Sentinel secret
filtering are enforced in `internal/api/src/tenant.rs`. The managed subscriber
surface is `https://api.conxian-labs.com/v1/agent/*`.

## Environment

| Variable | Purpose |
|----------|---------|
| `MERCHANT_CHECKOUT_URL` | MoR checkout URL (credit-card path) |
| `MANAGED_API_KEYS` | JSON map of live managed keys (from Neon) |
| `FUSION_JWT_SECRET` | HS256 secret for short-lived Fusion tokens |
| `RATE_LIMIT_REQUESTS_PER_MINUTE` | Per-tenant request budget (default 120) |

> The billing webhook handler requires MoR credentials (webhook signing secret +
> Neon write role). These are operator-managed and intentionally out of scope for
> CI; the schema + contract above make the handler a drop-in deployable.
