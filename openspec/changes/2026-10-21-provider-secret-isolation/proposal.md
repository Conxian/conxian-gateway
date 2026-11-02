# Isolate disabled provider credentials from Gateway startup

Owner: Gateway #466; portfolio: Business #1390.

The minimum lane currently requires FIAT_WEBHOOK_SECRET and HMAC_SECRET even when
all fiat and OTP providers are disabled. Require these credentials only for their
existing enabled provider modes. Do not load them when disabled. Validate the
webhook provider against enabled adapters before accepting any HMAC, including a
correctly signed webhook addressed to a disabled provider.

Keep API_TOKEN, SETTLEMENT_INGRESS_SECRET and OFFLINE_QUEUE_SECRET mandatory.
They protect authentication, settlement ingress and encrypted/replay persistence;
removing those dependencies requires separate operation isolation and recovery
proof. No fallback secret, mock signer, unqualified custody or escrow is enabled.

The startup preflight and deploy declaration check must implement the same
conditional dependencies. The declaration check may omit a shared credential
only when every dependent mode is explicitly and uniquely disabled. Unknown,
missing, dynamic and duplicate declarations still require the credential. Keep
all CI workflow, promotion, review, soak and release controls intact.

Validation: native configuration tests for absent/unused and enabled/missing
secrets; disabled-provider signed-webhook rejection; Python contract/preflight
and deploy declaration tests; full existing hosted Rust/Node/security suites.
Production activation still requires review, normal promotion, protected secret
provisioning for the selected lane, deployed-source verification and live recovery.

Next slices: operation-level readiness for settlement/signing/offline queue;
independent escrow lifecycle (fund, verify, release/refund, expiry, replay and
restart/recovery); evidence-qualified Kwil/Tableland adapters. Those are remaining
work, not delivered by this change.
