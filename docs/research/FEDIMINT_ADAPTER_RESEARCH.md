# Fedimint Adapter: Evidence Review & Integration Strategy

**Status:** Research Scaffold (CON-1304) | **Lines:** 122
**Last refreshed:** 2026-08-07 | **Session:** 49

---

## Executive Summary

Fedimint is a community-custody protocol that enables Chaumian e-cash mints on
Bitcoin. Multiple guardians collectively hold funds in a federation, and users
receive blind-signed e-cash tokens representing claims on the federation's
Bitcoin reserves. Conxian Gateway has a **research scaffold adapter**
(`FedimintAdapter`) implementing the `ChainAdapter` trait for Fedimint mint
verification.

**Current state:** The adapter validates blinded-signature proofs, checks
quorum thresholds (≥2/3 guardians), and maps to T2 Managed trust tier.
`verify_state_proof` is rehearsal-only — it checks non-empty blinded signatures
but does not perform cryptographic verification. No Fedimint SDK dependency
exists.

**Decision:** Fedimint remains a T2 Boundary adapter. The scaffold is
structurally complete for proof boundary validation. Promotion to T1 requires
Fedimint SDK integration and cryptographic verification of guardian signatures.

---

## 1. Protocol Evidence

### 1.1 Fedimint Specification

Fedimint is defined by a modular protocol specification and implemented in Rust:

- **Fedimint docs:** <https://fedimint.org/docs>
- **GitHub:** <https://github.com/fedimint/fedimint>
- **Consensus:** Guardian federation with configurable threshold (typically 2/3+1 or 3/5)
- **Modules:** Mint (e-cash), Wallet (on-chain BTC), Lightning (LN gateway)
- **Privacy:** Chaumian blind signatures — guardians cannot link issuance to redemption

### 1.2 Mainnet Evidence

Fedimint has been operational on Bitcoin mainnet since 2023. The protocol is
used by community mints for privacy-preserving group custody. Key evidence:

- **Fedimint v0.4+:** Stable Rust implementation with modular architecture
- **Guardian model:** Threshold BFT consensus, no single point of failure
- **Federation size:** Typically 3-5 guardians (odd numbers for quorum)
- **Lightning integration:** Gateway module for LN payments from mint to external nodes

### 1.3 Trust Model

| Property | Value |
|----------|-------|
| Custody model | Federation (m-of-n threshold) |
| Privacy | Chaumian blind signatures (untraceable e-cash) |
| Finality | Guardian consensus (not Bitcoin L1) |
| Trust tier (Gateway) | T2 — Managed |

---

## 2. Current Gateway Implementation

### 2.1 Architecture

```
FedimintAdapter implements ChainAdapter
    │
    ├─ validate_fedimint_consensus(proof_metadata)
    │   └─ Checks blinded_signature non-empty
    │   └─ Checks quorum_guardians >= 2/3 of total guardians
    │   └─ Maps to FedimintMint at T2 Managed trust tier
    │
    ├─ verify_state_proof(proof)
    │   └─ Rehearsal mode: checks blinded_signature present
    │   └─ Does NOT verify cryptographic signature
    │
    └─ prepare_unsigned_transaction()
        └─ Returns mint_operation envelope (placeholder)
```

### 2.2 Capabilities

| Capability | Status | Notes |
|-----------|--------|-------|
| ChainAdapter impl | ✅ Complete | 6 trait methods implemented |
| Blind sig proof validation | ⬜ Rehearsal | Non-empty check only; no crypto |
| Quorum threshold check | ✅ Live | ≥2/3 guardian validation |
| Trust tier mapping | ✅ Live | T2 Managed for all Fedimint mints |
| Mint operation envelope | ✅ Scaffold | Placeholder structure |
| SDK integration | ❌ None | No `fedimint-client` dependency |

---

## 3. Gap Analysis

### 3.1 G-FM1: Cryptographic Verification (P1 — High Priority)

**Current:** `verify_state_proof` checks that `blinded_signature` is non-empty
but does not verify it against guardian public keys.

**Gap:** The adapter cannot cryptographically verify that a mint token was
signed by the claimed guardians. This prevents Trust Tier promotion to T1.

**Evidence:**
- Fedimint Rust SDK: `fedimint-client` crate with `Client` struct
- Guardian keys are published in federation config
- Blind signature verification requires the federation's public key set

**Promotion gates:**
1. Evaluate `fedimint-client` SDK for API stability and license compatibility
2. Implement `FedimintVerifier` using guardian public key set
3. Verify blind signatures against federation config
4. Add positive/negative test vectors with known guard sets
5. Wire into `UniversalVerifier` chain registry

### 3.2 G-FM2: Federation Discovery (P2 — Medium Priority)

**Current:** The adapter has no mechanism to discover or validate federation
configurations. Federation metadata must be provided out-of-band.

**Gap:** For production use, the Gateway needs a way to discover active
fedimints and validate their guardian configurations.

**Evidence:**
- Federation invite codes encode connection info
- Fedimint federation config includes guardian pubkeys, module configs, API endpoints

**Promotion gates:**
1. Define `FedimintFederationConfig` struct
2. Implement federation discovery via invite code
3. Validate guardian set threshold meets Gateway minimums
4. Expose configured federations via `GET /api/v1/fedimint/federations`

### 3.3 G-FM3: E-Cash Audit Support (P3 — Research)

**Current:** The adapter validates mint operations but has no insight into
e-cash issuance, redemption, or supply.

**Gap:** For compliance (BRICS, sanctions), the Gateway may need to verify
that a Fedimint mint is not facilitating sanctioned transactions.

**Note:** Chaumian e-cash is inherently privacy-preserving — transaction
tracing is not possible. This gap represents a fundamental tension between
privacy and compliance that requires governance-level resolution.

---

## 4. Decision Gates Summary

| Gate | Status | Blocking |
|------|--------|----------|
| ChainAdapter impl | ✅ Complete | — |
| Blind sig proof shape validation | ✅ Rehearsal | — |
| Quorum threshold check | ✅ Live | — |
| Cryptographic blind sig verification | ❌ G-FM1 | Fedimint SDK |
| Federation discovery | ❌ G-FM2 | Design |
| E-cash audit | ❌ G-FM3 | Governance |
| T1 promotion | ❌ Blocked | G-FM1 + G-FM2 |

---

## 5. Cross-References

- **ADAPTER_FAMILY_STRATEGY.md:** Fedimint at T2 Boundary
- **CON-1304:** Jira ticket for Fedimint integration
- **BRICS_FINANCIAL_SYSTEMS_RESEARCH.md:** Privacy-compliance tension for e-cash
- **ChainAdapter trait:** `pkg/conxian-core/src/lib.rs`

---

## 6. Recommendations

1. **Maintain T2 Boundary classification.** The scaffold is appropriate for
   proof-shape validation without cryptographic verification.

2. **Evaluate Fedimint SDK before G-FM1.** The `fedimint-client` crate must
   be assessed for license compatibility (MIT/Apache 2.0 likely), API
   stability, and maintenance status.

3. **Defer G-FM3 pending governance.** E-cash privacy creates a fundamental
   tension with BRICS sanctions compliance that requires ExCo-level guidance.

4. **Add structured error variants.** Replace generic `ConxianError` with
   Fedimint-specific errors (e.g., `GuardianThresholdNotMet`,
   `BlindedSignatureVerificationFailed`).
