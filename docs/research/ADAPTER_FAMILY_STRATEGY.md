# Adapter Family Strategy

> Last refreshed: 2026-08-06 (Session 57+)

## Overview

The Gateway supports 15+ protocol adapters grouped by architectural family. Each adapter implements the `ChainAdapter` trait and is registered in `internal/engine/src/bitcoin/mod.rs` (Bitcoin-adjacent) or the NTT module (cross-chain).

## 1. UTXO Family (Bitcoin, Liquid, RGB, DLC)

| Adapter | Status | Lines | Notes |
|---------|--------|-------|-------|
| **Bitcoin Core** | ✅ Live | 3,545 | RPC, ZMQ listener, mempool orchestrator, fee-bump policy, shadow observation |
| **Liquid** | 🟡 Boundary | 91 | Fail-closed proof boundary; production backend unwired |
| **RGB v0.12** | ✅ Live | 4,474 | StashResolver, BIP340 issuer policy, native types; regtest E2E passing |
| **DLC** | ⚠️ Scaffold | 242 | HTTP oracle scaffold only; no cryptographic CET verification |

**Shared**: PSBT (BIP-174), descriptor-based wallets, mempool fee estimation. Liquid diverges on Confidential Transactions and Elements opcodes.

## 2. BitVM / ZK Verification Family

| Adapter | Status | Lines | Notes |
|---------|--------|-------|-------|
| **BitVM (Groth16)** | 🟡 Boundary | 1,320 | BN254 envelope, backend-neutral contract; MockGroth16Verifier only |
| **BitVM2** | 🟡 Boundary | 197 | Role/encoding/instance validation; `sdk::blockchain::bitvm2` path |
| **BitVM3** | 🔬 Research | 144 | Structural placeholder; fail-closed; tracked in #189 |
| **RISC Zero** | 🟡 Unwired | 221 | STF verifier adapter exists; no runtime integration |
| **BitVMX-CPU Eval** | 🔬 Research | 3,700 | Isolated subprocess evaluator; not in production dep graph |

## 3. Bitcoin L2 / Sidechain Family

| Adapter | Status | Lines | Notes |
|---------|--------|-------|-------|
| **Stacks** | ✅ Live | 2,119 | RPC, listener, sBTC peg, Clarity contract bridge, ALEX (read-only/shadow) |
| **Babylon** | 🟡 Partial | 1,311 | Header-chain SPV merged (#253); EOTS/finality extensions remain |
| **Rootstock (RSK)** | ✅ Live | 126 | NTT adapter; merged-mining finality |
| **Citrea** | ✅ Live | 93 | NTT adapter; Bitcoin ZK rollup |
| **Fedimint** | ✅ Live | 122 | Federated Chaumian e-cash |
| **Strata** | ✅ Testnet | 43 | ZK rollup bridge (Alpen Labs) |

## 4. Cross-Chain / Interop Family

| Adapter | Status | Lines | Notes |
|---------|--------|-------|-------|
| **NTT Relayer** | ✅ Live | 189 | Cross-chain native token transfer attestation forwarding |
| **Canton/M2M** | ✅ Live | 1,110 | CBTC verification, machine identity, M2M Lightning, CCIP routing, RWA |

## 5. Settlement Rail Family

| Rail | Status | Notes |
|------|--------|-------|
| **Lightning** | ✅ Live | 806-line handler; NWC NIP-47 relay-settle |
| **sBTC** | ✅ Live | 441-line adapter; deposit/withdrawal/proof verification |
| **Fiat (ISO 20022)** | ✅ Live | SPFS, PAPSS, CIPS, mBridge, CAMT.053 |
| **x402** | ✅ Live | HTTP 402 payment protocol |

## 6. Trust Tiers and Readiness

| Tier | Definition | Adapters |
|------|-----------|----------|
| **T1 — Production** | Full implementation, mainnet-ready, tested | Bitcoin, Stacks, Lightning, sBTC, Fedimint, Citrea, Rootstock, RGB, Canton/M2M, Fiat |
| **T2 — Boundary** | Adapter exists, fail-closed or partial verification | Liquid, BitVM/Groth16, BitVM2, Babylon, Strata, RISC Zero |
| **T3 — Research** | Structural placeholder or evaluation-only | BitVM3, BitVMX-CPU, DLC CET |

## 7. Planned / Not Started (from 41-chain SDK map)

BOB, Botanix, Mezo, Solana, Starknet, Monad, Near, Cosmos, XRP, Tron, Sui, Aptos, Sei, Stellar — no adapter work started; SDK protocol types available via `lib-conxian-core`.
