# Opportunity Mapping & Research Expansion (2026-09-04)

This document expands on existing research and maps emerging opportunities for the Conxian Gateway stack. **Updated with Canton Network eUTXO / CBTC non-custodial reserve attestation research, BRICS+ financial systems, and client-side Wasm verification.**

## 1. Emerging Protocol Opportunities

### A. BitVM3, BitVMX, and Recursive Proof Research (Expansion of SSV-1)
- **Status**: Research / Evaluation Only (Fail-Closed)
- **Canonical evidence**: [`BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md`](./BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md); [`BITVM3_BITVMX_RESEARCH_EXPANSION.md`](./BITVM3_BITVMX_RESEARCH_EXPANSION.md).
- **Current position**: BitVM3 is a paper/protocol family centered on garbled-circuit-based off-chain verification. It is not a recursive Groth16 SDK or a Conxian dependency. BitVMX-CPU is limited to the isolated [`tools/bitvmx-eval`](../../tools/bitvmx-eval/) lane; BitVMX-GC and GOATNetwork/`bitvm2-gc` remain research/reference targets.
- **Expansion**:
    - Maintain an evidence matrix with exact upstream revisions, license signals, resource claims, reproducibility status, and explicit confidence.
    - Keep the existing `Groth16Verifier` boundary backend-neutral; do not wire `UniversalVerifier`, settlement, or compliance paths to an unreviewed proof or GC implementation.
    - Treat recursive SNARK/IVC systems such as Nova as a separate comparison track rather than an interchangeable BitVM3 or BitVMX component.
    - Promote only after license, stable revision, reproducible build, independent positive/negative vectors, resource fit, process/network isolation, proof/key formats, and security-review gates pass.

### B. Local-First (Wasm) UCV-1 Verification
- **Status**: Experimental / SDK Expansion
- **Opportunity**: Moving verification to the client (`@conxian/client-sdk` / Wasm) improves latency, privacy, and zero-trust verification.
- **Expansion**:
    - Audit `pkg/conxian-core` for `no_std` compatibility to support Wasm compilation.
    - Research a "Verified Lite-Client" mode for the SDK where the client verifies Stacks Nakamoto and Bitcoin L1 header proofs locally using the Gateway only for data availability.

### C. Canton Network Interoperability & CBTC Non-Custodial Reserve Verification
- **Status**: Active Candidate Initiation (Candidate I — Score 9.6)
- **Opportunity**: Canton Network is a privacy-enabled institutional DLT powering $6T+ in tokenized RWAs across Goldman Sachs, BNP Paribas, Deutsche Börse. CBTC (BitSafe wrapped Bitcoin on Canton) represents wrapped Bitcoin reserves. Conxian provides non-custodial, zero-custody verification of CBTC reserve attestations.
- **Expansion**:
    - **G-C1 (Candidate I)**: CBTC non-custodial attestation verification — Verify FROST threshold Schnorr attestation signatures and Bitcoin L1 UTXO reserve proofs for Canton-wrapped BTC without joining the signer set or holding custody.
    - **G-C4 (Candidate J)**: Canton state translation adapter — Map Daml Active Contract Set (ACS) changes to Universal Contract References (UCR) anchored on Bitcoin. Observe-only, never run a Canton validator.
- **Sovereignty Alignment**: ✅ Observe & verify only, zero custody, zero validator overhead.

### D. ISO 20022 camt.* & CIPS/mBridge Expansion
- **Status**: Active (G-FI1 & G-FI2 Shipped)
- **Opportunity**: Move beyond payment initiation (`pacs.008`) to full treasury statement reporting (`camt.053` Bank-to-Customer Statement).
- **Expansion**:
    - Map `TreasuryMonitor` events to `camt.053` XML messages for automated audit ingestion into institutional ERPs (SAP, Oracle, OData v4).
    - Maintain dual-stack normalization across Western SWIFT/ISO 20022 and BRICS CIPS/mBridge payment formats.

---

## 2. Strategic Priority Matrix

1. **Candidate I**: CBTC Non-Custodial Reserve Verification (Canton wrapped BTC threshold attestation + L1 UTXO proof check) — Score 9.6.
2. **Candidate J**: Canton State Translation Adapter (Daml ACS anchor → Bitcoin UCR) — Score 8.0.
3. **Candidate G**: Machine Identity & peaq DLT Adapter — Score 7.8.
4. **Candidate H**: M2M Lightning Settlement Rail — Score 7.5.

### E. Candidate Q - Wasm UCV-1, BitVM3 Folding & mBridge Node Deployment (Detailed Architecture)
- **Status**: Candidate Q Initiated (Score 9.4)
- **Detailed Specifications**:
    - **Wasm UCV-1 Execution Engine**: Compiles `conxian_core` cryptographic verification subroutines to `wasm32-unknown-unknown` for `@conxian/client-sdk`, enabling client-side zero-trust verification of Schnorr signatures, ISO 20022 XML payloads, and Bitcoin Merkle roots.
    - **BitVM3 Garbled-Circuit Folding**: Implements recursive SNARK/garbled circuit accumulator folding target <200,000 cycles for BitVM3 challenge-response fraud proofs.
    - **BRICS mBridge Observer Deployment**: Non-custodial containerized sidecar spec (`brics-mbridge-observer`) for parsing mBridge DLT state commitments and normalizing ISO 20022 payloads across cross-border trade corridors.
