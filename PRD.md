# Product Requirements Document (PRD): Conxian Gateway

## 1. Executive Summary
The Conxian Gateway is an institutional-grade middleware for Bitcoin/Stacks state logic and enterprise compliance. It serves as the primary "Compliance Pipe" for B2B fintech integrations, ensuring secure, SLA-grade interfaces and cryptographic state verification.

## 2. Requirements Tracking
- [x] R1: Rust Workspace Initialization (Status: Complete)
- [x] R2: Bitcoin State Engine Porting (Status: Complete)
- [x] R3: Stacks State Engine Porting (Status: Complete)
- [x] R4: Institutional API & Auth Porting (Status: Complete)
- [x] R5: ZKC Module Implementation (Status: Enhanced with Schnorr/Taproot support)
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
- [x] R21: MVCR (Mathematically Verifiable Compliance Reports) Generation (Status: Complete)
- [x] R22: Institutional Treasury Monitoring (Status: Complete)

## 3. Progress Log
- 2026-02-13: Initialized workspace structure.
- 2026-02-13: Ported Bitcoin state engine (RPC and basic listener).
- 2026-02-13: Ported Stacks (Nakamoto) state engine structure.
- 2026-02-13: Ported Institutional API and Auth layer (Axum based).
- 2026-02-13: Implemented ZKC Module for Conxius Wallet attestation.
- 2026-02-13: Finalized documentation and entry point.
- 2026-02-13: Enhanced ZKC with secp256k1 verification.
- 2026-02-13: Implemented real Stacks RPC client.
- 2026-02-13: Added graceful shutdown and improved state monitoring.
- 2026-02-20: Aligned `conxian-core` with latest research:
    - Moved `Attestation` to core library for better interoperability.
    - Added support for Schnorr/Taproot-ready attestations.
    - Integrated Nakamoto-specific state signaling in Stacks listener.
    - Standardized error reporting and versioning.
- 2026-02-21: Maintenance and Enhancements:
    - Fixed clippy warnings in API module.
    - Implemented Unified ZKC API supporting both ECDSA and Schnorr attestations.
    - Improved API error handling with appropriate HTTP status codes (400 for verification failures).
    - Added basic Prometheus-compatible metrics support (`/api/v1/metrics`).
- 2026-02-22: System-wide Review and Repairs:
    - Fixed formatting issues across the entire workspace (`cargo fmt`).
    - Implemented a Persistence layer for saving/loading gateway state (Bitcoin/Stacks heights).
    - Enhanced Stacks RPC to fetch real network and Nakamoto epoch information from Hiro API.
    - Added uptime and detailed request counters to Prometheus metrics and state API.
    - Refactored listeners to use the persistence layer for reliable state monitoring across restarts.
- 2026-02-23: Advanced System Review and Enhanced Reliability:
    - Enhanced `ChainState` to include `burn_block_height` for better Bitcoin/Stacks cross-referencing.
    - Upgraded `StacksRpcClient` to fetch and parse `burn_block_height` from Hiro API.
    - Improved Prometheus metrics formatting for better compatibility with standard scrapers.
    - Implemented atomic write mechanism in `FilePersistence` to prevent data corruption.
    - Conducted a full system audit and verified all modules pass rigorous testing and clippy checks.
- 2026-02-24: Observability and Health Enhancements:
    - Expanded `Metrics` struct with detailed per-endpoint request counters.
    - Implemented fine-grained tracking for attestation verification results (success vs failure).
    - Enhanced `/api/v1/health` to report "degraded" status if any underlying chain listener encounters errors, including specific error details.
    - Updated Prometheus metrics endpoint to expose the new granular telemetry data.
    - Improved internal logging in Bitcoin and Stacks listeners for better operational visibility.
- 2026-02-25: Institutional Observability Upgrade:
    - Added configurable sync intervals for Bitcoin and Stacks listeners to optimize resource usage.
    - Implemented `last_sync_time` tracking for all blockchain listeners.
    - Enhanced health monitoring with stale sync detection (flagging status as degraded if sync is delayed).
    - Added granular logging for ZKC verification types (ECDSA vs Schnorr) to support compliance auditing.
    - Expanded Prometheus metrics to include last successful sync timestamps for all chains.
- 2026-03-22: Sovereign/Institutional Realignment (Jules):
    - Aligned codebase with the "Sovereign/Institutional" ethos and standardized naming.
    - Enhanced ZKC module with ZKML proof verification and MVCR generation.
    - Implemented Treasury Monitor for institutional balance tracking and Sovereign Tax (1%) simulation.
    - Expanded Prometheus metrics with treasury-specific gauges.
    - Formalized development standards with a new `AGENTS.md`.

## 4. Technical Implementation Details
- **Bitcoin Engine**: Uses `bitcoincore-rpc` for state monitoring. Includes a `BitcoinRpc` trait for improved testability and mocking.
- **Stacks Engine**: Uses `StacksRpcClient` for real-time state monitoring via Stacks Node API. Nakamoto-ready with epoch signaling and burn block height tracking.
- **Treasury Engine**: Monitors institutional balances and simulates automated tax extraction.
- **ZKC Module**: Implements robust attestation validation using `secp256k1` ECDSA and Schnorr signatures. Supports ZKML proof verification.
- **MVCR Generator**: Provides Mathematically Verifiable Compliance Reports for institutional state attestation.
- **Security**: Institutional API is protected by Bearer token authentication.
- **Metrics**: Exposes internal state, uptime, request counters, and treasury balances via a Prometheus-compatible endpoint.
- **Persistence**: File-based state persistence with atomic write guarantees.
