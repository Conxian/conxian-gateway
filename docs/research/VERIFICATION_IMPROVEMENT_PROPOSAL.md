# Improvement Proposal: Universal Chain Verification (UCV-1)

## 1. Problem Statement
The current Conxian Gateway verification logic is fragmented across multiple handlers and specific verification methods (BitVM, ZKC, TEE). As we expand to multi-chain adapters (Liquid, Rootstock, Babylon), we need a unified verification interface that can handle heterogeneous proof types.

## 2. Proposed Solution
Implement a `UniversalVerifier` service that utilizes the `ChainAdapter` trait to delegate verification to the appropriate chain family.

### Key Components:
- **Proof Registry**: A central registry of known proof types and their associated verifiers.
- **Adapter Delegation**: The `UniversalVerifier` identifies the target chain from the proof metadata and calls `adapter.verify_state_proof()`.
- **Trust Policy Integration**: Automatically apply `TrustPolicy` decisions based on the verification result and the adapter's trust tier.

## 3. Implementation Plan
1. Refactor `internal/compliance/src/zkc.rs` to extract general-purpose verification logic into a `CoreVerifier` trait.
2. Update `AppState` to include the `UniversalVerifier`.
3. Migrate `/api/v1/verify` to use the `UniversalVerifier` for all proof types.

## 4. Expected Outcomes
- Reduced duplication in verification logic.
- Faster integration of new chain families.
- Consistent trust-tier enforcement across the entire gateway.
