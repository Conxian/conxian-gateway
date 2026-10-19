# Preserve the API token installation contract

Owner: Gateway #544; continuation of #545, Business #1411/#1390. R1 implementation; R3 deployment and R4 acceptance remain separate.

Recover the narrow compatibility change on current dev without replacing its configuration, persistence, provider or initialization behavior. New installer environment/Compose output uses `API_TOKEN`. Runtime accepts the legacy `CONXIAN_GATEWAY_AUTH_TOKEN` only when the canonical variable is absent; an explicitly invalid canonical value must never fall back. Both forms reject empty/sentinel/installer-placeholder values. Error output never contains token values.

The local startup preflight must use the same precedence and report one logical canonical requirement. Deployment declarations continue requiring `API_TOKEN`; the legacy alias is not a new mandatory deploy secret. Installer doctor token validation rejects its unconfigured default; its other historical diagnostics remain outside this scoped compatibility correction.

Acceptance: native resolver and installer tests, alias/precedence/invalid-value/redaction process tests, deploy-declaration conformance, existing workspace/default/mock suites and actual container initialization. No prices, entitlement, external resource, credential or production authority changes. Existing #545 retains all unrelated work/history until independently reviewed or proven superseded.
