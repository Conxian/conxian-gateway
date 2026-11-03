# Billing access-token contract — Gateway #507

## Problem

Subscription activation requests an access token using the 30-day subscription
expiry. Managed authentication rejects tokens with more than 900 seconds
remaining. Direct string interpolation also does not encode tenant identities
as JSON data.

## Change

Cap issued access-token lifetime at the existing verifier maximum and encode
claims with serde_json. Subscription expiry and credit expiry remain separate
from access-token expiry. Preserve zero-duration token rejection and all
existing verifier restrictions. No tier entitlement mapping is introduced.

## Verification and release

Tests exercise the concrete billing token issuer through FusionJwt verification,
long subscription durations, escaped identities and existing negative cases.
Normal CI, independent review and dev/staged/main promotion remain required.
Rollback is a revert of this isolated patch. This does not accept the overall
payment journey: invoice redemption/binding, durable credits, registered key
lifecycle, allowed commercial-tier mapping, token refresh/revocation, metering,
dunning and qualified payment-backend configuration remain under #507 and
Conxian/conxian-business#1591. No payment or live key is created by these tests.
