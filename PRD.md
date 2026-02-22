# Product Requirements Document (PRD): Conxian Gateway

## 1. Executive Summary
The Conxian Gateway is an institutional-grade middleware for Bitcoin/Stacks state logic and enterprise compliance.

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

## 4. Technical Implementation Details
- **Bitcoin Engine**: Uses `bitcoincore-rpc` for state monitoring. Includes a `BitcoinRpc` trait for improved testability and mocking.
- **Stacks Engine**: Uses `StacksRpcClient` for real-time state monitoring via Stacks Node API. Nakamoto-ready with epoch signaling.
- **ZKC Module**: Implements robust attestation validation using `secp256k1` ECDSA and Schnorr signatures.
- **Security**: Institutional API is protected by Bearer token authentication.
- **Metrics**: Exposes internal state, uptime, and request counters via a Prometheus-compatible endpoint.
- **Persistence**: Simple file-based persistence for cross-restart reliability.
- **Testing**: Comprehensive unit and integration tests covering API, Compliance, and Engine.
