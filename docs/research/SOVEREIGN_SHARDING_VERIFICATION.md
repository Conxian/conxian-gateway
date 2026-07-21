# Research: Phase 7 Sovereign Labor & Sharding Verification (SSV-1)

## 1. Objective
Define the verification logic for decentralized sovereign sharding, specifically focusing on multi-currency jurisdictional compliance and BitVM2-backed labor proofs.

## 2. Theoretical Framework
Utilizes the **Conxian Unified Theory v2.0** for sharded state consistency across heterogeneous chains.

### Core Components:
- **Labor Attestation**: TEE-signed proofs of work/service completion (Job Cards).
- **Jurisdictional Sharding**: Mapping of settlement rails to regional compliance rules (BRICS, PAPSS) verified via UCV-1.
- **State Root Aggregation**: A research concept for checkpointing sharded state. No segment count or benchmark is verified for the Gateway.

## 3. Implementation Path
1. **ZkcVerifier Expansion**: Integrated with Tableland for decentralized state commit (SovereignCommit trait).
2. **Multi-Source Ingress**: Normalization of ISO 20022, PAPSS, and BRICS payloads into a unified settlement envelope.
3. **Optimistic Fraud Proofs**: BitVM2 adapter implements challenge-response logic for labor settlement roots.

## 4. Institutional Alignment
- **OData v4 ERP Sync**: Automatic field extraction for SAP/Oracle audit alignment.
- **Fail-Closed Readiness**: All labor settlements require context-aware BitVM attestations (CON-1279).

## 5. Emerging BitVM3, BitVMX, and Recursive Proof Research (2026-07-21 alignment)

The detailed, dated evidence record is [`BITVM3_BITVMX_RESEARCH_EXPANSION.md`](./BITVM3_BITVMX_RESEARCH_EXPANSION.md). The classification remains **Research / Evaluation Only**; it does not authorize production integration, settlement, or compliance decisions.

- **BitVM3** is a paper/protocol family centered on garbled-circuit-based off-chain verification. Its paper uses a Groth16 verifier as a circuit in the construction; it is not a recursive Groth16 SDK and is not a Conxian dependency.
- **BitVM2/Groth16** are separate layers: BitVM2 is a challenge protocol, while Groth16 is a proof system whose verifier can be the computation under review. Conxian's current boundary accepts an injected verifier but does not provide a production pairing backend.
- **BitVMX-CPU** is covered only by the isolated [`tools/bitvmx-eval`](../../tools/bitvmx-eval/) lane. It is not BitVM3, BitVMX-GC, garbled-circuit verification, or Groth16 verification.
- **BitVMX-GC and GOATNetwork/`bitvm2-gc`** remain research/reference material pending a stable public target, license review, reproducible build, independent vectors, resource fit, and security review.
- **Recursive SNARK/IVC systems such as Nova** are a separate research track and must not be treated as interchangeable with BitVM3, BitVMX-GC, or the current Gateway verifier boundary.
