# Conxian Gateway — Production Enablement Research (2026-09-11)

> Scope: what remains to move **conxian-gateway** (transport/RPC + compliance
> pipe) from a CI-green, partially fail-closed state to **full production
> enablement**. This is a dated research snapshot, not a replacement for
> `docs/READINESS_GATES.md` or `docs/ORG_WIDE_FUNCTIONALITY_AUDIT_*.md`.

## 0. Verdict

Gateway is **functionally substantial but not fully production-enabled**. `main`
is CI-green (build-and-test, clippy, fmt, release build, lightning-coverage,
RGB-native, gitleaks, MSRV) and 38 of 40 PRD requirements are complete. The two
open requirements — **R32** (BitVM2-backed Job Card settlement) and **R40**
(BitVM3 adapter) — are deliberately fail-closed research lanes, and three
adapter lanes (**BitVM3**, **BitVM**, **Liquid**) remain rehearsal-only. The
blocker is **proof-surface + attestation evidence**, not build hygiene.

## 1. Current baseline (verified 2026-09-11)

- CI on `main`: green across the full matrix (see §0).
- Version `v0.1.5` (released 2026-07-30); MSRV **1.98.1** (consistent).
- Adapter registry: `liquid` (fail-closed, Elements proof unwired),
  `rootstock` (structural), `babylon` (BIP340 EOTS), `bitvm` (BN254 Groth16
  envelope, no pairing backend), `bitvm3` (research-only, fail-closed),
  `fedimint` (structural + blind-sig), `citrea` (structural), `strata`
  (structural).

## 2. Production-enablement research gaps (prioritized)

| Pri | Gap | Blocked-by | Notes |
|-----|-----|-----------|-------|
| **P0** | **BitVM3 adapter (R40)** — garbled-circuit + recursive proof verification. | #189 | Research: stable BitVM3/GC SDK, audit, verified deployment. |
| **P0** | **BitVM2 Job Card settlement verification (R32)** — reviewed cryptographic verifier before the `/v1/job-card/settle` path can persist a card. | #189 | Research: BN254 Groth16 verifier with reviewed VK. |
| **P0** | **Enclave attestation evidence** — production proof/settlement enablement is gated on enclave-sdk #202/#240/#241/#242 (independent security review, Android StrongBox, AWS Nitro, revocation/replay). | enclave-sdk (P0) | Research: TEE remote-attestation evidence model (shared with Nexus P0). |
| **P1** | **Liquid adapter** — wire the Elements proof backend (currently fail-closed). | — | Research: Elements SPV/proof verification surface. |
| **P1** | **Curve / verifier-ownership contract (G-2)** — Gateway BN254 vs Nexus Arkworks/BLS12-381. | #189 (G-2) | Research: single canonical curve/VK/public-input/state-root contract. |
| **P1** | **DLC bond lifecycle** — trait + DTO scaffolding only; no wire protocol, oracle signature verification, or CET/refund construction. | `docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md` | Research: DLC oracle attestation + CET construction. |
| **P1** | **ALEX settlement path** — quote/prepare are read-only/shadow; signer, broadcast, receipt, reconciliation remain unwired. | ALEX evidence gate | Research: exact-helper-principal settlement path. |
| **P2** | **sBTC rail depth + RGB native** — deeper verification beyond structural. | — | Research: sBTC peg proof; RGB transition verification. |

## 3. Dated findings relevant to enablement (2026-09-11)

1. **BitVM3 whitepaper** — garbled-circuit fraud proofs (~200-byte on-chain
   challenge) directly inform R40/#189; confirms BitVM3 is not yet
   production-grade and should stay fail-closed.
2. **AWS Bedrock AgentCore Payments (Preview, 2026-05-07)** — validates the
   x402 industrial-intent lane (R36) as the agentic-commerce settlement path.
3. **BIS withdrawal from mBridge (~2026-09-10)** — reinforces the ISO 20022 /
   CIPS / BRICS / PAPSS / SPFS settlement-rail posture already wired (pacs.008).

## 4. Non-goals / boundary

- Gateway is transport/verification + compliance pipe; it is **not** the
  protocol source-of-truth and **not** a custody authority.
- No DeFi protocol rebuilding; use existing rails (x402, NTT, ISO 20022).
