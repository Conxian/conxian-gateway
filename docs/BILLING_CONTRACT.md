# Shared billing configuration integration

Business [#1343](https://github.com/Conxian/conxian-business/issues/1343) owns
billing integration; [#1390](https://github.com/Conxian/conxian-business/issues/1390)
owns capability/dependency propagation. Pricing and ecosystem economics research
are a separate sprint-exit track in [#1419](https://github.com/Conxian/conxian-business/issues/1419).

The engine's existing `billing` module re-exports the versioned Core Gateway
rates and `billing_catalog()`. Existing public constant paths, MRR reports,
self-hosted treatment and volume-discount calculations are preserved. Core's
machine-readable `spec/billing_conformance.v1.json` describes the compiled
configuration; it is not an offer, paid entitlement or collection receipt.

The manifest/lockfile pin the exact Core candidate containing this contract and
the opt-in SDK 2.1.0 provider-policy corrections. No additional features or
provider execution paths are enabled. Conformance tests retain golden service
totals immediately below/at the volume threshold, distinct USD-cent/satoshi
units and fail-closed provider identity classification through the dependency.

Nexus signature quotas and satoshi upgrade amounts are a separate product.
`SubscriptionTier::enterprise_fee_cap()` configures ADR-004 settlement-cap
eligibility; it does not prove payment or alter Gateway's MRR volume discount.
No cross-product entitlement mapping or currency conversion is inferred.

Access-token lifetime is separate from subscription and credit expiry.
Fusion-issued JWTs are capped at the existing managed-auth maximum of 900
seconds, even when activation passes a 30-day subscription expiry. Claims
are encoded as JSON data so tenant identity cannot alter the claim structure.
This does not introduce token refresh, revocation or a commercial-plan to
authorization-tier mapping; those remain under #507 before paid acceptance.
The real issuer now checks signing configuration and the existing verifier's
allowed tier contract before invoice creation, and validates the exact tenant
claims before settlement lookup or credit grants. Failed activation preflight
returns HTTP 503 without exposing configuration values. Unsupported commercial
tiers remain unavailable until an explicit entitlement mapping is reviewed.

The calculator remains a local reporting library. This change does not add a
billing HTTP endpoint, authenticate a customer's subscription, collect charges
or confirm MRR. Merchant webhook signatures/replay handling, durable
tenant/key lifecycle and actual service metering remain under Gateway #507 and
Market #159. The public acquisition surface must remain unavailable until its
real offer/payment/entitlement integration is qualified.

Develop this dependency candidate through exact-head review and CI first.
Core review, staged/main promotion, live deployment, provider/custody
qualification and commercial acceptance remain separate gates. The published
SDK =2.0.17 companion's compatibility boundary is unchanged upstream and is not
reported as upgraded by this consumer change.
