# Conxian Gateway — Knowledge Map

> Generated: 2026-09-06 | All P1/P2 strategic gaps implemented | Active Candidates Q, R, S, T Integrated

---

## Architecture Overview

```
                            ┌──────────────────────────────────────────────┐
                            │            Conxian Gateway (Axum)             │
                            │                                              │
                            │  ┌──────────┐  ┌──────────┐  ┌───────────┐  │
                            │  │ REST API │  │   Auth   │  │  Metrics   │  │
                            │  │ (22 EP)  │  │ (Bearer) │  │ (Prometheus)│  │
                            │  └────┬─────┘  └──────────┘  └───────────┘  │
                            │       │                                      │
                            │  ┌────┴─────────────────────────────────┐    │
                            │  │         UCV-1 Universal Verifier       │    │
                            │  │  (Ecdsa,Schnorr,Zkml,BitVm,Cbtc,Wasm) │    │
                            │  └────┬─────────────────────────────────┘    │
                            │       │                                      │
                            │  ┌────┴──────────────┬──────────────────┐    │
                            │  │   Compliance      │    Engine         │    │
                            │  │   (ZKC Pipeline)  │  (14 Adapters)   │    │
                            │  └───────────────────┴──────────────────┘    │
                            └──────────────────────────────────────────────┘
                                     │                      │
                            ┌────────┴──────┐    ┌─────────┴──────────┐
                            │  Sanctions     │    │  Protocol Adapters │
                            │  Screening     │    │  ┌──────────────┐  │
                            │  (OFAC/EU/UN)  │    │  │ Bitcoin Core │  │
                            └───────────────┘    │  ├──────────────┤  │
                                                  │  │ Lightning    │  │
                            ┌───────────────┐    │  ├──────────────┤  │
                            │  Persistence   │    │  │ Liquid       │  │
                            │  (Atomic FS)   │    │  ├──────────────┤  │
                            └───────────────┘    │  │ Stacks/sBTC  │  │
                                                  │  ├──────────────┤  │
                                                  │  │ Rootstock    │  │
                                                  │  ├──────────────┤  │
                                                  │  │ RGB v0.12    │  │
                                                  │  ├──────────────┤  │
                                                  │  │ Babylon      │  │
                                                  │  ├──────────────┤  │
                                                  │  │ BitVM2/BitVM3│  │
                                                  │  ├──────────────┤  │
                                                  │  │ Fedimint     │  │
                                                  │  ├──────────────┤  │
                                                  │  │ Citrea       │  │
                                                  │  ├──────────────┤  │
                                                  │  │ Canton / Daml│  │
                                                  │  ├──────────────┤  │
                                                  │  │ BRICS mBridge│  │
                                                  │  ├──────────────┤  │
                                                  │  │ peaq Machine │  │
                                                  │  └──────────────┘  │
                                                  └────────────────────┘
```

---

## Key Subsystems & File Map

### 1. Gateway REST API (`internal/api/`)
- `src/handlers.rs`: REST endpoint handlers including UCV-1 verification, ISO 20022 payment initiation (`pacs.008`), BRICS mBridge ingress, identity resolution, DLC bonds, and admin governance.
- `src/canton_m2m.rs`: Canton Daml state translation (Candidate J), Canton CCIP routing (Candidate S), Machine Identity & RWA attestation (Candidate R), and M2M Lightning micro-settlement.
- `src/camt.rs`: SWIFT ISO 20022 `pacs.008` generation and `camt.053` Bank-to-Customer Treasury Statement XML generation (Candidate T).
- `src/auth.rs`: Bearer token authentication and sentinel rejection.
- `src/nostr.rs` & `src/nwc_backend.rs`: Nostr Wallet Connect (NWC) NIP-47 relay-settle protocol handlers.

### 2. Compliance & Zero-Knowledge Verification (`internal/compliance/`)
- `src/zkc.rs`: `ZkcVerifier` implementing `CoreVerifier` and `Bip322Verifier`. Normalizes multi-source ingress (ISO 20022, BRICS mBridge, PAPSS, ERP) and validates XML structure against quick-xml rules.
- `src/identity.rs`: Tier 1 identity resolution integrating Web3.bio Profile API and World ID Verification API with fail-closed simulated fallback.
- `src/crypto.rs`: Cryptographic primitives including MuSig2 key aggregation and Blake2s PRF for V-UTXO derivation.

### 3. Execution Engine & Adapters (`internal/engine/`)
- `src/bitcoin/dlc_oracle.rs`: DLC contract orchestration, Schnorr oracle threshold verification, CBTC non-custodial reserve attestation (`verify_cbtc_reserve_attestation`), and Canton Daml ACS state translation (`translate_to_ucr`).
- `src/bitcoin/babylon_adapter.rs`: Babylon staking EOTS Schnorr signature verification and double-signing private key extraction ($x = (s_1 - s_2) / (e_1 - e_2)$).
- `src/bitcoin/fedimint_adapter.rs`: Fedimint guardian x-only pubkey Schnorr blind signature verification.
- `src/brics_adapter.rs`: BRICS mBridge DLT state proof and threshold Schnorr consensus signature verification.
- `src/stacks/sbtc.rs`: sBTC double-SHA256 raw transaction hashing and 80-byte block header PoW verification.
- `src/treasury/mod.rs`: Sovereign Yield Index (SYI) monitoring and ALEX DEX market quote integration.

### 4. Client SDK & Schemas (`packages/`)
- `packages/schemas/index.ts`: TypeScript domain interfaces for UCV-1, MuSig2, DLC, ISO 20022, Canton state translation, CCIP routing, mBridge ingress, Wasm UCV-1, Machine Identity, DePIN RWA attestation, and camt.053 treasury statements.
- `packages/client-sdk/index.ts`: `ConxianClient` providing typed API calls for web and Node.js applications, including client-side zero-trust Wasm verification (`verifyStateProofLocal`).

---

## Candidate Mapping Summary

| Candidate | Name | Implementation Location | Status |
|---|---|---|---|
| **Candidate I** | CBTC Non-Custodial Reserve Verification | `internal/engine/src/bitcoin/dlc_oracle.rs` | ✅ Shipped |
| **Candidate J** | Canton State Translation Adapter | `internal/engine/src/bitcoin/dlc_oracle.rs` & `internal/api/src/canton_m2m.rs` | ✅ Shipped |
| **Candidate K** | ISO 20022 XML Schema Validation | `internal/compliance/src/zkc.rs` | ✅ Shipped |
| **Candidate L** | ISO 20022 pacs.008 Payment Initiation | `internal/api/src/camt.rs` & `internal/compliance/src/zkc.rs` | ✅ Shipped |
| **Candidate M** | Babylon EOTS Key Extraction | `internal/engine/src/bitcoin/babylon_adapter.rs` | ✅ Shipped |
| **Candidate N** | Fedimint Blind Sig Verification | `internal/engine/src/bitcoin/fedimint_adapter.rs` | ✅ Shipped |
| **Candidate O** | sBTC L1 Proof Verification | `internal/engine/src/stacks/sbtc.rs` | ✅ Shipped |
| **Candidate P** | BRICS mBridge DLT Settlement | `internal/engine/src/brics_adapter.rs` & `internal/compliance/src/zkc.rs` | ✅ Shipped |
| **Candidate Q** | Wasm UCV-1 & BitVM3 Folding | `@conxian/client-sdk` & `internal/engine/src/bitvm3_adapter.rs` | ✅ Shipped |
| **Candidate R** | Machine Economy & DePIN peaq DLT | `internal/api/src/canton_m2m.rs` & `@conxian/client-sdk` | 🚀 Active |
| **Candidate S** | Canton CCIP Cross-Chain Gateway | `internal/api/src/canton_m2m.rs` & `@conxian/client-sdk` | ✅ Shipped |
| **Candidate T** | SWIFT camt.053 Real-Time ERP Reporting | `internal/api/src/camt.rs` & `@conxian/client-sdk` | 🚀 Active |
