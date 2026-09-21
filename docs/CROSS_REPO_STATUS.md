# Conxian Organizational Cross-Repository Status Dashboard

**Last Updated:** September 16, 2026
**Baseline Version:** v0.1.5

## Repository Status Matrix

| Repository | Role / Layer | Status | Primary Audit Findings / Notes |
|---|---|---|---|
| **lib-conxian-core** | L3 Core Primitives | ✅ Aligned (v0.3.3 / v0.1.5) | Shared types, MuSig2, DLC primitives, DePIN machine identities. |
| **conxian-gateway** | L1 Critical Gateway | ✅ Active (v0.1.5) | ISO 20022 XML normalizer, OData v4 callbacks, mBridge/Canton adapters. |
| **conxian-nexus** | L1 Critical Protocol | ✅ Aligned | Protocol state indexer and cross-chain state proof coordinator. |
| **conxius-enclave-sdk**| L3 Enclave SDK | ✅ Aligned | KMS/HSM hardware key abstraction and non-custodial signing. |
| **conxius-platform**  | L3 Orchestration | ✅ Audited | Deployment orchestration clean of non-sovereign dependencies. |
| **conxian_market**    | L2 Transparency Surface | ✅ Active | Treasury dashboard and monthly transparency reporting. |
| **conxian-labs-site**| L2 Corporate Surface | ✅ Active | Public B2B communication, legal, and enterprise contact point. |
| **conxian-business**  | L4 BOS Surface | ✅ Strategic | Business Operations System (BOS) strategy and governance. |
| **.github**          | L4 Org Governance | ✅ Hardened | Immutable Gitleaks action, Node CI, Rust CI, CODEOWNERS. |

## Domain Routing Compliance
- Protocol Surface (`conxian.org`): `nexus.conxian.org`, `gateway.conxian.org`, `sdk.conxian.org`, `platform.conxian.org`, `market.conxian.org`
- Corporate Surface (`conxian-labs.com`): `bos.conxian-labs.com`, `www.conxian-labs.com`
