# Conxian Gateway: Candidate Maturity & Scoring Matrix

This matrix tracks the maturity of core components and identifies the best candidates for next-phase implementation based on urgency, technical readiness, and institutional demand.

## 1. Maturity Scoring (0-10)

| Component | Maturity | Priority | Status | Gap |
| :--- | :--- | :--- | :--- | :--- |
| **UCV-1 (Universal Verification)** | 9 | Urgent | Production | None (Hardened) |
| **BIP-322 Message Signing** | 9 | Urgent | Production | Integrated into Identity API |
| **ALEX Swap Integration** | 8 | High | Production | Signer Enclave cutover pending |
| **Identity Resolution (ENS/Web3.bio)** | 8 | High | Production | Integrated live APIs |
| **DLC Orchestration** | 8 | Medium | Research / Spike | Cryptographic BIP340 Schnorr oracle verification and multi-oracle threshold quorum active; CET construction in research spike. |
| **MuSig2 Aggregation** | 6 | High | Production | Primitives and Aggregator active |
| **Mempool Orchestrator** | 7 | High | Production | Industrial Intent integration |
| **Identity Resolution (BNS)** | 7 | Medium | Production | Full resolver active with RPC fallback |
| **Identity Resolution (World ID)** | 4 | High | Development | Transitioning from placeholder to live API |
| **Blake2s (Ark Alignment)** | 2 | High | Research | Required for V-UTXO PRF (CON-1282) |
| **Silent Payments (BIP-352)** | 1 | High | Research | Native scanning integration planned (CON-1281) |
| **Nostr Wallet Connect (NWC)** | 7 | High | Production | NIP-47 relay-settle integrated; 5 API tests passing. See `internal/api/src/nwc_backend.rs` |

## 2. Best Candidates for Implementation

### Candidate A: Identity Resolver (World ID) Hardening (Score: 8.5)
- **Urgency**: High (CON-1284).
- **Readiness**: High. API endpoint and request structure researched; framework ready.
- **Impact**: Completes the Tier 1 identity resolution suite.

### Candidate B: Blake2s for Ark Protocol (Score: 7.8)
- **Urgency**: High (CON-1282).
- **Readiness**: High. Deterministic hashing required for V-UTXO; implementation is self-contained.
- **Impact**: Unblocks Ark protocol compliance and recovery model.

### Candidate C: Nostr Wallet Connect (NWC) (Score: N/A — Shipped)
- **Status**: ✅ Shipped. NIP-47 relay-settle integrated with 5 passing API tests.
- **Impact**: Enables non-custodial authorization of Lightning payments.

## 2. Best Candidates for Implementation (Continued)

### Candidate D: BRICS Sanctions-Risk Tagging (Implemented Phase 3)
- **Urgency**: Critical (G-B4, Priority 16). Compliance must distinguish SWIFT-linked from CIPS-direct settlement flows.
- **Readiness**: High. `SettlementSource` enum already exists. Adding `SanctionsRisk` classification is type-system work.
- **Impact**: Enables regulatory compliance across G7 and BRICS jurisdictions. Unblocks multi-rail deployment.

### Candidate E: CIPS Message Normalization (Implemented Phase 3)
- **Urgency**: High (G-B1, Priority 12). CIPS processes $24.47T/year. Current BRICS normalization only handles mBridge.
- **Readiness**: Medium. Requires CIPS-specific ISO 20022 extensions research.
- **Impact**: First-mover advantage for CIPS-direct settlement in Bitcoin-native infrastructure.

### Candidate F: Multi-Currency FX Tracking (Implemented Phase 3)
- **Urgency**: Medium-High (G-B2, Priority 8). TreasuryMonitor currently tracks sBTC/BTC only.
- **Readiness**: Medium. ALEX oracle feeds for BRICS FX pairs need research.
- **Impact**: Positions Gateway as multi-currency settlement hub for BRICS corridors (RMB, RUB, INR, AED).

### Candidate G: Machine Identity DID Extension (Score: 7.8)
- **Urgency**: High (G-C2, Q3 2026). Prerequisite for all M2M routing.
- **Readiness**: High. Existing BNS/ENS/World ID stack provides the pattern. peaq DID + device key extension is additive.
- **Impact**: Opens Machine Economy vertical — 500K+ machines on peaq alone.

### Candidate H: Lightning M2M Settlement Primitives (Score: 7.5)
- **Urgency**: High (G-C3, Q3 2026). LN has $1.1B/month volume and USDT support.
- **Readiness**: High. Existing Lightning adapter in preparation phase. SettlementSource extension is type-system work.
- **Impact**: Positions Gateway as routing layer for autonomous machine payments.

### Candidate I: CBTC Non-Custodial Verification (Score: 7.0)
- **Urgency**: High (G-C1, Q3 2026). CBTC is live on Canton today.
- **Readiness**: Medium-Low. A DLC-shaped API/oracle scaffold exists, but cryptographic oracle verification, CET construction, and vector compatibility remain open; see [`DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`](DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md).
- **Impact**: First non-custodial Bitcoin reserve verification for Canton-wrapped BTC.

### Candidate J: Canton State Translation Adapter (Score: 6.5)
- **Urgency**: Medium (G-C4, Q4 2026).
- **Readiness**: Medium-Low. Requires Daml ACS observation capability; Canton observer API status unknown.
- **Impact**: Sovereign routing between $6T+ institutional Canton and Bitcoin.

## 3. Recommended Initiation (Updated 2026-07-06)
Initiate **Candidate D (BRICS Sanctions-Risk Tagging)** was completed in Phase 3 — it's the highest-priority gap (P=16) and is a type-system change with low effort. Follow with **Candidate A (World ID)** to close identity gap, then **Candidate E (CIPS Normalization)** to capture the $24.47T CIPS settlement market. **Candidate B (Blake2s)** aligns with Ark specifications and should follow.

### Canton & Machine Economy Research Basis
Full analysis in `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md`. Two new strategic vectors: (1) Canton Network institutional DLT with $6T+ tokenized RWAs — Conxian routes sovereign capital across the institutional boundary without touching; (2) Machine Economy with peaq (500K+ machines) + Lightning M2M settlements ($1.1B/month) — Conxian provides machine identity, routing, and compliance infrastructure.

**Q3 2026 Priority Candidates**: G (Machine Identity, Score 7.8), H (M2M Lightning, Score 7.5), I (CBTC Verification, Score 7.0). These are high-readiness, high-impact, and fully aligned with Conxian's non-custodial sovereignty ethos.

### BRICS Research Basis
Full financial systems analysis in `docs/research/BRICS_FINANCIAL_SYSTEMS_RESEARCH.md`. The global financial system is bifurcating: Western SWIFT/ISO 20022 (~45% GDP) vs BRICS CIPS/mBridge/SPFS (~40% GDP). The Gateway's dual-stack architecture must support both.


### Candidate K: ISO 20022 XML Schema Validation (Score: 9.0)
- **Status**: ✅ Shipped (G-FI1). Implemented structural XML validation and namespace checking for pacs.008, pacs.009, and camt messages in `internal/compliance/src/zkc.rs`.
- **Impact**: Eliminates silent bank rejection risks and guarantees schema compliance for institutional payment initiation.

### Candidate L: ISO 20022 pacs.008 Payment Initiation (Score: 9.2)
- **Status**: ✅ Shipped (G-FI2). Implemented `pacs.008.001.08` FI-to-FI Customer Credit Transfer XML builder, structural validation, and compliance normalization.
- **Impact**: Enables cross-border payment initiation and settlement envelope construction for institutional banking networks.


### Candidate M: Babylon EOTS Verification & Double-Sign Key Extraction (Score: 9.5)
- **Status**: ✅ Shipped (G-BB1). Implemented Schnorr attestation verification, double-sign detection, and algebraic secret key extraction $x = (s_1 - s_2)/(e_1 - e_2) \pmod n$ in `internal/engine/src/bitcoin/babylon_adapter.rs`.
- **Impact**: Resolves sole remaining P1 gap and enables independent slashability verification for Babylon BTC staking finality providers.


### Candidate N: Fedimint Cryptographic Blind Signature Verification (Score: 9.3)
- **Status**: ✅ Shipped (G-FM1). Implemented Schnorr blind signature verification against guardian x-only public keys in `verify_fedimint_blind_signature()` in `internal/engine/src/bitcoin/fedimint_adapter.rs`.
- **Impact**: Enables cryptographic trust verification for community-governed Chaumian mints.

### Candidate O: sBTC Bitcoin L1 Proof Verification (Score: 9.1)
- **Status**: ✅ Shipped (G-SB3). Implemented raw transaction double-SHA256 hashing and 80-byte header PoW verification in `internal/engine/src/stacks/sbtc.rs`.
- **Impact**: Provides independent Bitcoin L1 verification for sBTC peg operations without relying solely on Emily API responses.
