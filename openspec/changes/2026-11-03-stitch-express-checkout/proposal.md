# Stitch Express server-side checkout — Gateway fiat on-ramp

## Problem

The Stitch fiat on-ramp adapter returned a hardcoded
`https://checkout.stitch.money/?reference=...` URL and held no credential.
Stitch Express is a separate product from the Stitch Enterprise/Connect
GraphQL API: it issues a hosted payment link **server-side**, from a
short-lived token obtained with a client id and client secret. The static
placeholder produced a link that was not a real, payable checkout, so the
route could not settle anything.

## Change

`StitchAdapter` now carries the client id and secret and creates the checkout
server-side: it exchanges credentials for a bearer token
(`POST /api/v1/token`, scope `client_paymentrequest`) and creates a payment
link (`POST /api/v1/payment-links`) whose `data.payment.link` becomes the
payer redirect URL. Amounts convert to Stitch's integer ZAR minor units and
are rejected below the R1.00 minimum; `merchantReference` is sanitised to the
documented charset; a payer name is required. The adapter trait gains a
defaulted async `create_hosted_checkout` so redirect-only providers keep their
stateless URL builder and need no HTTP client.

Enabling `STITCH_MODE=active` now requires both `STITCH_CLIENT_ID` and
`STITCH_CLIENT_SECRET` and fails closed when either is absent; disabled mode
reads neither. Stitch Express delivers webhooks through Svix, so a
`verify_stitch_webhook` helper verifies the HMAC-SHA256 over
`{svix-id}.{svix-timestamp}.{raw-body}` against the Base64-decoded signing
secret, in constant time, and fails closed without a secret.

## Verification and release

Tests exercise the token exchange and payment-link creation against a WireMock
double, the minor-unit conversion and minimum, reference sanitisation, the
missing-payer-name and rejected-token paths, and Svix signature acceptance,
tamper rejection and malformed-secret failure. The startup-secret preflight
and its README contract are extended and bound to the Rust `from_env` contract.
Normal CI, independent review and dev/staged/main promotion remain required.
Rollback is a revert of this isolated patch. This does not accept the overall
on-ramp journey: registered redirect URLs, live webhook registration, the
`payment.paid` handler wiring, settlement reconciliation and qualified
production credentials remain separate operation-specific requirements. The
tests create no payment and use no live credential.
