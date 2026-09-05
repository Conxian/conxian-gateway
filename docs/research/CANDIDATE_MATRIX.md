# Conxian Gateway: Candidate Maturity & Scoring Matrix

This matrix tracks the maturity of core components and identifies the best candidates for next-phase implementation based on urgency, technical readiness, and institutional demand. **Updated 2026-09-04 with CBTC non-custodial reserve verification, Canton Daml state translation, multi-chain settlement rails, and BRICS financial system integration.**

## 1. Component Maturity Scoring (0-10)

| Component | Maturity | Priority | Status | Gap / Evidence |
| :--- | :--- | :--- | :--- | :--- |
| **UCV-1 (Universal Verification)** | 9.5 | Urgent | Production | Multi-chain adapter verification active (Liquid, Stacks, Babylon, Fedimint, Citrea, Strata) |
| **ISO 20022 Payment Initiation (`pacs.008`)** | 9.5 | Urgent | Production | Shipped (G-FI1 & G-FI2). Full XML generator & schema validator in `zkc.rs` |
| **Babylon Staking EOTS & Key Extraction** | 9.5 | High | Production | Shipped (G-BB1). Schnorr attestation & key extraction active in `babylon_adapter.rs` |
| **Fedimint Blind Signature Verification** | 9.3 | High | Production | Shipped (G-FM1). Guardian pubkey Schnorr blind sig verification in `fedimint_adapter.rs` |
| **sBTC L1 Proof Verification** | 9.2 | High | Production | Shipped (G-SB3). Double-SHA256 tx & block header PoW verification in `sbtc.rs` |
| **BIP-322 Message Signing** | 9.0 | Urgent | Production | Integrated into compliance and identity layer (`zkc.rs`) |
| **CBTC Non-Custodial Reserve Verification** | 9.0 | High | Production | Initiated (Candidate I). Threshold Schnorr attestation & UTXO reserve proof check in `dlc_oracle.rs` |
| **Canton State Translation Adapter** | 8.8 | High | Production | Initiated (Candidate J / G-C4). Daml ACS contract parsing & state root UCR translation active |
| **DLC Orchestration & Oracle Attestation** | 8.8 | Medium | Production | Shipped (G-DL1). Cryptographic BIP340 Schnorr oracle verification active |
| **Identity Resolution (ENS/Web3.bio/World ID)** | 8.5 | High | Production | Integrated live APIs with fail-closed fallback (`identity.rs`) |
| **Nostr Wallet Connect (NWC)** | 8.0 | High | Production | NIP-47 relay-settle integrated (`nwc_backend.rs`) |
| **MuSig2 Key Aggregation** | 8.0 | High | Production | Primitives and Aggregator active (`zkc.rs`) |
| **BitVM3 / Garbled Circuits** | 2.0 | Medium | Research | Research-only, fail-closed (`bitvm3_adapter.rs`) |

---

## 2. Candidate Portfolio & Ranking

### Candidate I: CBTC Non-Custodial Reserve Verification (Score: 9.6 — Initiated)
- **Status**: 🟢 Initiated / Active. Implemented `verify_cbtc_reserve_attestation` in `internal/engine/src/bitcoin/dlc_oracle.rs`.
- **Urgency**: High (G-C1, Q3 2026). CBTC (Canton wrapped Bitcoin) represents institutional reserves across Canton Network ($6T+ RWAs).
- **Impact**: Provides non-custodial, zero-custody threshold Schnorr attestation verification and Bitcoin L1 UTXO reserve proof checks.

### Candidate J: Canton State Translation Adapter (Score: 8.8 — Initiated)
- **Status**: 🟢 Initiated / Active. Daml Active Contract Set (ACS) commitment parsing, contract ID syntax verification, and state root hash mapping to Bitcoin Universal Contract References (UCR).

### Candidate K: ISO 20022 XML Schema Validation (Score: 9.0 — Shipped)
- **Status**: ✅ Shipped (G-FI1). Structural XML validation for pacs.008, pacs.009, and camt messages in `internal/compliance/src/zkc.rs`.

### Candidate L: ISO 20022 pacs.008 Payment Initiation (Score: 9.2 — Shipped)
- **Status**: ✅ Shipped (G-FI2). FI-to-FI Customer Credit Transfer XML builder and compliance normalization in `zkc.rs` & `/api/v1/iso20022/payment`.

### Candidate M: Babylon EOTS Verification & Double-Sign Key Extraction (Score: 9.5 — Shipped)
- **Status**: ✅ Shipped (G-BB1). Schnorr attestation verification and double-sign key extraction $x = (s_1 - s_2)/(e_1 - e_2) \pmod n$ in `babylon_adapter.rs`.

### Candidate N: Fedimint Cryptographic Blind Signature Verification (Score: 9.3 — Shipped)
- **Status**: ✅ Shipped (G-FM1). Schnorr blind signature verification against guardian x-only pubkeys in `fedimint_adapter.rs`.

### Candidate O: sBTC Bitcoin L1 Proof Verification (Score: 9.1 — Shipped)
- **Status**: ✅ Shipped (G-SB3). Double-SHA256 tx hashing and 80-byte header PoW verification in `sbtc.rs`.

---

## 3. Recommended Roadmap Execution

With Candidates K, L, M, N, O, A, B, C, D, E, F shipped and Candidates I & J active, the next development cycles will focus on client Wasm UCV-1 verification, mBridge validator node research, and BitVM3 recursive proof optimization.


### Candidate P: BRICS mBridge & Cross-Border Sovereign Settlement (Score: 9.2 — Shipped)
- **Status**: ✅ Shipped (G-B6, G-FI3). Implemented `MBridgeAdapter::verify_mbridge_dlt_attestation` in `internal/engine/src/brics_adapter.rs` validating HotStuff/e-CNY DLT state proofs and threshold Schnorr consensus signatures. Enhanced `normalize_mbridge_ingress` in `internal/compliance/src/zkc.rs` and exposed `/api/v1/ingress/mbridge` in `internal/api/src/handlers.rs`.
- **Urgency**: High (Q3/Q4 2026). Strategic expansion into non-SWIFT international trade corridors and sovereign multi-CBDC settlement platforms.
- **Impact**: Enables Conxian Gateway to orchestrate non-custodial atomicity between mBridge ISO 20022 messages and Bitcoin/Lightning liquidity rails.
