# Universal Chain Verification (UCV-1) & API Hardening

## Overview
This PR implements the Universal Chain Verification (UCV-1) architecture, hardens the X402 payment-required middleware, and fixes institutional API handlers affected by schema drift.

## Changes

### 1. Compliance & Verification (UCV-1)
- **Trait Definition**: Added `CoreVerifier` trait in `internal/compliance/src/verifier.rs` to standardize proof verification across heterogeneous chains.
- **Universal Verifier**: Implemented `UniversalVerifier` which delegates to specific verifiers (e.g., `ZkcVerifier`) based on chain or proof type.
- **ZKC Integration**: Integrated `ZkcVerifier` into the core verification flow by implementing the `CoreVerifier` trait.

### 2. API Layer
- **New Endpoint**: Added `POST /api/v1/chains/{chain}/verify` for unified verification requests.
- **AppState Integration**: Injected `Arc<dyn CoreVerifier>` into `AppState` and updated `gateway` main to initialize it.
- **X402 Hardening**: Refactored `x402.rs` to support both numeric and string-encoded amounts (Satoshi precision) and hardened path protection for `/api/v1` and `/admin/v1`.
- **Handler Fixes**: Resolved type mismatches in ALEX swapping, A2P OTP verification, and ISO 20022 settlement handlers.

### 3. SDK & Shared Libraries
- **Client SDK**: Added `verifyStateProof` method to `ConxianClient`.
- **Schemas**: Extended `packages/schemas/index.ts` with `VerificationRequest` and `VerificationResponse` interfaces.

### 4. Quality & Documentation
- **Tests**: Verified all 80+ unit and integration tests pass.
- **Clippy**: Resolved regressions including "items after test module" and borrow mismatches.
- **Research**: Updated `ENHANCEMENT_PLAN.md`, `VERIFICATION_IMPROVEMENT_PROPOSAL.md`, and `UNIVERSAL_CHAIN_RESEARCH.md`.
- **Changelog**: Formalized 0.1.4 updates.
