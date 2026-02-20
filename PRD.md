# Product Requirements Document (PRD): Conxian Gateway

## 1. Executive Summary
The Conxian Gateway is an institutional-grade middleware for Bitcoin/Stacks state logic and enterprise compliance.

## 2. Requirements Tracking
- [x] R1: Rust Workspace Initialization (Status: Complete)
- [x] R2: Bitcoin State Engine Porting (Status: Complete)
- [x] R3: Stacks State Engine Porting (Status: Complete)
- [x] R4: Institutional API & Auth Porting (Status: Complete)
- [x] R5: ZKC Module Implementation (Status: Complete)
- [x] R6: Audit-Ready Cleanliness (Status: Complete)
- [x] R7: Robustness & Graceful Shutdown (Status: Complete)

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

## 4. Technical Implementation Details
- **Bitcoin Engine**: Uses `bitcoincore-rpc` for state monitoring. Includes a `BitcoinRpc` trait for improved testability and mocking.
- **Stacks Engine**: Uses `StacksRpcClient` for real-time state monitoring via Stacks Node API.
- **ZKC Module**: Implements robust attestation validation using `secp256k1` ECDSA signatures.
- **Security**: Institutional API is protected by Bearer token authentication.
- **Testing**: Comprehensive unit and integration tests.
