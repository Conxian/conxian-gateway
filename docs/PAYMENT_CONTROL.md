# Gateway Payment Control (P16) and Collections (P17)

> **Status:** implemented on `dev`, **off by default**. Mounted only when
> `PAYMENT_CONTROL_ENABLED` is set. The store is process-local and in memory, so
> this is a working control surface, **not** a durable ledger — see
> [Known limitations](#known-limitations). No custody, no balance holding, no
> money movement.

## Where this fits

Business buyers comparing the Gateway against VALR Business, 21bitcoin and Strike
Business ask for two things before anything else:

1. **Proof that no single operator can release a payment.** Today the Gateway
   has release approvals at the admin layer (`/admin/v1/releases/*`). That is a
   release-level control, not a payment-level one: it does not stop the operator
   who created a payment from approving it.
2. **Reconciliation that matches an inbound payment to an invoice.** The Gateway
   emits and ingests `pain.001`, `pacs.008`, `camt.053` and `camt.054`, but it had
   no per-payer reference that makes matching deterministic and no ledger that
   records what happened to each reference.

This module adds both, in `internal/api/src/payment_control.rs`. The rationale
and the competitor evidence live outside this repository: see
`Conxian/.github-private` PR #148 (`docs/commercial/COMPETITOR_VALR_BUSINESS_2026_11_27.md`).

## Enablement

| Variable | Effect |
|---|---|
| `PAYMENT_CONTROL_ENABLED` | `1`/`true`/`yes` mounts the routes. Anything else leaves them unmounted. |
| `PAYMENT_CONTROL_MAX_AMOUNT_MINOR` | Optional hard cap on a single payment, in integer minor units. `0` or unset disables the cap. |

Mounted routes are still bearer-authenticated. Because they are added to a
bearer-auth router, each one carries an explicit entry in the M2M scope matrix
(`required_scope_for_request` in `internal/api/src/auth.rs`); an unmapped route
denies JWTs by default.

## Identity is the control

The acting principal is taken from the verified M2M identity
(`VerifiedM2mIdentity.service_id`), **never from the request body**. A maker
therefore cannot approve their own payment by claiming to be someone else.

A **legacy shared bearer token carries no per-principal identity**, so it cannot
satisfy separation of duties. The mutation handlers refuse it with `403` and a
message that says why. Payment control is usable only in M2M JWT/dual mode.

## P16 — payment-level maker-checker

```
created --submit--> pending_approval --approve (quorum met)--> approved --release--> released
                            |                                                  ^
                            +--reject--> rejected                              |
created|pending_approval --cancel (maker only)--> cancelled                   |
                                          approved --release (maker) ----------+ refused
```

| Rule | Enforcement |
|---|---|
| Only the maker may submit or cancel | `submit`/`cancel` compare the actor with `created_by` |
| The maker may not approve | `approve` returns `SamePrincipal` |
| The maker may not release | `release` returns `SamePrincipal` |
| A quorum is required | `approved` only when `approvals.len() >= required_approvals` |
| One principal, one approval | a repeat approval is refused (`AlreadyApproved`), not silently dropped |
| Terminal states are terminal | `released`, `rejected`, `cancelled` accept no further transition |
| Re-checked at release | `is_releasable()` re-verifies quorum and that no approval came from the maker |

Every transition appends an `AuditEntry { seq, at_ms, actor, action, from, to }`.
`seq` is strictly increasing, so the trail replays the payment's whole life and a
missing step is detectable.

`release` **authorises** a payment; it does not execute it. Execution stays with
the existing settlement paths (`/settle`, `/iso20022/*`, `/settlement/*`).

## P17 — reference-matched collections

A reference is derived deterministically from `(tenant, payer_id, invoice_id,
nonce)`: `CX` plus ten Crockford base32 characters (50 bits). The alphabet omits
`I`, `L`, `O` and `U`, so a reference read off a phone screen is unambiguous.
The same inputs always yield the same reference, so a retried request cannot
create a second one; different inputs on the same reference are refused as a
collision.

Inbound receipts are matched on the normalised reference (whitespace and dashes
removed, upper-cased) and the result is a first-class outcome, not an error:

| Outcome | Meaning | Ledger effect |
|---|---|---|
| `matched` | reference known, amount and currency equal | reference becomes `settled` |
| `amount_mismatch` | reference known, amount differs | stays `pending`; operator decides |
| `already_settled` | a second payment arrived on a settled reference | recorded, nothing changes |
| `duplicate` | the same `receipt_id` replayed | nothing changes (idempotent) |
| `unknown` | reference not issued by us | recorded in suspense |

A **currency** mismatch is a hard `400`: nothing is recorded, so the operator can
fix the feed and retry the same receipt id.

`mark_returned` moves a settled reference to `returned` — money that came back is
no longer settled, and the state never silently returns to `pending`.

## API

All paths are under `/api/v1`. POST bodies are JSON.

| Method | Path | Scope | Purpose |
|---|---|---|---|
| POST | `/payments/intents` | `write:settlement` | create an intent (`id`, `tenant`, `amount_minor`, `currency`, `payee`, optional `reference`, `required_approvals`) |
| GET | `/payments/intents/{id}` | `read:settlement` | read one intent with its audit trail |
| POST | `/payments/intents/{id}/submit` | `write:settlement` | maker submits for approval |
| POST | `/payments/intents/{id}/approve` | `write:settlement` | checker approves |
| POST | `/payments/intents/{id}/reject` | `write:settlement` | checker rejects |
| POST | `/payments/intents/{id}/release` | `write:settlement` | release an approved payment |
| POST | `/collections/references` | `write:settlement` | issue a per-payer reference |
| POST | `/collections/inbound` | `write:settlement` | record and match an inbound receipt |
| GET | `/collections/statement?tenant=` | `read:settlement` | per-tenant statement plus unsettled count |
| GET | `/payments/journal` | `read:settlement` | append-only cross-object journal |

Errors are typed and mapped: `404` unknown intent/reference, `403` separation of
duties or missing identity, `409` illegal transition, duplicate, already settled
or reference collision, `400` validation and currency mismatch.

## Known limitations

- **Not durable.** State lives in a process-local `Mutex<PaymentControl>`. A
  restart loses intents, references and receipts. Wiring the atomic persistence
  layer is a precondition for enabling this in production; it is tracked in
  `Conxian/conxian-gateway#1003`.
- **No retention bound.** The maps and journal grow without eviction. A durable
  backend must impose one.
- **No webhook delivery.** `collections` outcomes are pull-only; subscribers are
  not notified.
- **Not activated.** Default-off, and `M2M_GATEWAY_AUTH_MODE` still defaults to
  `legacy`, so the identity the control plane requires is not yet the default.

## Tests

`cargo test -p conxian_api --lib payment_control` covers separation of duties,
quorum, duplicate approvals, terminal states, amount caps, audit ordering,
reference determinism and collision, all five match outcomes, replay
idempotency, returns, tenant scoping and journal ordering. Three tests exercise
the HTTP handlers directly, including the refusal of a shared bearer token.
