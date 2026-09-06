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

## 7. Canton Network eUTXO & CBTC Non-Custodial Reserve Verification (2026-09 Expansion)

Canton Network employs a privacy-enabled Daml eUTXO model that is architecturally isomorphic to Bitcoin UTXOs. To bridge institutional $6T+ RWA state with permissionless Bitcoin settlement without introducing custodial risk, Conxian Gateway establishes two non-custodial verification primitives:

1. **CBTC Threshold Attestation Verification (Candidate I)**:
   - Verifies $k$-of-$n$ FROST threshold Schnorr attestation signatures emitted by BitSafe/Canton guardians.
   - Validates the accompanying Bitcoin L1 UTXO reserve proof (TXID, output index, satoshi value, and Merkle path) against the Gateway's L1 header verifier.
   - Operates in a strict zero-custody mode: the Gateway verifies attestation validity and reserve adequacy without holding keys or joining the guardian set.

2. **Canton eUTXO State Translation (Candidate J)**:
   - Maps Daml Active Contract Set (ACS) commitment hashes into Universal Contract References (UCR).
   - Anchors UCR roots to Bitcoin OP_RETURN / DLC commitment transactions.

## 8. BitVM3 Recursive Proof Efficiency & Canton ACS-to-UCR Translation Specs (2026-09 Expansion)

- **BitVM3 Recursive Proof Efficiency Targets**: BitVM3 incorporates garbled circuit verification with Groth16 circuit folding. Targets include maintaining recursive proof verification under 200,000 gas units / cycles equivalent and sub-second verification latency for nested SNARK proofs.
- **Canton Daml ACS to Bitcoin UCR Translation Protocol (Candidate J)**: Daml Active Contract Set (ACS) contract instances are hashed via SHA-256 to produce contract state commitments. These commitments map to Universal Contract References (UCR) format `ucr:canton:<domain>:<contract_id>` and are anchored to Bitcoin L1 UTXO outputs or DLC contract states.
- **BRICS mBridge Validator Deployment Requirements**: Requirements include non-custodial mBridge node payload parser compatibility, ISO 20022 `pacs.008`/`camt.053` payload mapping, and dual-rail settlement fallback.


## 9. Candidate Q: Client-Side Wasm UCV-1 Verification & BitVM3 Garbled-Circuit Folding Engine (2026-09 Expansion)

Candidate Q initiates local-first zero-trust client verification by compiling the Gateway Universal Chain Verification (UCV-1) core logic to WebAssembly (wasm32-unknown-unknown):

1. **Client-Side Wasm UCV-1 Core Architecture**:
   - Compiles cryptographic verification algorithms (BIP-340 Schnorr signatures, FROST threshold attestations, ISO 20022 XML structure validation, and Bitcoin L1 double-SHA256 Merkle proofs) into Wasm modules for consumption by `@conxian/client-sdk`.
   - Enables edge verification in browser and Node.js environments with zero network roundtrips to Gateway endpoints, reducing attestation latency to <50ms.

2. **BitVM3 Sub-200k Cycle Garbled-Circuit Folding**:
   - Implements a recursive Groth16 circuit folding accumulator for BitVM3 challenge-response protocols.
   - Compresses state-transition verifications into <200,000 gas/cycle equivalents, allowing optimistic fraud-proof dispute transactions to be posted on-chain within standard Bitcoin taproot script limits.

3. **Multi-Chain Edge State Anchoring**:
   - Maps cross-chain proofs across Canton Daml ACS, mBridge DLT state, and Stacks sBTC headers into unified local-first verification envelopes.
