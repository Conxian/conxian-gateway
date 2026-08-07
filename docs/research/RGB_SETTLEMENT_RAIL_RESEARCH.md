# RGB Settlement Rail: Evidence Review & Integration Strategy

**Status:** Live (T1 Production) | **Lines:** 3,744 (adapter + stash) | **Feature:** `rgb-native`
**Last refreshed:** 2026-08-07 | **Session:** 49

---

## Executive Summary

RGB is a client-side validated smart contract protocol for Bitcoin and the
Lightning Network. Contracts are executed and validated off-chain by contract
participants, with only blinded state commitments anchored to Bitcoin
transactions. This enables private, scalable smart contracts (tokens, NFTs,
DeFi) without modifying Bitcoin's consensus layer.

Conxian Gateway integrates RGB as a **T1 Production settlement rail** through
a 489-line adapter (`NodeRgbAdapter`) backed by a 3,255-line native stash
(`rgb_stash.rs`). The adapter implements a three-tier `RolloutMode` strategy
(Disabled → Shadow → Active) with `rgb-stl` stockpile management, Esplora
UTXO resolution, and BIP340 issuer signature policy.

**Current state:** Active mode uses the native RGB stockpile exclusively
(fail-closed — never trusts HTTP proofs). Shadow mode uses HTTP with
simulation fallback. BIP340 issuer policy is wired at runtime from
`RGB_ISSUER_POLICY_PATH` (Session 49 enhancement).

---

## 1. Protocol Evidence

### 1.1 RGB Specification

- **RGB Protocol:** <https://rgb.tech>
- **RGB STL (Standard Library):** <https://github.com/RGB-Tools/rgb-stl>
- **AluVM:** RGB's deterministic virtual machine for contract execution
- **Schema validation:** Strict encoding for contract state transitions

### 1.2 Architecture

```
RGB Node (external)                    Gateway
    │                                     │
    │  HTTP API (Shadow mode)             │
    ├──→ /consignments/{contract_id}      │
    │    /transitions/{contract_id}       │
    │    /verify                          │
    │                                     │
    │                              NodeRgbAdapter
    │                              ├─ lookup_contract()
    │                              ├─ verify_transition()
    │                              └─ RolloutMode gating
    │                                     │
    │                              rgb_stash (native)
    │                              ├─ Stockpile (filesystem)
    │                              ├─ Esplora UTXO resolution
    │                              ├─ BIP340 issuer policy
    │                              └─ Transition validity
```

### 1.3 Three-Tier RolloutMode

| Mode | RPC Path | Fallback | Trust |
|------|----------|----------|-------|
| **Disabled** | None | Returns `ConxianError::Unavailable` | Zero |
| **Shadow** | HTTP (simulation) | Mock/simulator | Observation-only |
| **Active** | Native stockpile | None (fail-closed) | Full verification |

### 1.4 Mainnet Evidence

- RGB v0.11 released (2024): stable protocol with `rgb-std` and `rgb-stl`
- RGB20 (fungible tokens) and RGB21 (NFTs) schemas standardized
- DIBA (Bitcoin NFT marketplace) uses RGB for asset issuance
- BitMask wallet supports RGB asset management
- BIP340 (Schnorr) signatures used for issuer attestations

---

## 2. Current Gateway Implementation

### 2.1 Code Surface

| Component | File | Lines | Status |
|-----------|------|-------|--------|
| Adapter | `internal/engine/src/bitcoin/rgb_adapter.rs` | 489 | Live |
| Native stash | `internal/engine/src/bitcoin/rgb_stash.rs` | 3,255 | Live (`rgb-native`) |
| Issuer policy | `internal/engine/src/bitcoin/rgb_issuer_policy.rs` | ~100 | Live |
| **Total** | | **3,744+** | |

### 2.2 Capabilities

| Capability | Status | Notes |
|-----------|--------|-------|
| Contract lookup (HTTP) | ✅ Live | Shadow mode: `GET /consignments/{contract_id}` |
| Transition verification (HTTP) | ✅ Live | Shadow mode: `POST /verify` |
| Native stockpile | ✅ Live | Active mode: `rgb-stl` filesystem stockpile |
| UTXO resolution | ✅ Live | Esplora API integration |
| BIP340 issuer policy | ✅ Live | Runtime-loaded from `RGB_ISSUER_POLICY_PATH` |
| Three-tier RolloutMode | ✅ Live | Disabled / Shadow / Active |
| Schema validation | ✅ Live | Active mode: strict encoding + state transition rules |
| Lightning integration | ⬜ Research | RGB assets over Lightning channels |
| Multi-contract portfolios | ⬜ Research | Cross-contract atomic swaps |

---

## 3. Modularization Note

`rgb_stash.rs` at 3,255 lines is the largest file in the codebase. It
contains all native RGB infrastructure: stockpile management, UTXO
resolution, transition validation, issuer policies, concurrency locking.

**Recommendation:** Split into:
- `rgb_stash/resolver.rs` — Esplora + UTXO resolution
- `rgb_stash/stockpile.rs` — Stockpile management (BinFile, temp imports)
- `rgb_stash/transitions.rs` — Transition validation + journal
- `rgb_stash/locking.rs` — File locking + concurrency

Deferred due to `rgb-native` feature gate (cannot compile without feature).

---

## 4. Decision Gates Summary

| Gate | Status | Blocking |
|------|--------|----------|
| Contract lookup | ✅ Deployed | — |
| Transition verification | ✅ Deployed | — |
| Native stockpile (Active) | ✅ Deployed | — |
| BIP340 issuer policy | ✅ Deployed | — |
| RolloutMode gating | ✅ Deployed | — |
| rgb_stash modularization | ❌ Deferred | Feature gate |
| Lightning RGB assets | ❌ Research | Protocol maturity |

---

## 5. Cross-References

- **ADAPTER_FAMILY_STRATEGY.md:** RGB at T1 Production
- **CON-768:** Gateway RGB node integration
- **Session 49:** BIP340 issuer policy runtime loading
- **API surface:** Internal wallet API routes (RGB contract endpoints)
