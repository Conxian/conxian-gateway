---
name: gateway-billing-rail
description: This skill should be used when working on the gateway's billing, subscriptions, Lightning invoice rail, x402 settlement, or the Merchant-of-Record webhook — anything that turns a payment into a managed API key.
---

# Gateway billing rail

The gateway is the single monetization surface for the org (ADR-009). It turns a
payment into a `cxn_agent_*` managed API key. Two rails exist: a **Lightning
subscription rail** (working) and an **x402 pay-per-call rail** (config-gated).

## Plans (sats/month)

`internal/api/src/billing.rs`: starter 0, indie 37,700, business 128,000,
enterprise 1,000,000. `plan_by_id` also accepts `pro` as a legacy alias for
business. The webhook reads the tier from `data.items[0].price.name` (Paddle) or
`data.attributes.variant_name` (Lemon Squeezy).

## Lightning subscription rail (the sanctioned crypto-native path)

- `POST /api/v1/billing/subscribe {plan}` → returns `{invoice_id, payment_request
  (bolt11), amount_sats, expires_at, term, ...}`. It creates a real invoice; it
  does **not** report a purchase.
- `POST /api/v1/billing/verify {plan, tenant_id, invoice_id}` → on settlement
  returns `{api_key, jwt, tier, expires_at}`; pre-settlement it returns **402
  "Lightning invoice not settled"** (correct fail-closed).
- Backend: `HttpInvoiceBackend::from_env()` reads `LIGHTNING_INVOICE_CREATE_URL`,
  `LIGHTNING_INVOICE_CHECK_URL`, `LIGHTNING_INVOICE_API_KEY` (lnbits-native,
  `X-Api-Key` header). Unset → `NoInvoiceCreateUrl` (fail closed).
- Backend selection: `InvoiceBackendSelector::from_env()` chooses the **Strike**
  backend (`internal/api/src/billing_strike.rs`) when `STRIKE_API_KEY` is set
  (optional `STRIKE_API_BASE_URL`, default `https://api.strike.me`; sandbox is
  `https://api.dev.strike.me`), else the lnbits-native backend above. Strike is
  custodial — it collects the payment; it is not a Conxian custody layer and does
  not replace the merchant-of-record. See
  `docs/research/STRIKE_API_LESSONS_2026_11_11.md`.

## x402 pay-per-call rail

- `cmd/gateway/src/main.rs` calls `new_lightning_adapter_from_env()`, which selects
  `NwcLightningBackend` when `NWC_URI` parses, else the fail-closed
  `ProductionLightningBackend` stub (always `Unavailable`).
- **Setting `NWC_URI` is the single enablement step** for x402; no code change.
- Protected paths issue a 402 challenge: `/settle`,
  `/ingress/{iso20022,pain001,papss,brics}`, `/erp/sync`.

## Merchant-of-Record webhook

- `internal/api/src/mor_webhook.rs` is provider-neutral: `Provider::{LemonSqueezy,
  Paddle}` selected by `MOR_PROVIDER`. Route mounts only when `MOR_PROVIDER` +
  `MOR_WEBHOOK_SECRET` + `MOR_WEBHOOK_DATABASE_URL` are all set.
- **No card-network MoR serves this product.** Paddle, Polar, Creem and Dodo each
  prohibit crypto/financial-services in their acceptable-use policies; Lemon
  Squeezy's catalogue is dashboard-only. Do not onboard a card MoR.

## Key lifecycle

- Keys are stored **hashed only** (`managed_api_keys`); the plaintext is shown
  exactly once on mint/rotate.
- The minted key authenticates `/api/v1/agent/*` by SHA-256 hash against the same
  durable store the webhook writes to.
- `migrations/0001_managed_api_keys.sql` creates `managed_api_keys` +
  `billing_webhook_events`; apply it to the Neon branch before enabling.

## Tests to keep green

`internal/api/src/billing.rs` (invoice/verify/credit), `mor_webhook.rs`
(signature + event mapping), `onboarding.rs` (idempotent provisioning). Run
`cargo test --workspace`.
