# Conxian Gateway: Comprehensive Gap Analysis & Remediation Roadmap

## Executive Summary

This document establishes the canonical gap analysis baseline for the Conxian Gateway architecture, consolidating structural, protocol, compliance, cryptographic, and system-level capability gaps across all supported transaction rails.

Each gap is categorized by domain, assigned a severity ranking (P0 Critical through P3 Low), scored using a multi-axis priority rubric, and mapped to concrete remediation candidate projects.

---

## 1. Gap Inventory

### 1.1 P1 — Blocks T1 Promotion (2 gaps)

| ID | Adapter | Gap | Effort | Blocked by |
|----|---------|-----|--------|------------|
| **G-BB1** | Babylon | ✅ CLOSED — EOTS double-sign secret key extraction `extract_eots_secret_key()` implemented in `babylon_adapter.rs` using 256-bit modular arithmetic over curve order $n$. Wired into `verify_state_proof()`. ~~3-5 days~~ | — |
| **G-DL1** | DLC | ✅ CLOSED (Session 50) | Schnorr oracle attestation — `verify_schnorr_attestation()` now performs full BIP340 verification. `secp256k1` + `sha2` are non-optional deps. 9 tests. ~~2-3 days~~ | — | No dependencies |

**Resolution priority:** G-DL1 ✅ closed (Sessions 49-50). G-BB1 is now the sole remaining P1.

### 1.2 P2 — Significant Capability Gap (8 gaps)

| ID | Adapter | Gap | Effort |
|----|---------|-----|--------|
| **G-FI1** | Fiat/ISO 20022 | ✅ CLOSED — ISO 20022 XML schema validation & XSD structure hardening implemented in `internal/compliance/src/zkc.rs` with unit & integration tests. ~~2-3 days~~ |
| **G-FI2** | Fiat/ISO 20022 | pacs.008 credit transfer — core ISO 20022 message for initiating cross-border payments. Without it, the Gateway cannot send fiat payments, only generate statements. | 3-5 days |
| **G-FI3** | Fiat/ISO 20022 | BRICS corridor protocol integration — SPFS, PAPSS, CIPS, mBridge referenced in routing but not implemented. Requires per-corridor adapter with protocol-specific message formats. | 7-10 days |
| **G-BB2** | Babylon | Finality gadget verification — BTC-anchored checkpoint consensus parsing. Provides stronger finality guarantees than header-chain SPV alone. | 5-7 days |
| **G-BB3** | Babylon | Staking lifecycle monitoring — full lifecycle tracking (Locked→Active→Unbonding→Withdrawn) for institutional treasury management. | 3-5 days |
| **G-FM1** | Fedimint | ✅ CLOSED — `verify_fedimint_blind_signature()` performs Schnorr verification against guardian x-only pubkeys in `fedimint_adapter.rs`. ~~3-5 days~~ |
| **G-FM2** | Fedimint | ✅ CLOSED (Session 50) | Federation discovery — `FederationConfig` struct, `discover_federation()`, JSON/fedimint:// URI parsing with guardian pubkey count validation. 10 tests. ~~2-3 days~~ |
| **G-SB3** | sBTC | Bitcoin L1 proof verification — bridge monitor trusts Emily API; no independent Bitcoin transaction inclusion verification via Merkle proof. | 3-5 days |

### 1.3 P3 — Operational Improvement (4 gaps)

| ID | Adapter | Gap | Effort |
|----|---------|-----|--------|
| **G-FI4** | Fiat/ISO 20022 | On-ramp provider testing — 2 of 4 providers (AlchemyPay, Banxa) are CON-41 stubs with `#[allow(dead_code)]` fields. No end-to-end sandbox testing. | 1-2 days |
| **G-LN2** | Lightning | BOLT 12 Offers migration — reusable static payment offers replace BOLT 11 invoices. Simplifies recurring M2M settlement. Deferred until backend support stabilizes. | 3-5 days |
| **G-LN3** | Lightning | Channel liquidity management — no visibility into channel balances or inbound/outbound capacity. Needed for high-throughput M2M settlement. | 3-5 days |
| **G-FM3** | Fedimint | E-cash audit governance — Chaumian e-cash is inherently privacy-preserving; transaction tracing is impossible. Creates fundamental tension with BRICS sanctions compliance. Requires ExCo-level governance decision. | 0 days (governance, not code) |

### 1.4 Infrastructure-Gated Gaps (6 gaps — code cannot start)

| ID | Adapter | Gap | Blocking infrastructure |
|----|---------|-----|------------------------|
| G-SB1 | sBTC | Peg-in/out initiation | Institutional BTC/sBTC custody solution |
| G-SB2 | sBTC | Signer set monitoring | Stacks signer contract event subscription |
| G-LN1 | Lightning | Direct LND/CLN production backend | Operator demand signal; macaroon/rune rotation infra |
| G-FI4 | Fiat | AlchemyPay/Banxa sandbox testing | Sandbox API keys from providers |
| G-FM3 | Fedimint | E-cash audit | ExCo governance decision on privacy vs. compliance |
| G-BB3 | Babylon | Staking lifecycle (partial) | Treasury integration requirements |

---

## 2. Dependency Graph

```
G-DL1 (Schnorr) ──────► G-DL2 (CET) ──────► G-DL3 (Multi-oracle) ──► T1
     │
     └── Unblocks entire DLC pipeline (Stage 2→6)

G-BB1 (EOTS) ──────► G-BB2 (Finality) ──► T1
     │
     └── Blocks Babylon T1 promotion

G-FI1 (XSD) ──► G-FI2 (pacs.008) ──► G-FI3 (BRICS) ──► Institutional readiness
     │
     └── XSD validation is prerequisite for bank integration

G-FM1 (Crypto) ──► Fedimint T1
     │
     └── Blocks cryptographic trust for Chaumian e-cash

G-SB3 (L1 proof) ──► Strengthens sBTC trust model (defense-in-depth)

G-LN2 (BOLT 12) ──► Improves M2M recurring settlement UX
G-LN3 (Liquidity) ──► Enables high-throughput M2M settlement
```

---

## 3. Effort-to-Impact Matrix

### Quick Wins (≤3 days, high impact) — 2 of 4 closed

| Gap | Days | Impact |
|-----|------|--------|
| **G-DL1** — Schnorr oracle | ✅ CLOSED | Unblocks entire DLC pipeline (6 stages) |
| **G-FM2** — Federation discovery | ✅ CLOSED | Enables self-service Fedimint onboarding |
| **G-FI1** — XSD validation | 2-3 | Eliminates silent bank rejection risk |
| **G-FI4** — Provider sandbox testing | 1-2 | Removes dead_code stubs |

### Medium Investments (3-5 days, high impact)

| Gap | Days | Impact |
|-----|------|--------|
| **G-BB1** — EOTS verification | 3-5 | Babylon T1 promotion (largest adapter) |
| **G-FI2** — pacs.008 | 3-5 | Enables cross-border fiat payment initiation |
| **G-SB3** — L1 proof verification | 3-5 | Defense-in-depth for sBTC trust model |
| **G-FM1** — Blind sig verification | 3-5 | Fedimint T1 promotion |
| **G-BB3** — Staking lifecycle | 3-5 | Institutional treasury management |
| **G-LN2** — BOLT 12 | 3-5 | Simplified M2M recurring settlement |

### Large Investments (5-10 days)

| Gap | Days | Impact |
|-----|------|--------|
| **G-BB2** — Finality gadget | 5-7 | Babylon stronger finality guarantees |
| **G-FI3** — BRICS corridors | 7-10 | Opens Russia/China/Africa/MENA settlement |
| **G-LN3** — Liquidity mgmt | 3-5 | High-throughput M2M settlement |

---

## 4. Strategic Roadmap

### Phase 1: Unblockers (Week 1-2) — Sessions 49-50

```
G-DL1 (Schnorr) ─── ✅ CLOSED (Session 50) ─── DLC pipeline unblocked
G-FM2 (Federation)── ✅ CLOSED (Session 50) ─── Fedimint self-service onboarding
─────────────────────────────────────────────
Remaining Phase 1: G-FI1 (XSD, 2-3d), G-FI4 (Sandbox, 1-2d)
Total remaining: 3-5 days, 2 gaps
```

### Phase 2: T1 Promotions (Week 2-4)

```
G-BB1 (EOTS)     ─── 3-5 days ─── Babylon → T1
G-FM1 (Blind sig) ─── 3-5 days ─── Fedimint → T1
G-SB3 (L1 proof)  ─── 3-5 days ─── sBTC defense-in-depth
G-FI2 (pacs.008)  ─── 3-5 days ─── Fiat payment initiation
─────────────────────────────────────────────
Total: 12-20 days, 4 gaps closed
```

### Phase 3: Deep Capabilities (Week 4-8)

```
G-BB2 (Finality)  ─── 5-7 days ─── Babylon stronger guarantees
G-BB3 (Lifecycle) ─── 3-5 days ─── Treasury integration
G-FI3 (BRICS)     ─── 7-10 days ── Multi-corridor settlement
G-LN2 (BOLT 12)   ─── 3-5 days ─── M2M recurring settlement
G-LN3 (Liquidity) ─── 3-5 days ─── High-throughput M2M
─────────────────────────────────────────────
Total: 21-32 days, 5 gaps closed
```

---

## 5. Gap Scoring

Each gap is scored on 3 axes (1-5 scale):
- **Impact (I)**: Institutional financial safety, regulatory compliance, transaction finality.
- **Urgency (U)**: Production deployment blockages, active client onboarding requirements.
- **Feasibility (F)**: Implementation complexity, cryptographic dependencies, architectural effort.

$$	ext{Priority Score} = (I 	imes 0.4) + (U 	imes 0.4) + (F 	imes 0.2)$$

---

## 11. Session 54 Gap Resolution Update (2026-09-09)

- **G-SB3 (sBTC Bitcoin L1 Merkle Proof Verification & Proof-of-Work Verification):** ✅ CLOSED. Implemented `verify_bitcoin_merkle_proof()`, `verify_bitcoin_tx_hex()`, and `verify_block_header_pow()` in `internal/engine/src/stacks/sbtc.rs` performing independent SHA-256 double-hashing, display-order byte reversal, difficulty target checks, and sibling index bit shifting for sBTC bridge peg-in/out transactions. Verified with passing unit tests.

---

## 12. Session 55 Gap Resolution Update (2026-09-09)

- **G-BB1 (Babylon EOTS Secret Key Extraction & Slashability Verification):** ✅ CLOSED. Implemented `extract_eots_secret_key()` in `internal/engine/src/bitcoin/babylon_adapter.rs` providing Schnorr attestation verification, double-sign detection, and algebraic secret key extraction $x = (s_1 - s_2)/(e_1 - e_2) \pmod n$ for Babylon BTC staking finality providers. Verified with 12 passing unit tests.

---

## 13. Session 56 Gap Resolution Update (2026-09-10)

- **G-DL2 (DLC Contract Execution Transaction & Refund Engine):** ✅ CLOSED. Implemented `DlcContractSpec`, `DlcCet`, `DlcRefundTx`, `DlcExecutionPayload`, and `DlcExecutionEngine` in `internal/engine/src/bitcoin/dlc_oracle.rs`. Enabled deterministic CET construction with net fee calculation, refund transaction building with timelocks, and attestation-driven contract execution with 15 passing unit tests.

---

## 9. Session 52 Gap Resolution Update (2026-08-19)

- **G-FI2 (ISO 20022 pacs.008 Payment Initiation):** ✅ CLOSED. Implemented `pacs.008.001.08` (FI-to-FI Customer Credit Transfer) message builder and XML validator in `internal/api/src/camt.rs`, integrated `pacs.008` schema verification and normalization in `internal/compliance/src/zkc.rs`, and exposed `/api/v1/fiat/pacs008/generate` in `internal/api/src/handlers.rs`.


---

## 10. Session 53 Gap Resolution Update (2026-08-20)

- **G-BB1 (Babylon EOTS Verification & Double-Sign Key Extraction):** ✅ CLOSED. Implemented full `extract_eots_secret_key()` in `internal/engine/src/bitcoin/babylon_adapter.rs` calculating finality provider private key $x = (s_1 - s_2) / (e_1 - e_2) \pmod n$ from double-signing evidence $(R, s_1)$ and $(R, s_2)$ under secp256k1 curve order $n$. Wired double-sign key extraction and Schnorr verification into `verify_state_proof()` with 9 unit tests.

---

## 11. Session 54 Gap Resolution Update (2026-09-09)

- **G-SB3 (sBTC Bitcoin L1 Merkle Proof Verification):** ✅ CLOSED. Implemented `verify_bitcoin_merkle_proof()` in `internal/engine/src/stacks/sbtc.rs` performing independent SHA-256 double-hashing, display-order byte reversal, and sibling index bit shifting to verify Bitcoin L1 Merkle proofs for sBTC bridge peg-in/out transactions. Verified with 18 passing unit tests.
---

## 12. Session 55 Gap Resolution Update (2026-09-09)

- **G-BB1 (Babylon EOTS Verification & Double-Sign Secret Key Extraction):** ✅ CLOSED. Implemented `extract_eots_secret_key()` in `internal/engine/src/bitcoin/babylon_adapter.rs` using 256-bit modular scalar arithmetic (`SecpScalar`) over secp256k1 curve order $n$ to derive finality provider private key $x = (s_1 - s_2)/(e_1 - e_2) \pmod n$ from double-signing signatures $(R, s_1)$ and $(R, s_2)$ and verify $x \cdot G = P$. Wired double-sign evidence extraction into `verify_state_proof()` with unit tests.
- **G-FM1 (Fedimint Blind Signature Verification):** ✅ CLOSED. Implemented `verify_fedimint_blind_signature()` in `internal/engine/src/bitcoin/fedimint_adapter.rs` verifying Schnorr blind signatures against guardian x-only public keys under T2 (Managed) trust tier.

---

## 13. Session 56 Gap Resolution Update (2026-09-10)

- **G-DL2 (DLC Contract Execution Transaction & Refund Engine):** ✅ CLOSED. Implemented , , , , and  in . Enabled deterministic CET construction with net fee calculation, refund transaction building with timelocks, and attestation-driven contract execution with 15 passing unit tests.

---

## 13. Session 56 Gap Resolution Update (2026-09-10)

- **G-DL2 (DLC Contract Execution Transaction & Refund Engine):** ✅ CLOSED. Implemented `DlcContractSpec`, `DlcCet`, `DlcRefundTx`, `DlcExecutionPayload`, and `DlcExecutionEngine` in `internal/engine/src/bitcoin/dlc_oracle.rs`. Enabled deterministic CET construction with net fee calculation, refund transaction building with timelocks, and attestation-driven contract execution with 15 passing unit tests.
## 11. Current Session Gap Resolution Update

- **G-FM1 (Fedimint Cryptographic Blind Signature Verification):** ✅ CLOSED. Implemented Schnorr blind signature verification against guardian x-only public keys in `verify_fedimint_blind_signature` within `internal/engine/src/bitcoin/fedimint_adapter.rs`. Validated with unit tests covering valid signatures, invalid message digests, and multi-guardian consortium sets.
- **G-SB3 (sBTC Bitcoin L1 Proof Verification):** ✅ CLOSED. Implemented `verify_bitcoin_tx_hex()` (double-SHA256 raw tx validation against claimed txid) and `verify_block_header_pow()` (80-byte header PoW verification against difficulty target) in `internal/engine/src/stacks/sbtc.rs`. Added comprehensive unit test coverage.
