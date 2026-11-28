# Payment control (P16) and reference-matched collections (P17) — Gateway #1003

## Problem

Business buyers ask for two controls the Gateway does not evidence. First, that
no single operator can release a payment: the existing release approvals
(`/admin/v1/releases/*`) are release-level, so the operator who creates a payment
can still approve it. Second, that an inbound payment can be matched to an
invoice without a spreadsheet: the Gateway formats and ingests ISO 20022
messages, but issues no per-payer reference and keeps no record of what happened
to one.

## Change

Add `internal/api/src/payment_control.rs` with a pure domain half and a thin
HTTP adapter.

P16 is a payment-intent state machine — `created`, `pending_approval`,
`approved`, `released`, `rejected`, `cancelled` — enforcing separation of duties
in code: the maker cannot approve or release, one principal cannot approve twice,
a quorum is required before release, terminal states are terminal, and every
transition appends an ordered audit entry. Release authorises a payment; it does
not execute it.

P17 derives a deterministic per-payer reference (`CX` plus ten Crockford base32
characters over tenant, payer, invoice and nonce), matches inbound receipts on
the normalised reference, and reports the result as data — `matched`,
`amount_mismatch`, `already_settled`, `duplicate`, `unknown` — rather than as an
error. Settlement is idempotent by receipt id. A currency mismatch records
nothing, so the operator can retry the same receipt.

The acting principal is the verified M2M identity, never a request field, and the
mutation handlers refuse a legacy shared bearer token because it carries no
per-principal identity.

Routes are off by default (`PAYMENT_CONTROL_ENABLED`), are added to a bearer-auth
router, and therefore carry explicit entries in the M2M scope matrix with a
route-coverage test; an unmapped route continues to deny JWTs.

## Verification and release

`cargo test -p conxian_api --lib payment_control` covers separation of duties,
quorum, duplicate approvals, terminal states, amount caps, audit ordering,
reference determinism and collision, all five match outcomes, replay idempotency,
returns, tenant scoping and journal ordering, plus three handler tests including
the legacy-token refusal. `cargo test --workspace` passes. Clippy and rustfmt are
clean for the changed crate; two `needless_bool` findings in `a2p.rs` and
`internal/compliance/src/zkc.rs` predate this change and are untouched.

Normal CI, independent review and dev/staged/main promotion remain required.
Rollback is a revert of this isolated patch, and the feature is additionally
dark by default.

This does not complete #1003. The store is process-local and in memory, so a
restart loses state; the atomic persistence layer, a retention bound, webhook
delivery for collection outcomes, and activation of M2M JWT mode are all still
required before this is usable in production. No custody, balance holding or
money movement is introduced.
