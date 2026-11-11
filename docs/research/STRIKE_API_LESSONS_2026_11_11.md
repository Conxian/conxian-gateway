# Strike API — design lessons for the Conxian gateway

**Date:** 2026-11-11
**Source:** `https://docs.strike.me/` (Strike API v1; API reference, webhooks, sandbox, API-key docs)
**Status:** Research. Findings are heuristic and from public documentation only; no Strike
credentials, sandbox access, or production integration were exercised.
**Scope:** what the Strike payments API teaches us about the gateway billing/payment lane,
and what to adopt, what we already do, and where our gaps are.

## What Strike is

A US-regulated **custodial** Bitcoin + Lightning payments API. REST/JSON at
`https://api.strike.me/v1/*`. Sandbox at `https://api.dev.strike.me` (simulated cash +
testnet BTC, gated behind business onboarding). Auth is a scoped **API key** or **OAuth
Connect**. Surface: invoices, receive requests, payment quotes (Lightning / on-chain /
LNURL-LN-address), currency-exchange quotes, payouts, subscriptions, and webhooks.

## Lessons (adopt / already-have / gap)

| # | Strike pattern | Our position | Action |
| --- | --- | --- | --- |
| 1 | **Versioned route path** `/v1/*` with a changelog; a webhook subscription must match the API version it was created under. | We already version `/api/v1/*`, but webhook version coupling is implicit. | **Adopt:** document that a webhook subscription is bound to the API version, and treat a version bump as a breaking change to re-subscribe. |
| 2 | **Two-step invoice flow**: `POST /invoices` creates the *obligation*, `POST /invoices/{id}/quote` returns the *payment instrument* (BOLT11 `lnInvoice`, optional `onchainAddress`). | We collapse obligation + instrument into one `create_invoice` call. | **Adopt (model):** separate the obligation (invoice) from the rail (quote). A plan could then be settled by Lightning **or** on-chain **or** bank without changing the obligation. |
| 3 | **Explicit state machine**: invoice `state ∈ {UNPAID, PENDING, PAID, CANCELLED}`; per-transaction `state ∈ {PENDING, COMPLETED}` + `amountReceived`. | Our `HttpInvoiceBackend` sniffs many shapes (`paid`, `status`, `preimage`, …) to guess settlement. | **Adopt:** prefer a typed `state` when the backend offers one; keep the heuristic path only as a fallback. (The Strike backend reads `state` directly.) |
| 4 | **Correlation ID** on invoices to bind them to an external entity. | We have idempotency keys in the credit ledger. | **Adopt:** surface a `correlationId` on our subscription invoices so an external order/tenant id maps to exactly one invoice. |
| 5 | **Multi-currency quotes** (BTC/USD/EUR/USDT/GBP/AUD) with an explicit `conversionRate` and `sourceAmount`/`targetAmount`. | Our plans are sats-denominated only (Lightning-native). | **Learn for later:** a fiat-priced plan needs an explicit quote + rate, not an implicit conversion. |
| 6 | **Scope-per-endpoint API keys** (`partner.invoice.create`, `partner.invoice.read`, …) and OAuth Connect for third parties. | We have tenant keys + Fusion JWT. | **Adopt:** granular scopes per billing operation; least privilege for any partner integration. |
| 7 | **Webhooks** (`invoice.created`/`updated`/`cancel`) with signature verification and version coupling. | We already verify MoR webhook signatures (`mor_webhook.rs`). | **Adopt:** subscribe to invoice settlement events instead of polling `check_settled` in a hot loop. |
| 8 | **First-class sandbox** (`api.dev.strike.me`, `dev.dashboard.strike.me`) with simulated balances, so code is written once and redeployed to prod. | We have no sandbox for the billing lane; tests use in-memory mocks. | **Adopt:** a `STRIKE_API_BASE_URL`-style switch already exists in our adapter; formalize a sandbox profile for the whole billing lane. |
| 9 | **OData list semantics** (`$filter`/`$orderby`/`$skip`/`$top`, max 100) on collection endpoints. | Our list endpoints are simple. | **Learn:** consistent, bounded pagination for any operator-facing list. |
| 10 | **Amount as a decimal string** in a currency object (`{"amount":"0.00128","currency":"BTC"}`), never a float. | We use integer sats internally. | **Adopt (at the boundary):** convert sats↔decimal string exactly, refusing sub-satoshi precision (done in `billing_strike.rs`). |

## What we should NOT copy

- **Custody.** Strike holds the funds it receives. We use it only to collect managed
  subscription payments; it is **not** a Conxian custody layer and does not change our
  non-custodial boundary.
- **Merchant-of-record.** Strike is a payments API, not an MoR/tax entity; it does not
  replace Lemon Squeezy / Paddle for tax and entitlement.
- **Vendor lock on the invoice rail.** Our `InvoiceBackend` trait already abstracts the
  rail, so Strike is one selector among several (`STRIKE_API_KEY`), never the only path.

## What this change does

`internal/api/src/billing_strike.rs` adds a fail-closed `StrikeInvoiceBackend` implementing
our existing `InvoiceBackend` trait, and `InvoiceBackendSelector` chooses it when
`STRIKE_API_KEY` is set (otherwise the lnbits-native backend is used, unchanged). It applies
lessons #3, #5, #10 directly (typed `state`, decimal-string amount with no sub-satoshi
rounding, fail-closed on underpayment) and sets up #2/#4/#7 for follow-up work.
