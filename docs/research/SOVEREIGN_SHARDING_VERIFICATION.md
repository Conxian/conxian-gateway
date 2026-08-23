# Research: Phase 7 Sovereign Labor & Sharding Verification (SSV-1)

## 1. Objective
Define research requirements for decentralized sovereign sharding, specifically focusing on multi-currency jurisdictional compliance and the design of BitVM2-backed labor-proof flows.

## 2. Theoretical Framework
Utilizes the **Conxian Unified Theory v2.0** for sharded state consistency across heterogeneous chains.

### Core Components:
- **Labor Attestation**: TEE-signed proofs of work/service completion (Job Cards).
- **Jurisdictional Sharding**: Mapping of settlement rails to regional compliance rules (BRICS, PAPSS) verified via UCV-1.
- **State Root Aggregation**: A research concept for checkpointing sharded state. No segment count or benchmark is verified for the Gateway.

## 3. Implementation Path
1. **ZkcVerifier Expansion**: Integrated with Tableland for decentralized state commit (SovereignCommit trait).
2. **Multi-Source Ingress**: Normalization of ISO 20022, PAPSS, and BRICS payloads into a unified settlement envelope.
3. **Optimistic Fraud Proofs (research only)**: BitVM2 is a challenge-protocol design, but the current Gateway provides no BitVM2 dispute handling for labor settlement roots. `BitVmAdapter::verify_state_proof` is metadata-only and checks for a `root_hash` field; its separate Groth16 envelope path validates a canonical request and delegates to an injected `Groth16Verifier`. The deterministic fixture-backed mock is test-only and does not perform cryptographic verification.

## 4. Institutional Alignment
- **OData v4 ERP Sync**: Automatic field extraction for SAP/Oracle audit alignment.
- **Fail-Closed Readiness**: Any future labor-settlement flow would require explicit context-aware attestation and settlement-authorization policy; the current BitVM adapter does not authorize labor settlements (CON-1279).

## 5. Emerging BitVM3, BitVMX, and Recursive Proof Research (2026-07-21 alignment)

The detailed, dated evidence record is [`BITVM3_BITVMX_RESEARCH_EXPANSION.md`](./BITVM3_BITVMX_RESEARCH_EXPANSION.md). The classification remains **Research / Evaluation Only**; it does not authorize production integration, settlement, or compliance decisions.

- **BitVM3** is a paper/protocol family centered on garbled-circuit-based off-chain verification. Its paper uses a Groth16 verifier as a circuit in the construction; it is not a recursive Groth16 SDK and is not a Conxian dependency.
- **BitVM2/Groth16** are separate layers: BitVM2 is a challenge protocol, while Groth16 is a proof system whose verifier can be the computation under review. Conxian's current boundary accepts an injected verifier but does not provide a production pairing backend or authorize settlement.
- **BitVMX-CPU** is covered only by the isolated [`tools/bitvmx-eval`](../../tools/bitvmx-eval/) lane. It is not BitVM3, BitVMX-GC, garbled-circuit verification, or Groth16 verification.
- **BitVMX-GC and GOATNetwork/`bitvm2-gc`** remain research/reference material pending a stable public target, license review, reproducible build, independent vectors, resource fit, and security review.
- **Recursive SNARK/IVC systems such as Nova** are a separate research track and must not be treated as interchangeable with BitVM3, BitVMX-GC, or the current Gateway verifier boundary.

## 6. Zero-Mock Production Alignment Strategy & Mock/Placeholder Audit

To guarantee institutional mainnet integrity, the Conxian Gateway enforces strict separation between production code paths and simulation/test-only scaffolds:

### 6.1 Audit of Mock Scaffolds & Research Placeholders
1. **Groth16 Verifier Boundary (`MockGroth16Verifier`)**:
   - *Status*: Test-only fixture verifier.
   - *Gating*: Strictly gated behind `#[cfg(any(test, feature = "mock-integrations"))]`.
   - *Production Pathway*: Production flows require an injected cryptographic pairing backend (e.g., `ark-groth16` / `ark-bn254` with hardware acceleration or trusted setup verification).

2. **DLC CET & Bond Manager Scaffold**:
   - *Status*: Scaffolding with cryptographic Schnorr oracle verification implemented; funding, CET, refund, adaptor-signature construction, and real bond state management remain research-gated.
   - *Production Pathway*: Transition from UUID/mock bond identifiers to full cryptographic Discreet Log Contract transaction building (`bdk_wallet` / `rust-dlc`) prior to mainnet value-bearing enablement.

3. **ALEX Swap & Quote Scaffold**:
   - *Status*: Read-only quote/venue manifest evaluation.
   - *Gating*: Execution disabled (`POST /api/v1/alex/swap` is stably execution-disabled).
   - *Production Pathway*: Requires full venue manifest attestation and verified Stacks smart contract execution before enabling production liquidity routing.

4. **BitVM & Babylon State Proof Verification**:
   - *Status*: Metadata-only / header-chain verification (recency checks and double-sign key extraction).
   - *Production Pathway*: Full BitVM dispute resolution protocol and EOTS finality provider key extraction implemented; complete multi-step challenge-response flows remain gated behind dedicated validator infrastructure.

### 6.2 Production Promotion Criteria
- **Zero-Contamination Enforcement**: Continuous integration automatically runs `python3 scripts/verify_contamination_guard.py` to ensure no `stub`, `placeholder`, or `changeme` keywords exist in `cmd/`, `internal/`, `pkg/`, `apps/`, or `packages/`.
- **Fail-Closed Runtime Architecture**: Any feature flag or unconfigured dependency fails closed with standard HTTP status codes (`503 Service Unavailable` or `400 Bad Request`) rather than falling back to unauthenticated or mock responses in production binaries.
