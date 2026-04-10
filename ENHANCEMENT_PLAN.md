# Conxian Gateway: Enhancement & Alignment Plan

This document outlines the discrepancies identified during the Systemic Alignment Audit and the subsequent enhancements implemented to reach production-grade readiness.

## 1. Discrepancy Reconciliation

### Identity Resolution (CON-66)
- **Gap**: Linear issue marked as "Done", but implementation was limited to mock placeholders.
- **Enhancement**: Implemented `resolve_identity` in `IdentityManager` supporting ENS, BNS, World ID, and Web3.bio. Added `IdentityResolutionRequest` and `IdentityResolutionResponse` types to `conxian-core`. Exposed `/api/v1/identity/resolve` endpoint.

### DLC Orchestration (CON-62 / CON-72)
- **Gap**: Functional logic for DLC Bond lifecycle was missing from the gateway core.
- **Enhancement**: Defined `DlcOrchestrator` trait in `conxian-core` to formalize the lifecycle of Bitcoin-native DLC bonds and coupon distribution.

### Sovereign Sharding (CON-69)
- **Gap**: Tableland persistence was only mentioned in documentation.
- **Enhancement**: Added `commit_to_tableland` (now part of `SovereignCommit`) to `ZkcVerifier` to simulate decentralized SQL state commitments for off-shore yield routing.

### Compliance & Mainnet Readiness (CON-151 / CON-156)
- **Gap**: Insufficient separation of concerns and heavy reliance on hardcoded mock values in handlers.
- **Enhancement**: Refined `internal/api/src/handlers.rs` to better utilize `SharedState` and injected services. Hardened A2P and Fiat routers with better error handling and structure.

### Infrastructure Migration (CON-329)
- **Gap**: Web2 dependencies (Neon, Supabase) needed clear mapping for sovereign transition.
- **Enhancement**: Created canonical inventory in `docs/SAB_MIGRATION.md`. Implemented `SovereignCommit` hooks in the compliance layer to decouple from Web2 persistence.

### Institutional Secrets Hardening
- **Gap**: Single-token API authentication was insufficient for institutional SLAs.
- **Enhancement**: Implemented role-based `AuthStore` with constant-time validation and multi-identity support.

## 2. Tool Mapping Verification

| Tool | Purpose | Status |
| :--- | :--- | :--- |
| **Neon** | Serverless Postgres for institutional ledger storage. | Mapped (Sovereign replacement identified) |
| **Supabase** | Real-time 3-Statement financial modeling and ARR tracking. | Mapped (Sovereign replacement identified) |
| **Render** | High-availability hosting for the Gateway API. | Verified (Transition to Docker Swarm mapped) |
| **Stitch** | AI-powered UI generation for Conxius Wallet/UI. | Verified (Indirect) |

## 3. Sequential Execution Progress
- [x] Audit codebase vs. Linear history.
- [x] Implement Identity Resolution (ENS/BNS/WorldID).
- [x] Formalize DLC Orchestrator and Tableland interfaces.
- [x] Hardened API handlers for mainnet readiness.
- [x] Implement SAB Infrastructure Migration hooks and inventory (CON-329).
- [x] Harden institutional secrets and telemetry.
- [x] Final verification and submission.

## 4. Maintenance Standards
- Mandatory `cargo clippy` and `cargo fmt` checks.
- Zero-PII persistence in the compliance pipe.
- Strict adherence to BOS (Sovereign Business Operations System) boundaries.
