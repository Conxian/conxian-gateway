# Conxian-Labs Repository Readiness Gates (CON-227)

This document defines the required readiness gates across four control domains for every active repository in the Conxian-Labs portfolio.

## 1. Control Domains

| Domain | Scope | Primary Gatekeeper |
| :--- | :--- | :--- |
| **Legal/Public** | Public-safe status, licensing, and exposure risk. | Compliance Officer |
| **Regulatory** | KYC/AML, ISO 20022 compliance, and audit trails. | Compliance Lead |
| **Treasury** | Fund impact, settlement risk, and payout authority. | Treasury Manager |
| **Security** | Enclave integrity, signer paths, and execution risk. | Security Lead |

## 2. Gate Matrix by Repository

### Layer 1: Decentralization-Critical
*High-integrity repositories requiring strict mainnet-only branches.*

- **conxian-gateway / conxian-nexus**
  - [x] **Security**: TEE-proposal enforcement verified (CON-162).
  - [x] **Treasury**: 144-block institutional timelock active.
  - [x] **Regulatory**: Zero-PII pass-through verified for ZKC logic.
  - [x] **Legal**: Standardized MIT License and Security.md present.
  - [ ] **ALEX Settlement Readiness**: Read-only/shadow and research-gated. The `/v1/quote` compatibility path is unverified; unsigned payload preparation is fail-closed and enabled only with an explicitly configured exact helper principal. No signer, broadcast, receipt, or reconciliation path exists. See [`docs/research/ALEX_SETTLEMENT_EVIDENCE_AND_GATE_2026-07-22.md`](research/ALEX_SETTLEMENT_EVIDENCE_AND_GATE_2026-07-22.md).
  - [x] **API Lifecycle Governance**: Owner, versioning, and deprecation policy documented in `README.md`.
  - [x] **Policy-Engine Boundary**: Gateway transport/verification boundary and compliance decision boundary documented.
  - [x] **Observability Control**: Health/metrics and security-event logging expectations documented.
  - [x] **Incident Handling**: Security incident triage/containment/recovery process documented in `SECURITY.md`.
  - [x] **Release Discipline**: Control sign-off checklist documented in `RELEASE.md`.
  - [x] **Authority Boundary**: Explicit statement that gateway is not protocol source-of-truth and not custody authority.

### Layer 2: User Surface
*Product interfaces delivering enclave-backed institutional experiences.*

- **conxius-wallet / Conxian_UI**
  - [x] **Security**: Non-custodial enclave-storage paths verified (CON-208).
  - [ ] **Legal**: Terms of Service and Privacy Policy aligned with BOS.
  - [x] **Regulatory**: Identity resolution (ENS/BNS/WorldID) verified.
  - [ ] **Security**: Full functional E2E audit of wallet signing path.

### Layer 3: Shared Runtime & SDKs
*Infrastructure and developer surfaces supporting the stack.*

- **lib-conxian-core / conxius-enclave-sdk**
  - [x] **Legal**: Public exposure of SDK interfaces approved.
  - [x] **Security**: No hardcoded secrets or sentinel values in code.
  - [x] **Technical**: API stability and semver consistency verified.
  - [x] **Finance**: Structured finance tranche logic implemented (CON-452).

### Layer 4: Governance & BOS
*Strategic and organizational control planes.*

- **conxian-business**
  - [x] **Legal**: Investigation of public/private visibility boundary complete.
  - [x] **Strategic**: TAM-capture strategy and SYI logic aligned with ALEX.
  - [x] **Operational**: Maintainer-controlled payout toggle verified (CON-230).

## 3. Promotion Path (Staged -> Main)

1. **Verification**: All repo-scoped gates marked as complete.
2. **Audit**: Production execution audit (CON-400) confirmed.
3. **Acceptance**: Final go/no-go review (CON-229) signed off by ExCo.
