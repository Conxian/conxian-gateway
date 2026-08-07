# DLC Settlement Rail: Evidence Review & Integration Strategy

**Status:** Research (T3) | **Lines:** 242 (scaffold) + experiments/ | **Issue:** #220
**Last refreshed:** 2026-08-07 | **Session:** 49

---

## Executive Summary

Discreet Log Contracts (DLCs) are a Bitcoin-native protocol for trustless
conditional payments. Two parties lock funds into a multisig UTXO that can be
spent according to outcomes attested by one or more oracles, without the
oracles learning about the contract. DLCs enable derivatives, prediction
markets, insurance, and other conditional financial instruments directly on
Bitcoin.

Conxian Gateway has a **research scaffold** for DLC oracle integration and a
comprehensive `experiments/dlc-stage0/` workspace with SDK comparison and
conformance testing. The DLC CET (Contract Execution Transaction) module was
attempted and reverted from `main` (commit `cb8b680`) due to API-incompatible
`dlc-manager` dependency.

**Current state:** 242-line oracle client scaffold with HTTP-only event
fetching. No cryptographic oracle signature verification. No CET
construction. Stage 0/1 conformance probes show 13/14 byte-perfect vector
matches against `dlcspecs`. 6-stage gated plan defined.

**Decision:** DLC remains T3 Research. No dependency added to workspace. Work
must proceed through isolated experiments before Gateway integration.

---

## 1. Protocol Evidence

### 1.1 DLC Specification

- **dlcspecs:** <https://github.com/discreetlogcontracts/dlcspecs> (pinned at commit `9cd9148`)
- **DLC paper:** "Discreet Log Contracts" by Thaddeus Dryja (2017)
- **CET construction:** Adaptor signatures on Bitcoin transactions keyed to oracle attestations

### 1.2 Protocol Architecture

```
┌─ Party A ─┐                    ┌─ Party B ─┐
│  BTC UTXO  │──── Funding TX ──→│  BTC UTXO  │
│            │                    │            │
│  CET₀ (sig_adaptor_A ⊕ σ₀)     │  CET₀ (sig_adaptor_B ⊕ σ₀)
│  CET₁ (sig_adaptor_A ⊕ σ₁)     │  CET₁ (sig_adaptor_B ⊕ σ₁)
│  Refund   (timelocked)         │  Refund   (timelocked)
└────────────┘                    └────────────┘
       │                                  │
       └──────── Oracle Attestation ──────┘
                       σ = s·G
        Oracle signs outcome → party decrypts adaptor sig → broadcasts CET
```

### 1.3 Mainnet Evidence

| Event | Year | Detail |
|-------|------|--------|
| Crypto Garage / Skew | 2019 | S&P 500 DLC using ICC oracle |
| Crypto Garage DLC on LN | 2022 | DLC settled over Lightning Network |
| Lava.xyz | 2023+ | DLC-based BTC collateralized stablecoin (USDL) |
| 10101 Finance | 2023+ | DLC-based derivatives platform |
| Atomic Loans | 2024+ | DLC-based BTC-backed lending |

Source: `DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md` (35,493 bytes)

### 1.4 Implementations

| Implementation | Language | License | Gateway Compatibility |
|---------------|----------|---------|----------------------|
| rust-dlc v0.8.0 | Rust | MIT/Apache 2.0 | Preferred (Stage 0) |
| DDK v1.1.2 | Rust | Proprietary? | Blocked (MSRV issue) |
| bitcoin-s (Suredbits) | Scala | MIT | N/A |
| CFD-DLC (Crypto Garage) | Rust | MIT | N/A |
| dlc-wallet (Atomic Loans) | Rust | MIT | N/A |

Source: `DLC_STAGE0_SDK_COMPARISON_2026-07-22.md`

---

## 2. Current Gateway Implementation

### 2.1 Oracle Client (242 lines scaffold)

```
DlcOracleClient
    ├─ list_announcements() → GET /v1/announcements
    ├─ get_attestation(event_id) → GET /v1/attestation/{event_id}
    └─ verify_attestation(ann, att, index)
        └─ Payload consistency only (event_id, pubkey, outcome match)
        └─ NO cryptographic signature verification ← issue #220

ThresholdOracleCoordinator
    ├─ collect_announcements() → fetch across N oracles
    └─ check_threshold_outcome(event_id) → count matching payloads
```

### 2.2 Stage 0/1 Experiments

```
experiments/dlc-stage0/
├── rust-dlc-probe/
│   ├── src/main.rs                    # SDK comparison
│   └── src/bin/
│       ├── rust-dlc-stage0-vector-probe.rs    # 7/14 vectors parse
│       ├── rust-dlc-stage1-conformance.rs     # 13/14 byte-perfect
│       └── rust-dlc-stage1-fixture.rs         # 13 rejection cases
└── ddk-probe/                         # Blocked on MSRV
```

**Stage 1 results:** 14/14 vectors parse, 13/14 byte-perfect. The hyperbola
offer mismatch is deterministic: fixed-point vs IEEE-754 f64 encoding.
8 rejection tests pass (oracle boundary, wrong event/key, signed-outcome
mutation, etc.).

### 2.3 What Was Removed

Commit `cb8b680` reverted the DLC CET module from `main`:
- `DlcBond` API used UUID-shaped mock IDs (no real funding/CET/refund)
- `dlc-manager` dependency was API-incompatible with workspace MSRV
- Oracle adapter was HTTP-only (no Schnorr signature verification)

---

## 3. 6-Stage Gated Plan

| Stage | Description | Status |
|-------|-------------|--------|
| 0 — SDK Spike | Compare SDKs, select low-level candidate | ✅ Complete |
| 1 — Conformance | Verify vectors, deterministic fixture | ✅ Complete (13/14) |
| 2 — Oracle Crypto | Schnorr sig verification for oracle attestations | ❌ Research |
| 3 — Gateway State | Wire CET construction into Gateway engine | ❌ Blocked |
| 4 — Public Testnet | End-to-end DLC lifecycle on Bitcoin testnet | ❌ Blocked |
| 5 — Security Review | Independent audit of CET logic | ❌ Blocked |
| 6 — Production | T1 promotion with fail-closed defaults | ❌ Blocked |

---

## 4. Gap Analysis

### 4.1 G-DL1: Oracle Schnorr Verification (Stage 2)

**Current:** `verify_attestation` checks payload consistency but not
cryptographic signatures.

**Gap:** The oracle attestation must be cryptographically verified against
the oracle's announced public key. This is a Schnorr signature verification
using secp256k1.

**Evidence:**
- DLC oracles use BIP340 Schnorr signatures
- The `dlcspecs` test vectors include oracle keys and signatures
- `secp256k1` crate is already in the Gateway workspace (MuSig2, BIP340)

**Promotion gates:**
1. Verify oracle announcement's `pubkey` against BIP340 key format
2. Verify attestation's `signature` against `SHA256(event_id || outcome)` with oracle pubkey
3. Pass all 8 rejection tests from Stage 1 fixture (oracle boundary, wrong key, mutation)
4. Implement in isolated experiment before Gateway integration

### 4.2 G-DL2: CET Construction (Stage 3)

**Current:** No CET, funding, or refund transaction construction exists in
Gateway.

**Gap:** The core DLC value proposition — trustless conditional Bitcoin
payments — requires CET construction with adaptor signatures.

**Evidence:**
- `rust-dlc` v0.8.0 provides CET construction through `Contract` and
  `Party` types
- Adaptor signatures use the same secp256k1 curve as existing MuSig2 code
- The `dlcspecs` defines canonical message formats for Offer, Accept, Sign

**Promotion gates:**
1. Isolated DLC workspace (not Gateway dependency) with CET construction
2. Full vector pass: 14/14 Offer/Accept/Sign byte-perfect
3. 13 rejection cases pass
4. Regtest deployment with two-party DLC lifecycle

### 4.3 G-DL3: Multi-Oracle Threshold (Stage 3+)

**Current:** `ThresholdOracleCoordinator` fetches attestations across N
oracles but does not verify cryptographic signatures.

**Gap:** Production DLCs typically use k-of-n oracle thresholds. The
coordinator must verify signatures from at least k distinct oracles.

**Promotion gates:**
1. Implement per-oracle Schnorr verification (G-DL1)
2. Count valid signatures against threshold
3. Return aggregated outcome when threshold met
4. Fail-closed when threshold not met

---

## 5. Decision Gates Summary

| Gate | Status | Blocking |
|------|--------|----------|
| SDK comparison (Stage 0) | ✅ Complete | — |
| Vector conformance (Stage 1) | ✅ 13/14 | — |
| Deterministic fixture | ✅ Complete | — |
| Oracle Schnorr (Stage 2) | ❌ G-DL1 | Stage 3 |
| CET construction (Stage 3) | ❌ G-DL2 | Stage 4+ |
| Multi-oracle threshold | ❌ G-DL3 | Stage 4+ |
| Dependency in workspace | ❌ Rejected | rust-dlc not added |

---

## 6. Cross-References

- **Issue #220:** DLC CET (open, score 58/90)
- **DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md:** Full evidence (35,493 bytes)
- **DLC_STAGE0_SDK_COMPARISON_2026-07-22.md:** SDK comparison
- **DLC_STAGE1_CONFORMANCE_2026-07-22.md:** 13/14 byte-perfect match
- **DLC_STAGE1_FIXTURE_2026-07-22.md:** Deterministic 2-outcome fixture
- **SESSION_SUMMARY_2026-07-20.md:** CET removal from main (cb8b680)
- **SESSION_SUMMARY_2026-07-22_DLC_220.md:** Research alignment session
- **SESSION_SUMMARY_2026-07-22_DLC_STAGE0.md:** Stage 0 SDK comparison
- **SESSION_SUMMARY_2026-07-22_DLC_STAGE1.md:** Stage 1 conformance
- **SESSION_SUMMARY_2026-07-22_DLC_STAGE1_FIXTURE.md:** Stage 1 fixture

---

## 7. Recommendations

1. **Proceed with Stage 2 (G-DL1) in isolated experiment.** Schnorr
   verification is low-risk: `secp256k1` is already in the workspace, and
   test vectors exist. Do NOT add any dependency to the Gateway workspace.

2. **Maintain strict isolation from Gateway.** All DLC work must remain in
   `experiments/dlc-stage*` until Stage 4 (public testnet). This prevents
   another `cb8b680` revert scenario.

3. **Fix the hyperbola vector mismatch.** The deterministic fixed-point vs
   IEEE-754 discrepancy at byte 104 should be investigated — use
   `rust_decimal` or similar for exact-precision payout computation.

4. **Defer Gateway integration until Stage 4.** The DLC ecosystem is active
   but not yet at institutional maturity. Research-only classification is
   correct for now.
