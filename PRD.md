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

## 3. Progress Log
- 2026-02-13: Initialized workspace structure.
- 2026-03-22: Sovereign/Institutional Realignment (Jules).
- 2026-03-23: Industry Enhancement Upgrade (Jules):
    - Implemented BitVM attestation verification for trustless state layer.
    - Added ISO 20022 Egress (pacs.008) for institutional banking alignment.
    - Integrated Workload Identity Federation (WIF) for TEE-based enclave authentication.
    - Enhanced Treasury Monitor with sBTC "Suction" simulation and Sovereign Yield Index (SYI).
    - Expanded REST API with identity exchange and ISO payment formatting endpoints.

## 4. Technical Implementation Details
- **TAM Engine**: Simulates growth of sBTC liquidity and tracks the Sovereign Yield Index.
- **WIF Manager**: Handles OIDC-to-GCP token exchange for enclave-signed attestations.
- **BitVM Verifier**: Implements state-root commitment verification for optimistic fraud proofs.
- **ISO 20022 Forge**: Generates standardized XML banking messages for institutional egress.
- **Metrics**: Exposes SYI and sBTC liquidity depth alongside existing blockchain telemetry.
