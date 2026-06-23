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
| **Identity Resolution (World ID)** | 3 | Medium | Placeholder | Needs real verifier integration |
| **Nostr Wallet Connect (NWC)** | 0 | High | Triage | Implementation planned |

## 2. Best Candidates for Implementation

### Candidate A: Identity Resolver (World ID) Hardening (Score: 8.0)
- **Urgency**: Medium.
- **Readiness**: High. Framework is already in place; requires connecting to World ID SDK or API.
- **Impact**: Moves the remaining "Compliance Pipe" from simulation to live observability.

### Candidate B: Nostr Wallet Connect (NWC) (Score: 7.5)
- **Urgency**: High (CON-1267).
- **Readiness**: Medium. Requires Nostr protocol support.
- **Impact**: Enables non-custodial authorization of Lightning payments in the dashboard.

## 3. Recommended Initiation
Initiate **Candidate A (World ID)** to complete the identity resolution suite, followed by **Candidate B (NWC)** to enhance the non-custodial payment experience.
