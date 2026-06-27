# Improvement Proposal: Universal Chain Verification (UCV-1)

## 1. Problem Statement
The current Conxian Gateway verification logic is fragmented across multiple handlers and specific verification methods (BitVM, ZKC, TEE). As we expand to multi-chain adapters (Liquid, Rootstock, Babylon), we need a unified verification interface that can handle heterogeneous proof types.

## 2. Proposed Solution
Implement a `UniversalVerifier` service that utilizes the `ChainAdapter` trait to delegate verification to the appropriate chain family.

### Key Components:
- **Proof Registry**: A central registry of known proof types and their associated verifiers.
- **Adapter Delegation**: The `UniversalVerifier` identifies the target chain from the proof metadata and calls `adapter.verify_state_proof()`.
- **Trust Policy Integration**: Automatically apply `TrustPolicy` decisions based on the verification result and the adapter's trust tier.

## 3. Implementation (Completed 2026-06-18)
1. **Refactored compliance layer**: Extracted general-purpose verification logic into a `CoreVerifier` trait in `internal/compliance/src/verifier.rs`.
2. **Implemented UniversalVerifier**: A service that manages chain adapters and delegates heterogeneous proof verification.
3. **API Integration**: Added `POST /api/v1/chains/{chain}/verify` to `internal/api/src/routes.rs` and implemented the handler in `handlers.rs`.
4. **SDK & Schemas**: Updated `@conxian/client-sdk` and `@conxian/schemas` to support universal verification requests.
5. **Hardened X402 Middleware**: Updated middleware to correctly route and validate payments for new heterogeneous verification endpoints.

## 4. Expected Outcomes
- Reduced duplication in verification logic.
- Faster integration of new chain families (Liquid and Rootstock adapters are now fully integrated into the verification pipeline).
- Consistent trust-tier enforcement across the entire gateway.

### 2026-06-18 Alignment Update
- **Wasm-First Alignment**: Verified that `@conxian/client-sdk` and `@conxian/schemas` build correctly in the Next.js 14 environment.
- **SSR Safety**: Hardened the Control-Plane UI to ensure server-side rendering does not bypass client-side verification gates.

### 2026-06-19 UCV-1 Hardening Update
- **Partner Lane Expansion**: Fully integrated Babylon and BitVM adapters into the `UniversalVerifier` pipeline.
- **Verification Logic**: Implemented rehearsal-mode verification for Babylon finality gadget proofs and BitVM2 optimistic state-root proofs.
- **Integration Tests**: Added automated API integration tests covering heterogeneous proof verification for Babylon and BitVM chains.

### 2026-06-26 Full Protocol Alignment
- **MuSig2 & DLC**: Integrated BIP-327 and Discreet Log Contract primitives into the universal verification surface.
- **Adaptive Proofs**: Prepared the `UniversalVerifier` to consume BitVM3 recursive proofs and BitVMX adaptive computation proofs.
