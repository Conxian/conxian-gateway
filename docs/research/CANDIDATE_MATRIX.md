# Conxian Gateway: Candidate Maturity & Scoring Matrix

This matrix tracks the maturity of core components and identifies the best candidates for next-phase implementation based on urgency, technical readiness, and institutional demand.

## 1. Maturity Scoring (0-10)

| Component | Maturity | Priority | Status | Gap |
| :--- | :--- | :--- | :--- | :--- |
| **UCV-1 (Universal Verification)** | 9 | Urgent | Production | None (Hardened) |
| **ALEX Swap Integration** | 8 | High | Production | Signer Enclave cutover pending |
| **Mempool Orchestrator** | 7 | High | Production | Industrial Intent integration |
| **Identity Resolution (BNS)** | 6 | Medium | Production | Full resolver active |
| **Identity Resolution (ENS/Other)** | 3 | Medium | Placeholder | Needs real resolver integration |
| **DLC Orchestration** | 2 | Medium | Research | Logic missing from core |
| **BIP-322 Message Signing** | 0 | Urgent | Triage | Implementation planned |
| **MuSig2 Aggregation** | 0 | High | Triage | Implementation planned |
| **Nostr Wallet Connect (NWC)** | 0 | High | Triage | Implementation planned |

## 2. Best Candidates for Implementation

### Candidate A: BIP-322 Universal Message Signing (Score: 9.5)
- **Urgency**: Urgent (CON-1266)
- **Readiness**: High. `bitcoin` crate 0.32.100 has required primitives.
- **Impact**: Enables standardized verification across all address types.

### Candidate B: MuSig2 Signature Aggregation (Score: 8.0)
- **Urgency**: High (CON-1270)
- **Readiness**: Medium. Requires `secp256k1-zkp` or similar for best performance.
- **Impact**: Materially reduces on-chain footprint for institutional multi-sig.

### Candidate C: Identity Resolver Hardening (Score: 7.5)
- **Urgency**: Medium.
- **Readiness**: High. Framework is already in place; requires connecting to public APIs (e.g., ENS, Web3.bio).
- **Impact**: Moves the "Compliance Pipe" from simulation to live observability.

## 3. Recommended Initiation
Initiate **Candidate A (BIP-322)** as it is marked **Urgent** and provides a foundational security layer for all address types, followed by **Candidate C** to close the "mock-to-production" gap in identity resolution.
