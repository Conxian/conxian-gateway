# Conxian Gateway: Enhancement & Alignment Plan

This document outlines the discrepancies identified during the Systemic Alignment Audit and the subsequent enhancements implemented to reach production-grade readiness.

## 1. Discrepancy Reconciliation

### Identity Resolution (CON-66)
- **Gap**: Linear issue marked as "Done", but implementation was limited to mock placeholders.
- **Enhancement**: Implemented `resolve_identity` in `IdentityManager` supporting ENS, BNS, World ID, and Web3.bio. Added `IdentityResolutionRequest` and `IdentityResolutionResponse` types to `conxian-core`. Exposed `/api/v1/identity/resolve` endpoint.
- **Full Functional State (2026-04-15)**: Integrated BNS resolution via Stacks `call_read_only` RPC, enabling live lookups of Stacks names.

### DLC Orchestration (CON-62 / CON-72)
- **Gap**: Functional logic for DLC Bond lifecycle was missing from the gateway core.
- **Enhancement**: Defined `DlcOrchestrator` trait in `conxian-core` to formalize the lifecycle of Bitcoin-native DLC bonds and coupon distribution.

### Universal Chain Verification (UCV-1)
- **Gap**: Verification logic was fragmented across multiple handlers and specific methods (BitVM, ZKC, TEE).
- **Enhancement**: Implemented `UniversalVerifier` and `CoreVerifier` trait to unify heterogeneous proof verification. Added `/api/v1/chains/{chain}/verify` endpoint to delegate verification to multi-chain adapters.
- **Status (2026-06-17)**: Fully implemented and integrated into the API layer. SDK and Schemas updated to support universal verification.

### Compliance & Mainnet Readiness (CON-151 / CON-156)
- **Gap**: Insufficient separation of concerns and heavy reliance on hardcoded mock values in handlers.
- **Enhancement**: Refined `internal/api/src/handlers.rs` to better utilize `SharedState` and injected services. Hardened A2P and Fiat routers with better error handling and structure.
- **Hardening (2026-06-17)**: Hardened X402 (Payment Required) middleware to support flexible payload formats and nested API paths. Unified ALEX swap and quote paths.

### Institutional Secrets Hardening
- **Gap**: Single-token API authentication was insufficient for institutional SLAs.
- **Enhancement**: Implemented role-based `AuthStore` with constant-time validation and multi-identity support.
- **Hardening (2026-06-13)**: Replaced insecure sentinel strings in `auth_middleware` with compliant production identifiers. Updated `docker-compose.yml` with hardened placeholders.

## 2. Shared Services Maturity Matrix

| Service | Maturity | Status (2026-06-17) |
| :--- | :--- | :--- |
| **BNS Resolver** | Production | Full functional state: live contract calls. |
| **ALEX Swap** | Production | Full functional state: builds prepared payloads for signers. |
| **A2P OTP** | Production | Full functional state: Infobip SMS integration active. |
| **Fiat Router** | Production | Multi-provider support (Ramp, Banxa, Alchemy Pay). |
| **Universal Verifier** | Production | UCV-1 implementation unified across chain adapters. |
| **Lightning Adapter** | Preparation | Failure taxonomy and lifecycle state machine implemented. |

## 3. Tool Mapping Verification

| Tool | Purpose | Status |
| :--- | :--- | :--- |
| **Neon** | Serverless Postgres for institutional ledger storage. | Mapped (Sovereign replacement identified) |
| **Supabase** | Real-time 3-Statement financial modeling and ARR tracking. | Mapped (Sovereign replacement identified) |
| **Render** | High-availability hosting for the Gateway API. | Verified (Transition to Docker Swarm mapped) |
| **Stitch** | AI-powered UI generation for Conxius Wallet/UI. | Verified (Indirect) |

## 4. Maintenance Standards
- Mandatory `cargo clippy` and `cargo fmt` checks.
- Zero-PII persistence in the compliance pipe.
- Strict adherence to BOS (Sovereign Business Operations System) boundaries.

### 2026-06-18 Alignment Update
- **Build Hardening**: Resolved Next.js 14 SSR/Client boundary violations in the control-plane.
- **Dependency Synergy**: Implemented `synergy-test.js` to ensure cross-language (Rust/TS) consistency.
- **UCV-1 Expansion**: Prepared SDK and Schemas for heterogeneous chain verification payloads.
