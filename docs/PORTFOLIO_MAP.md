# Conxian-Labs Portfolio Map & Repository Inventory (CON-468 / CON-410)

This document serves as the canonical inventory and classification for the Conxian-Labs repository stack. It defines the layer, role, and evaluation standard for each component to ensure production integrity and decentralization alignment.

## 1. Repository Inventory by Layer

### Layer 1: Decentralization-Critical
*High-integrity repositories that manage protocol state, settlement, or sovereign execution.*

| Repository | Role | Production Path | Evaluation Standard |
| :--- | :--- | :--- | :--- |
| **conxian-nexus** | Protocol coordination and state verification. | `main` (Mainnet-only) | Deterministic execution, high-coverage testing. |
| **conxian-gateway** | Institutional compliance pipe and blockchain state listener. | `main` (Mainnet-only) | Zero-PII persistence, SLA-grade async Rust. |

### Layer 2: User and Application Surface
*Repositories delivering product interfaces and direct user interactions.*

| Repository | Role | Production Path | Evaluation Standard |
| :--- | :--- | :--- | :--- |
| **conxius-wallet** | Non-custodial institutional wallet (Enclave-backed). | `main` (Production) | Non-custodial proof, E2E functional audit. |
| **Conxian_UI** | Primary product web interface. | `main` (Production) | UX consistency, secure session handling. |
| **conxian-labs-site** | Public communication and portfolio surface. | `main` (Public) | Branding alignment, no internal leakage. |

### Layer 3: Shared Runtime & Developer Infrastructure
*Libraries, SDKs, and deployment tools supporting the wider stack.*

| Repository | Role | Production Path | Evaluation Standard |
| :--- | :--- | :--- | :--- |
| **lib-conxian-core** | Common models, types, and persistence logic. | `main` (Shared) | Strict semver, backwards compatibility. |
| **conxius-enclave-sdk** | Public developer SDK for Conxian integration. | `main` (Public) | API stability, comprehensive documentation. |
| **conxius-platform** | Orchestration, automation, and platform coordination. | `main` (Internal) | Workflow reliability, secret-safe CI. |
| **stacksorbit** | Deployment automation and infrastructure tools. | `main` (Internal) | Repeatable deployments, audit-ready config. |

### Layer 4: Governance & Operating System
*Repositories defining business logic, governance, and organizational standards.*

| Repository | Role | Production Path | Evaluation Standard |
| :--- | :--- | :--- | :--- |
| **conxian-business** | BOS (Business Operations System) state & strategy. | `main` (Strategic) | Accuracy of strategy, separation of private/public. |
| **.github** | Org-wide governance standards and CI templates. | `main` (Governance) | Template integrity, security hardening. |

## 2. Review Standards per Layer

### Decentralization-Critical (L1)
- **Review Lens**: Mathematical correctness, cryptographic verification, and non-custodial integrity.
- **Mandatory**: No stubs, mocks, or bypasses on production branches. Full testnet/mainnet separation.

### User & Application Surface (L2)
- **Review Lens**: Security of interaction, session integrity, and UX fidelity.
- **Mandatory**: Verification of non-custodial posture and enclave-signed pathing.

### Shared Runtime & SDKs (L3)
- **Review Lens**: Dependability, API stability, and performance.
- **Mandatory**: Semver adherence and exhaustive documentation for downstream consumers.

### Governance & BOS (L4)
- **Review Lens**: Strategic accuracy and operational clarity.
- **Mandatory**: Clear distinction between public-safe messaging and internal-only operations.

## 3. Dependency Map
- **lib-conxian-core** is the foundation for almost all Rust-based L1 and L3 repos.
- **conxius-wallet** depends on **conxian-gateway** for state and **conxius-enclave-sdk** for logic.
- **conxian-nexus** coordinates state across the gateway and protocol layers.
- **conxius-platform** automates the deployment of **conxian-gateway** and **stacksorbit**.

## 4. Current Discrepancies & Evaluation Gaps
- **Framing Inconsistency**: `conxius-wallet` is often treated as a standard app; it must be evaluated as a Layer 2 high-integrity surface with non-custodial proofs.
- **Contamination Risk**: Historical drift in `conxian-business` sometimes leaks testnet or placeholder logic into production strategy.
