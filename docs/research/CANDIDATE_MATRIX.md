# Conxian Gateway: Candidate Maturity & Scoring Matrix

This matrix tracks the maturity of core components and identifies the best candidates for next-phase implementation based on urgency, technical readiness, and institutional demand. **Updated 2026-09-06 with Machine Economy (peaq DLT / Candidate R), Canton CCIP Gateway (Candidate S), Wasm UCV-1 Client Engine (Candidate Q), and ISO 20022 camt.053 ERP Treasury Reporting (Candidate T).**

## 1. Component Maturity Scoring (0-10)

| Component | Maturity | Priority | Status | Gap / Evidence |
| :--- | :--- | :--- | :--- | :--- |
| **UCV-1 (Universal Verification)** | 9.8 | Urgent | Production | Multi-chain adapter verification active (Liquid, Stacks, Babylon, Fedimint, Citrea, Strata) |
| **ISO 20022 (`pacs.008`, `camt.053`)** | 9.6 | Urgent | Production | Shipped (G-FI1, G-FI2, G-TR1). Full XML generator, parser & schema validator in `zkc.rs` & `camt.rs` |
| **Wasm UCV-1 Local Verification** | 9.5 | High | Production | Shipped (Candidate Q / G-20, G-21). Local-first zero-trust state proof verification in `@conxian/client-sdk` |
| **Babylon Staking EOTS & Key Extraction** | 9.5 | High | Production | Shipped (G-BB1). Schnorr attestation & double-sign key extraction active in `babylon_adapter.rs` |
| **Fedimint Blind Signature Verification** | 9.3 | High | Production | Shipped (G-FM1). Guardian pubkey Schnorr blind sig verification in `fedimint_adapter.rs` |
| **Canton CCIP Cross-Chain Gateway** | 9.3 | High | Production | Shipped (Candidate S / G-C5). Dynamic risk scoring & ZKC compliance routing active in `canton_m2m.rs` |
| **BRICS mBridge DLT Ingress** | 9.2 | High | Production | Shipped (Candidate P / G-FI3). HotStuff/e-CNY DLT state proof verification in `brics_adapter.rs` |
| **sBTC L1 Proof Verification** | 9.2 | High | Production | Shipped (G-SB3). Double-SHA256 tx & block header PoW verification in `sbtc.rs` |
| **Machine Economy & DePIN Settlement** | 9.2 | High | Initiated | Candidate R (G-ME1, G-ME2). peaq machine identity resolution, RWA revenue attestation & Lightning/X402 settlement active in `canton_m2m.rs` |
| **Canton State Translation Adapter** | 9.0 | High | Production | Shipped (Candidate J / G-C4). Daml ACS contract parsing & state root UCR translation active |
| **CBTC Non-Custodial Reserve Verification** | 9.0 | High | Production | Shipped (Candidate I / G-C1). Threshold Schnorr attestation & UTXO reserve proof check in `dlc_oracle.rs` |
| **BIP-322 Message Signing** | 9.0 | Urgent | Production | Integrated into compliance and identity layer (`zkc.rs`) |
| **SWIFT camt.053 ERP Treasury Reporting** | 8.8 | High | Initiated | Candidate T (G-TR1). Real-time bank-to-customer statement generation & OData v4 synchronization active |
| **DLC Orchestration & Oracle Attestation** | 8.8 | Medium | Production | Shipped (G-DL1, G-DL3). Cryptographic BIP340 Schnorr oracle threshold verification active |
| **Identity Resolution (ENS/Web3.bio/World ID)** | 8.5 | High | Production | Integrated live APIs with fail-closed fallback (`identity.rs`) |
| **Nostr Wallet Connect (NWC)** | 8.0 | High | Production | NIP-47 relay-settle integrated (`nwc_backend.rs`) |
| **MuSig2 Key Aggregation** | 8.0 | High | Production | Primitives and Aggregator active (`zkc.rs`) |
| **BitVM3 / Garbled Circuits** | 2.0 | Medium | Research | Research-only, fail-closed (`bitvm3_adapter.rs`) |

---

## 2. Candidate Portfolio & Ranking

### Candidate I: CBTC Non-Custodial Reserve Verification (Score: 9.6 — Shipped)
- **Status**: ✅ Shipped (G-C1). Implemented `verify_cbtc_reserve_attestation` in `internal/engine/src/bitcoin/dlc_oracle.rs`.
- **Urgency**: High (Q3 2026). CBTC (Canton wrapped Bitcoin) represents institutional reserves across Canton Network ($6T+ RWAs).
- **Impact**: Provides non-custodial, zero-custody threshold Schnorr attestation verification and Bitcoin L1 UTXO reserve proof checks.

### Candidate J: Canton State Translation Adapter (Score: 9.0 — Shipped)
- **Status**: ✅ Shipped (G-C4). Daml Active Contract Set (ACS) commitment parsing, contract ID syntax verification, and state root hash mapping to Bitcoin Universal Contract References (UCR) via `translate_to_ucr` & `/api/v1/canton/state/translate`.

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

### Candidate P: BRICS mBridge & Cross-Border Sovereign Settlement (Score: 9.2 — Shipped)
- **Status**: ✅ Shipped (G-B6, G-FI3). Implemented `MBridgeAdapter::verify_mbridge_dlt_attestation` in `internal/engine/src/brics_adapter.rs` validating HotStuff/e-CNY DLT state proofs and threshold Schnorr consensus signatures. Enhanced `normalize_mbridge_ingress` in `internal/compliance/src/zkc.rs` and exposed `/api/v1/ingress/mbridge` in `internal/api/src/handlers.rs`.

### Candidate Q: Client-Side Wasm UCV-1 & BitVM3 Garbled-Circuit Folding Engine (Score: 9.5 — Shipped)
- **Status**: ✅ Shipped (G-20, G-21, G-B6). Wasm-compatible UCV-1 verification primitives implemented in `@conxian/client-sdk` (`verifyStateProofLocal`) enabling zero-trust client-side verification, alongside sub-200,000 cycle recursive Groth16/garbled-circuit proof folding for BitVM3 state transitions.

### Candidate R: Machine Economy & DePIN peaq DLT Settlement Engine (Score: 9.2 — Initiated)
- **Status**: 🚀 Initiated (G-ME1, G-ME2). peaq DLT machine identity resolution (`resolve_machine_identity`), machine RWA revenue attestation (`verify_machine_rwa_revenue`), and machine-to-machine (M2M) Lightning/X402 micro-settlement implemented in `canton_m2m.rs` and exposed via `/api/v1/m2m/settle` and `/api/v1/m2m/rwa/verify`.
- **Urgency**: High (Q4 2026). Essential for autonomous machine agents, solar/telecom DePIN sensors, and smart mobility fleets requiring instant micropayments and revenue tokenization.
- **Impact**: Bridges IoT machine telemetry directly to Bitcoin/Lightning settlement rails and Canton tokenized asset contracts.

### Candidate S: Canton CCIP Cross-Chain Message Gateway (Score: 9.3 — Shipped)
- **Status**: ✅ Shipped (G-C5). Chainlink CCIP message verification and dynamic risk-scoring routing engine (`route_ccip_message`) implemented in `canton_m2m.rs` and exposed via `/api/v1/ccip/route`.

### Candidate T: SWIFT camt.053 Real-Time Bank Treasury Reporting (Score: 9.0 — Initiated)
- **Status**: 🚀 Initiated (G-TR1). Implemented `camt.053.001.10` Bank-to-Customer Statement XML builder in `internal/api/src/camt.rs` mapping `TreasuryMonitor` events to institutional ERP systems (SAP, Oracle) via OData v4 ledger synchronization.

---

## 3. Recommended Roadmap Execution

With Candidates I through S shipped and Candidates Q, R, T active, the next development cycles will focus on expanding Wasm UCV-1 verification bindings in `@conxian/client-sdk`, extending peaq DLT machine DID verification across multi-chain DePIN networks (DIMO, peaq, Helium), and completing OData v4 ERP webhook callbacks.
