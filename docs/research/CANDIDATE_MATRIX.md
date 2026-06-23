# Conxian Gateway: Candidate Maturity & Scoring Matrix

This matrix tracks the maturity of core components and identifies the best candidates for next-phase implementation based on urgency, technical readiness, and institutional demand.

## 1. Maturity Scoring (0-10)

| Component | Maturity | Priority | Status | Gap |
| :--- | :--- | :--- | :--- | :--- |
| **UCV-1 (Universal Verification)** | 9 | Urgent | Production | None (Hardened) |
| **BIP-322 Message Signing** | 9 | Urgent | Production | Integrated into Identity API |
| **ALEX Swap Integration** | 8 | High | Production | Signer Enclave cutover pending |
| **Identity Resolution (ENS/Web3.bio)** | 8 | High | Production | Integrated live APIs |
| **DLC Orchestration** | 7 | Medium | Production | Primitives mapped to USI |
| **MuSig2 Aggregation** | 6 | High | Production | Primitives and Aggregator active |
| **Mempool Orchestrator** | 7 | High | Production | Industrial Intent integration |
| **Identity Resolution (BNS)** | 7 | Medium | Production | Full resolver active with RPC fallback |
| **Identity Resolution (World ID)** | 4 | High | Development | Transitioning from placeholder to live API |
| **Blake2s (Ark Alignment)** | 2 | High | Research | Required for V-UTXO PRF (CON-1282) |
| **Silent Payments (BIP-352)** | 1 | High | Research | Native scanning integration planned (CON-1281) |
| **Nostr Wallet Connect (NWC)** | 1 | High | Research | Protocol transport defined (CON-1267) |

## 2. Best Candidates for Implementation

### Candidate A: Identity Resolver (World ID) Hardening (Score: 8.5)
- **Urgency**: High (CON-1284).
- **Readiness**: High. API endpoint and request structure researched; framework ready.
- **Impact**: Completes the Tier 1 identity resolution suite.

### Candidate B: Blake2s for Ark Protocol (Score: 7.8)
- **Urgency**: High (CON-1282).
- **Readiness**: High. Deterministic hashing required for V-UTXO; implementation is self-contained.
- **Impact**: Unblocks Ark protocol compliance and recovery model.

### Candidate C: Nostr Wallet Connect (NWC) (Score: 7.5)
- **Urgency**: High (CON-1267).
- **Readiness**: Medium. Requires NIP-47 transport logic.
- **Impact**: Enables non-custodial authorization of Lightning payments.

## 3. Recommended Initiation
Initiate **Candidate A (World ID)** immediately to close the identity gap, followed by **Candidate B (Blake2s)** to align with Ark specifications.
