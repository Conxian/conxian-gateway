# Gateway architecture and operating boundary

## Role

Conxian Gateway translates validated external messages into tokenless routing events and applies transport, tenant, compliance, and policy controls. It does not issue a native token, custody customer keys, claim chain finality from message receipt, or implement Nexus proof arithmetic.

## Public protocol and commercial operation

| Surface | Public protocol responsibility | Managed Conxian-Labs responsibility |
|---|---|---|
| Routing | Stateless event schemas and deterministic route inputs | Provider connectivity, throughput, retries, and support |
| ISO 20022 | Bounded translation profile and test fixtures | Bank-specific profiles, conformance certification, reconciliation, and compliance operations |
| Authentication | Interoperable request contracts | API-key lifecycle, JWT issuer operation, tenant policy, and revocation |
| Rate limiting | Single-process self-hosted baseline | Shared atomic counters, tier policy, abuse response, and capacity planning |
| Audit | Structured event hook with payload digests and correlation ids | Durable append-only sink, retention, access controls, and regulatory export |

## ISO 20022 translation

`internal/compliance/src/iso20022.rs` accepts at most 1 MiB and supports:

- XML namespaces for pacs.008, pacs.009, pain.001, and camt.053;
- a strict JSON profile with unknown fields rejected;
- decimal-to-minor-unit conversion without floating point;
- deterministic SHA-256 payload binding;
- bounded identifiers, parties, currencies, and dates;
- rejection of XML `DOCTYPE` declarations.

The translator is an ingress normalization layer, not an XSD validator or clearing-network verifier. It never marks an ingested message as final. camt.053 is translated as reporting evidence and is rejected by settlement-initiation handlers.

The XML profile currently extracts the first payment transaction in a document. Batch messages require an explicit versioned batch event before they can be accepted. XML entity text is preserved as encoded source text; bank-specific canonicalization and XSD validation remain a commercial conformance gate.

## Managed API boundary

Managed requests require an authenticated tenant. Fusion JWTs are pinned to HS256, require a bounded expiry, validate tenant/tier fields, and are never accepted without a configured secret. The middleware attaches the tenant context to the request and emits a structured completion event without logging credentials or payload bodies.

The checked-in fixed-window limiter is process-local and suitable only for basic self-hosted/single-instance routing. A high-availability managed deployment must inject a shared atomic limiter, define fail-closed behavior for backend loss, and prove quota consistency under concurrent replicas before advertising enterprise rate limits.

API keys are currently loaded from environment configuration. Production readiness requires hashed-at-rest key records, rotation/revocation, scoped capabilities, and an authoritative database-backed registry.

## Audit boundary

Structured tracing is an audit hook, not a durable audit trail. A managed deployment must route events to an append-only sink and bind at least tenant identity, correlation id, route, result, policy version, payload digest, and timestamp. Raw ISO payloads and credentials must not be written to logs.

## Repository ownership

- `conxian-nexus`: observation, state commitments, proof verification, and Bitcoin anchoring.
- `conxian-gateway`: routing, ISO 20022 translation, tenant middleware, and compliance adapters.
- `conxius-enclave-sdk`: non-custodial hardware-backed signing and attestation evidence.
- `conxius-platform`: multi-tenant deployment orchestration, dashboards, and SLA evidence.

Legacy Stacks and other network adapters remain compatibility surfaces. They are not protocol trust roots and do not imply a Conxian token. Removal must be fixture-backed and coordinated with consumers instead of deleting dependencies by name.
