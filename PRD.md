# Conxian Gateway: Institutional Compliance Pipe

Institutional-grade middleware bridging Bitcoin/Stacks state logic with enterprise compliance, featuring mathematically verifiable state proofs and ZK-compliant auditing.

## 1. Vision & Strategy
Conxian is designed to capture the Total Addressable Market (TAM) of Bitcoin-native liquidity ($10B+), moving beyond the initial Stacks Serviceable Addressable Market (SAM).

### Industry Enhancement Pillars
- **A. sBTC "Suction" Pattern**: Incentivize native BTC-to-sBTC migrations via the Sovereign Yield Index (SYI).
- **B. BitVM & DLC Bonds**: Trustless cross-chain state verification and non-custodial Bitcoin debt.
- **C. Institutional ISO 20022 Egress**: Banking-standard messaging (pacs.008) for legacy egress.
- **D. Workload Identity Federation (WIF)**: TEE-based agent authentication without static keys.

## 2. Requirements Tracking
- [x] R1: Rust Workspace Initialization (Status: Complete)
- [x] R2: Bitcoin State Engine Porting (Status: Complete)
- [x] R3: Stacks State Engine Porting (Status: Complete)
- [x] R4: Institutional API & Auth Porting (Status: Complete)
- [x] R5: ZKC Module Implementation (Status: Enhanced)
- [x] R6: Audit-Ready Cleanliness (Status: Complete)
- [x] R7: Robustness & Graceful Shutdown (Status: Complete)
- [x] R8: Core Library Alignment (Status: Complete)
- [x] R9: Unified ZKC API (Status: Complete)
- [x] R10: Basic Metrics Support (Status: Complete)
- [x] R11: Persistence Support (Status: Complete)
- [x] R12: Enhanced Stacks RPC (Status: Complete)
- [x] R13: Uptime and Enhanced Metrics (Status: Complete)
- [x] R14: Atomic Persistence (Status: Complete)
- [x] R15: Cross-Chain State Referencing (Status: Complete)
- [x] R16: Enhanced Health Monitoring (Status: Complete)
- [x] R17: Detailed API Metrics (Status: Complete)
- [x] R18: Configurable Sync Intervals (Status: Complete)
- [x] R19: Stale Sync Detection & Observability (Status: Complete)
- [x] R20: ZKML Verification Support (Status: Complete)
- [x] R21: MVCR Generation (Status: Complete)
- [x] R22: Institutional Treasury Monitoring (Status: Complete)
- [x] R23: TAM-Capture Strategy Implementation (Status: Complete)
- [x] R24: WIF Identity Exchange (Status: Complete)
- [x] R25: ISO 20022 Egress Support (Status: Complete)
- [x] R26: BitVM Attestation Verification (Status: Complete)
- [x] R27: Fiat Webhook Verification (Status: Complete)
- [x] R28: Production Fiat On-Ramp Integration (Status: Complete)
- [x] R29: Global Stateless OTP Messaging (Status: Complete)
- [x] R30: Specialized NTT Relayer Deployment (Status: Complete)
- [x] R31: Conxian Job Card Schema (CJCS) v2.0 Integration (Status: Complete)
- [x] R32: BitVM2-Backed Job Card Settlement Verification (Status: Complete)
- [x] R33: Institutional OData v4 ERP Sync (Status: Complete)
- [x] R34: Advanced Axum Observability & Latency Tracking (Status: Complete)
- [x] R35: Canonical Portfolio Mapping (Status: Complete)
- [x] R36: Industrial Intent & x402 Alignment (Status: Complete)
- [x] R37: Structured Finance Tranche Implementation (Status: Complete)
- [x] R38: Phase 7 Sovereign Labor & Sharding Verification (Status: Complete)
- [x] R39: Network Promotion Policy (dev -> main) Integration (Status: Complete)

## 3. Progress Log
- 2026-02-13: Initialized workspace structure.
- 2026-03-22: Sovereign/Institutional Realignment (Jules).
- 2026-03-23: Industry Enhancement Upgrade (Jules):
    - Implemented BitVM attestation verification for trustless state layer.
    - Added ISO 20022 Egress (pacs.008) for institutional banking alignment.
    - Integrated Workload Identity Federation (WIF) for TEE-based enclave authentication.
    - Enhanced Treasury Monitor with sBTC "Suction" simulation and Sovereign Yield Index (SYI).
- 2026-03-24: Fiat Gateway Implementation (Jules):
    - Implemented FiatRouter with production-grade Ramp and Investec integrations.
    - Added HMAC-SHA256 signature verification for authenticated webhooks.
- 2026-03-25: A2P & NTT Enhancement (Jules):
    - Implemented Infobip integration and hardened stateless OTP verification in A2pRouter.
    - Deployed specialized NTT Relayer for sovereign bridging of native token transfers.
- 2026-03-26: Institutional Hardening & CJCS v2.0 (Jules):
    - Refactored API layer to use dependency injection via `AppState`, removing hardcoded mocks.
    - Integrated CJCS v2.0 JSON-LD into core SDK for labor orchestration.
    - Implemented BitVM2 verification floor for trustless Job Card settlement.
- 2026-04-10: Mainnet Alignment & Production Readiness (Jules):
    - Implemented SovereignCommit hooks for Tableland migration (CON-329).
    - Enhanced institutional metrics with TAM capture and SYI tracking.
    - Established mandatory TERMS.md and PRIVACY.md institutional documentation.
    - Created canonical portfolio map and repository inventory (CON-468/CON-410).
    - Implemented ALEX DEX client and exposed quote/swap API endpoints (CON-136).
    - Integrated maintainer-controlled bounty payout toggle (CON-230).
    - Implemented Industrial Intent (x402) metadata and structured finance tranches (CON-451/CON-452).
    - Hardened security boundaries and aligned system wallets with mainnet prefixes (CON-208).
- 2026-04-15: Full Functional Consolidation (Jules):
    - Completed Stacks BNS resolution via `call_read_only` in `IdentityManager`.
    - Hardened ALEX swap path to return prepared transaction payloads for signer integration.
    - Implemented production-grade Infobip SMS egress in `A2pRouter`.
    - Harmonized `AppState` dependency injection for BNS-capable `IdentityManager`.
    - Verified all shared services reach full functional state (Identity, ALEX, A2P).

- 2026-04-24: Full System Review, Repair, and Alignment (Jules):
    - Remediated contamination guard failures by replacing simulated identifiers across the codebase.
    - Implemented ZKML-backed Guardian Attestation verification in ZkcVerifier.
    - Enhanced ISO 20022 support with pacs.008 payment generation from Job Cards.
    - Standardized release hygiene and validated workspace-wide version consistency.
    - Aligned all documentation with v1.9.2 standards.

## 4. Technical Implementation Details
- **TAM Engine**: Simulates growth of sBTC liquidity and tracks the Sovereign Yield Index.
- **WIF Manager**: Handles OIDC-to-GCP token exchange for enclave-signed attestations.
- **BitVM Verifier**: Implements state-root commitment verification for optimistic fraud proofs.
- **ISO 20022 Forge**: Generates standardized XML banking messages for institutional egress.
- **ALEX Client**: Interfaces with ALEX Stacks DEX for sBTC liquidity and swap operations.
- **A2P Router**: Orchestrates global stateless OTP delivery via Infobip with HMAC verification.
- **NTT Relayer**: Observes and relays Native Token Transfer events for the sovereign bridge.

## 5. Audit & Alignment Progress (April 2026)
- **Mainnet Readiness Audit**: Completed audit of production execution and settlement paths. Verified mainnet-only guardrails for critical routes.
- **Compliance Hardening**: Verified TEE-based proposal enforcement and 144-block institutional timelock logic. Verified Multi-Currency Jurisdictional Sharding implementation boundaries.
- **Portfolio Mapping**: Created canonical portfolio map in `docs/PORTFOLIO_MAP.md`.
- **Readiness Gates**: Defined repository-scoped readiness criteria in `docs/READINESS_GATES.md`. Branch promotion policy (`dev` as integration, `main` as mainnet-only) enforced.
- **SAB Migration**: Mapped Web2 dependencies and sovereign target states in `docs/SAB_MIGRATION.md`.

- **R33 (ERP Sync)**: Replaced simulation with robust OData v4 field extraction logic supporting SAP/Oracle institutional standards, including x402 industrial intent and sector-specific metadata.
- **R38 (Phase 7 Verifications)**: Integrated 364-segment BitVM2 SNARK checkpoints and Global Jurisdictional Sharding logic to track fiat and token compliance rules.
