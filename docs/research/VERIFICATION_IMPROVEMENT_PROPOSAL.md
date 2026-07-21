# Improvement Proposal: Universal Chain Verification (UCV-1)

## 1. Problem Statement
The current Conxian Gateway verification logic is fragmented across multiple handlers and adapter-level verification surfaces (including BitVM metadata handling, ZKC, and TEE). As we expand to multi-chain adapters (Liquid, Rootstock, Babylon), we need a unified verification interface that can handle heterogeneous proof types.

## 2. Proposed Solution
Implement a `UniversalVerifier` service that utilizes the `ChainAdapter` trait to delegate verification to the appropriate chain family.

### Key Components:
- **Proof Registry**: A central registry of known proof types and their associated verifiers.
- **Adapter Delegation**: The `UniversalVerifier` identifies the target chain from the proof metadata and calls `adapter.verify_state_proof()`; this is generic adapter dispatch, not cryptographic proof verification by itself.
- **Trust Policy Integration**: Automatically apply `TrustPolicy` decisions based on the verification result and the adapter's trust tier.

## 3. Implementation (Completed 2026-06-18)
1. **Refactored compliance layer**: Extracted general-purpose verification logic into a `CoreVerifier` trait in `internal/compliance/src/verifier.rs`.
2. **Implemented UniversalVerifier**: A service that manages chain adapters and delegates heterogeneous proof verification.
3. **API Integration**: Added `POST /api/v1/chains/{chain}/verify` to `internal/api/src/routes.rs` and implemented the handler in `handlers.rs`.
4. **SDK & Schemas**: Updated `@conxian/client-sdk` and `@conxian/schemas` to support universal verification requests.
5. **Hardened X402 Middleware**: Updated middleware to correctly route and validate payments for new heterogeneous verification endpoints.

## 4. Expected Outcomes
- Reduced duplication in verification logic.
- Faster integration of new chain families (Liquid and Rootstock adapters are wired into the generic adapter-dispatch path; this does not by itself imply cryptographic proof verification).
- Consistent trust-tier enforcement across the entire gateway.

### 2026-06-18 Alignment Update
- **Wasm-First Alignment**: Verified that `@conxian/client-sdk` and `@conxian/schemas` build correctly in the Next.js 14 environment.
- **SSR Safety**: Hardened the Control-Plane UI to ensure server-side rendering does not bypass client-side verification gates.

### 2026-06-19 UCV-1 Hardening Update
- **Partner Lane Wiring**: Babylon and BitVM adapters are registered in the generic `UniversalVerifier` dispatch path. Babylon's configured path performs bounded BTC-height recency checks while its unconfigured path retains rehearsal-mode proof-type acceptance; BitVM's legacy `verify_state_proof()` remains metadata-only.
- **Verification Logic**: Babylon's `verify_state_proof()` is a rehearsal/metadata surface: with a configured header source it checks only bounded `btc_height` recency (EOTS and full finality verification remain out of scope), and without one it accepts the `finality_gadget` proof type. BitVM's generic `verify_state_proof()` only checks for `root_hash`; the separate Groth16 envelope path validates the backend-neutral request and delegates to an injected `Groth16Verifier`. The checked-in fixture mock is non-cryptographic, and no production pairing backend or settlement authorization is wired.
- **Integration Tests**: Added automated API integration tests covering heterogeneous adapter dispatch and the current Babylon rehearsal/metadata and BitVM metadata behaviors.

### 2026-06-26 Full Protocol Alignment
- **MuSig2 & DLC**: Integrated BIP-327 and Discreet Log Contract primitives into the universal verification surface.
- **BitVM3 / BitVMX boundary**: No production `UniversalVerifier` wiring exists for BitVM3, BitVMX-GC, or recursive proof verification. BitVMX-CPU is limited to the isolated evaluation lane, while the BitVM Groth16 path accepts an injected backend and the checked-in mock is fixture-only and non-cryptographic; no production pairing backend or settlement authorization is wired. See [`BITVM3_BITVMX_RESEARCH_EXPANSION.md`](./BITVM3_BITVMX_RESEARCH_EXPANSION.md) for the evidence matrix and promotion gates.
