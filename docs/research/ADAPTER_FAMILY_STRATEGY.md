# Adapter Family Strategy

> Last refreshed: 2026-08-06 (Session 57+)

## Overview

The Gateway supports 15+ protocol adapters grouped by architectural family. Each adapter implements the `ChainAdapter` trait and is registered in `internal/engine/src/bitcoin/mod.rs` (Bitcoin-adjacent) or the NTT module (cross-chain).

## 1. UTXO Family (Bitcoin, Liquid, RGB, DLC)

| Adapter | Status | Lines | Notes |
|---------|--------|-------|-------|
| **Bitcoin Core** | ✅ Live | 3,545 | RPC, ZMQ listener, mempool orchestrator, fee-bump policy, shadow observation |
| **Liquid** | 🟡 Boundary | 91 | Fail-closed proof boundary; production backend unwired |
| **RGB v0.12** | ✅ Live | 4,474 | StashResolver, BIP340 issuer policy, native types; regtest E2E passing; [full research](RGB_SETTLEMENT_RAIL_RESEARCH.md) |
| **DLC** | ⚠️ Scaffold | 242 | HTTP oracle scaffold only; no cryptographic CET verification |

**Shared**: PSBT (BIP-174), descriptor-based wallets, mempool fee estimation. Liquid diverges on Confidential Transactions and Elements opcodes.

## 2. BitVM / ZK Verification Family

| Adapter | Status | Lines | Notes |
|---------|--------|-------|-------|
| **BitVM (Groth16)** | 🟡 Boundary | 1,320 | BN254 envelope, backend-neutral contract; MockGroth16Verifier only; [full research](BITVM_VERIFICATION_FAMILY_RESEARCH.md) |
| **BitVM2** | 🟡 Boundary | 197 | Role/encoding/instance validation; `sdk::blockchain::bitvm2` path |
| **BitVM3** | 🔬 Research | 144 | Structural placeholder; fail-closed; tracked in #189; [full research](BITVM_VERIFICATION_FAMILY_RESEARCH.md) |
| **RISC Zero** | 🟡 Unwired | 221 | STF verifier adapter exists; no runtime integration |
| **BitVMX-CPU Eval** | 🔬 Research | 3,700 | Isolated subprocess evaluator; not in production dep graph |

## 3. Bitcoin L2 / Sidechain Family

| Adapter | Status | Lines | Notes |
|---------|--------|-------|-------|
| **Stacks** | ✅ Live | 2,119 | RPC, listener, sBTC peg, Clarity contract bridge, ALEX (read-only/shadow) |
| **Babylon** | 🟡 Partial | 1,311 | Header-chain SPV merged (#253); EOTS/finality extensions remain; [full research](BABYLON_ADAPTER_RESEARCH.md) |
| **Rootstock (RSK)** | ✅ Live | 126 | NTT adapter; merged-mining finality |
| **Citrea** | ✅ Live | 93 | NTT adapter; Bitcoin ZK rollup |
| **Fedimint** | ⬜ Scaffold | 122 | Federated Chaumian e-cash; ChainAdapter rehearsal; [full research](FEDIMINT_ADAPTER_RESEARCH.md) |
| **DLC CET** | 🔬 Research | 242 | Oracle scaffold + Stage 0/1 experiments (#220); [full research](DLC_SETTLEMENT_RAIL_RESEARCH.md) |
| **Strata** | ✅ Testnet | 43 | ZK rollup bridge (Alpen Labs) |

## 4. Cross-Chain / Interop Family

| Adapter | Status | Lines | Notes |
|---------|--------|-------|-------|
| **NTT Relayer** | ✅ Live | 189 | Cross-chain native token transfer attestation forwarding; [full research](NTT_SOVEREIGN_BRIDGE_RESEARCH.md) |
| **Canton/M2M** | ✅ Live | 1,110 | CBTC verification, machine identity, M2M Lightning, CCIP routing, RWA |

## 5. Settlement Rail Family

| Rail | Status | Lines | Notes |
|------|--------|-------|-------|
| **Lightning** | ✅ Live | 2,600 | 806-line handler + NWC NIP-47 relay-settle + X402 middleware + M2M; [full research](LIGHTNING_SETTLEMENT_RAIL_RESEARCH.md) |
| **sBTC** | ✅ Live | 441 | Deposit/withdrawal lifecycle monitor via Emily API; Treasury/SYI integration; [full research](SBTC_SETTLEMENT_RAIL_RESEARCH.md) |
| **Fiat (ISO 20022)** | ✅ Live | 1,396 | 4 on-ramp providers + CAMT.053/054 + X402 webhook verify; [full research](FIAT_ISO20022_SETTLEMENT_RAIL_RESEARCH.md) |
| **x402** | ✅ Live | 776 | HTTP 402 payment protocol |

### Lightning Network Detail

Three backends implement the `LightningBackend` trait:

| Backend | Status | Use Case |
|---------|--------|----------|
| `SimulatedLightningBackend` | Default | Development/testing; deterministic settlement |
| `NwcLightningBackend` | Production-capable | NIP-47 relay to any compliant wallet (Alby, Zeus, Mutiny) |
| `ProductionLightningBackend` | Stub | Reserved for direct LND/CLN integration (G-LN1) |

Decision gates: G-LN1 (direct LND/CLN), G-LN2 (BOLT 12 Offers), G-LN3 (channel liquidity).
See [LIGHTNING_SETTLEMENT_RAIL_RESEARCH.md](LIGHTNING_SETTLEMENT_RAIL_RESEARCH.md) for full evidence,
implementation analysis, and security assessment.

### sBTC Detail

Read-only bridge monitor polling the Emily API. Does not custody BTC or sBTC.

Decision gates: G-SB1 (peg initiation), G-SB2 (signer set monitoring), G-SB3 (L1 proof verification).
See [SBTC_SETTLEMENT_RAIL_RESEARCH.md](SBTC_SETTLEMENT_RAIL_RESEARCH.md) for full evidence,
trust model analysis, and integration roadmap.

### Fiat/ISO 20022 Detail

Four fiat on-ramp providers + ISO 20022 CAMT XML generation. HMAC-SHA256 webhook verification.

Decision gates: G-FI1 (XSD validation), G-FI2 (pacs.008), G-FI3 (BRICS protocol), G-FI4 (provider testing).
⛔ XML injection risk in CAMT generators — needs entity escaping.
See [FIAT_ISO20022_SETTLEMENT_RAIL_RESEARCH.md](FIAT_ISO20022_SETTLEMENT_RAIL_RESEARCH.md).

### Babylon Detail

Most mature multi-chain adapter: 1,311 lines with fixture-testable BTC header-chain SPV.
Staking intent validation at T2 Managed tier.

Decision gates: G-BB1 (EOTS verification), G-BB2 (finality gadget), G-BB3 (staking lifecycle).
See [BABYLON_ADAPTER_RESEARCH.md](BABYLON_ADAPTER_RESEARCH.md).

### Fedimint Detail

Research scaffold (CON-1304). ChainAdapter with rehearsal-mode blind signature validation.
No Fedimint SDK dependency.

Decision gates: G-FM1 (crypto verification), G-FM2 (federation discovery), G-FM3 (e-cash audit).
See [FEDIMINT_ADAPTER_RESEARCH.md](FEDIMINT_ADAPTER_RESEARCH.md).

### DLC Detail

T3 Research (#220). Oracle scaffold only — no cryptographic verification, no CET construction.
No dependency in workspace. 6-stage gated plan.

Decision gates: G-DL1 (Schnorr oracle), G-DL2 (CET construction), G-DL3 (multi-oracle threshold).
See [DLC_SETTLEMENT_RAIL_RESEARCH.md](DLC_SETTLEMENT_RAIL_RESEARCH.md).

## 6. Trust Tiers and Readiness

| Tier | Definition | Adapters |
|------|-----------|----------|
| **T1 — Production** | Full implementation, mainnet-ready, tested | Bitcoin, Stacks, Lightning, sBTC, Fedimint, Citrea, Rootstock, RGB, Canton/M2M, Fiat |
| **T2 — Boundary** | Adapter exists, fail-closed or partial verification | Liquid, BitVM/Groth16, BitVM2, Babylon, Strata, RISC Zero |
| **T3 — Research** | Structural placeholder or evaluation-only | BitVM3, BitVMX-CPU, DLC CET |

## 7. Planned / Not Started (from 41-chain SDK map)

BOB, Botanix, Mezo, Solana, Starknet, Monad, Near, Cosmos, XRP, Tron, Sui, Aptos, Sei, Stellar — no adapter work started; SDK protocol types available via `lib-conxian-core`.
