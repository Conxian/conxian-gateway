# Conxian Gateway — Gap Analysis Report
**Date:** 2026-07-14  
**Review Agent:** OpenHands  
**Scope:** All 11 open issues vs. actual codebase

> **Historical snapshot:** This report captures the 2026-07-14 review. Dated
> correction and current-status notes added later supersede stale entries; this
> document is not a live issue-state tracker.

---

## Executive Summary

This report provides a systematic gap analysis comparing all 11 open GitHub issues against the actual codebase implementation. Issues are classified by implementation status.

> **Continuity correction — 2026-07-20:** A follow-up check found that `main`
> contained a partial `groth16_verifier.rs` trait skeleton, so the original
> “not implemented” label understated the existing code while overstating the
> completed contract. The focused `charlie/issue-219-groth16-boundary` branch
> defines the canonical statement contract, commitment public-input binding,
> circuit/key association, validation, BitVM handoff, fixture, and rejection
> tests. It is not merged in this phase and does not provide a production
> cryptographic Groth16 backend.

> **DLC research alignment — 2026-07-22:** The focused review in
> [`docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`](research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md)
> confirms that the gateway has an HTTP oracle scaffold, not a cryptographic
> DLC implementation. `verify_attestation()` checks event ID, oracle public key,
> and expected outcome but does not verify the supplied signature; there is no
> DLC dependency, funding/CET/refund builder, or real bond construction on
> `main`. The next checkpoint is an isolated API/vector comparison of pinned
> `rust-dlc v0.8.0` and DDK `v1.1.2`, not an automatic `dlc-manager` dependency
> addition.

| Status | Count | Issues |
|--------|-------|--------|
| ✅ Complete | 2 | #228 (Phase 1), #222 (mostly) |
| ⚠️ Partial | 6 | #236, #220, #219, #218, #193, #199 |
| ❌ Not Implemented | 1 | #216 |
| 🔬 Research Only | 2 | #189, #202 |

---

## Issue-by-Issue Analysis

### #236: [P0] Publish TypeScript SDK to npm

**Status:** ⚠️ Partial — Implementation exists with alignment issues

**Code Verified:**
- ✅ `packages/client-sdk` with `@conxian/client-sdk` name
- ✅ `private: true` in package.json (needs change for npm)
- ✅ Methods: `getHealth()`, `getSupportedChains()`, `getChainHeight()`, `prepareTransaction()`, `verifyStateProof()`, `createDlcBond()`, `aggregateMuSig2Keys()`
- ✅ API endpoints aligned in `routes.rs`

**Critical Issues Found:**
1. ❌ **README claims "Production Ready (v0.1.0)"** — Issue specifically flags this as overstated
2. ❌ **Version drift** — SDK `package.json` shows `0.1.0`, workspace shows `0.1.4`
3. ⚠️ **No monetization clarity** — No docs on sandbox vs. production flows

**Required Actions:**
- [ ] Update README: Remove "Production Ready" → "Developer Preview" or "Proof-of-Concept"
- [ ] Bump SDK version to `0.1.4` to match workspace
- [ ] Add stability markers to package.json exports
- [ ] Document which endpoints are production-ready vs. experimental

---

### #228: [RGB] Full rgb-std stash resolver integration (G-1385)

**Status:** ✅ Phase 1 Complete, Phase 2 Pending

**Code Verified:**
- ✅ `rgb_stash.rs` — `StashResolver` with:
  - File-backed JSON contract metadata cache
  - Bech32m format validation
  - Contract lookup with in-memory HashMap
  - Transition verification with format + stash checks
  - Esplora UTXO query method (`check_utxo`)
- ✅ `rgb_native.rs` — Wires StashResolver into adapter chain
- ✅ Dependencies: `rgb-std` v0.12.0-rc.3, `bp-esplora` v0.12.0-rc.3
- ✅ Feature-gated behind `rgb-native`

**Phase 2 Missing (Blockers):**
1. **Stockpile trait** — `rgb-std` v0.12 only defines trait, no concrete impl
2. **Full ContractVerify trait** — Not implemented
3. **Consignment import/export** — Not implemented
4. **Seal closure verification** — UTXO check exists, full verification incomplete

**Recommendation:** Issue correctly scoped. Phase 2 blocked on ecosystem stabilization.

---

### #222: Enforce strict CI/CD baseline for autonomous verification and release

**Status:** ✅ Mostly Implemented — Minor gaps remain

**Code Verified:**
- ✅ `rust-ci.yml`: Format, Clippy, Test, Build with pinned actions (SHA-hashes)
- ✅ `release.yml`: SBOM (CycloneDX), SLSA L3 provenance attestation, version validation
- ✅ `cargo-audit.yml`: Weekly dependency audit
- ✅ `secret-scan.yml`: Gitleaks scanning
- ✅ `dependency-review.yml`: Dependency review
- ✅ `node-ci.yml`: TypeScript SDK build + vitest
- ✅ Version drift RESOLVED — `v0.1.4` release exists

**Minor Gaps:**
1. ⚠️ **Coverage threshold** — `lightning-coverage.yml` exists, no mandatory gate in rust-ci.yml
2. ⚠️ **Rollback path** — Not documented in RELEASE.md
3. ⚠️ **SDK version drift** — `packages/client-sdk/package.json` shows `0.1.0` instead of `0.1.4`

**Recommendation:**
- Add explicit coverage threshold to rust-ci.yml
- Document rollback procedure in RELEASE.md
- Fix SDK package.json version

---

### #220: [DLC] Build CET construction path with local oracle-fixture verification

**Status:** ⚠️ Partial — HTTP oracle scaffold exists; cryptographic verification and CET construction are NOT done

**Code Verified:**
- ✅ `dlc_oracle.rs`:
  - `DlcOracleClient` — HTTP client for oracle communication
  - `OracleAnnouncement` / `OracleAttestation` structs
  - `list_announcements()`, `get_attestation()`, `verify_attestation()` — field matching only; the supplied signature is not cryptographically verified
  - `ThresholdOracleCoordinator` for multi-oracle support
- ✅ `POST /api/v1/dlc/bond` in routes.rs

**Missing:**
1. ❌ **dlc-manager** — NOT in Cargo.toml
2. ❌ **dlc_messages** — NOT in workspace
3. ❌ **CET construction** — No Contract Execution Transaction building
4. ❌ **Real bond-ID generation** — Only stubs

**Recommendation:** Issue correctly identifies work needed. Start with the pinned
[`DLC ecosystem research and readiness gates`](research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md),
then compare `rust-dlc v0.8.0` and DDK `v1.1.2` in an isolated spike before
selecting a dependency. The first implementation milestone should be a
deterministic enumerated-outcome offer/accept/sign, funding/CET/refund fixture
with real announcement and attestation signature verification.

> **2026-07-22 isolated checkpoint:** The follow-up
> [`DLC_STAGE1_CONFORMANCE_2026-07-22.md`](research/DLC_STAGE1_CONFORMANCE_2026-07-22.md)
> adds a documented in-memory payout-field compatibility path, deterministic
> hyperbola mismatch evidence, and eight deterministic checks: one valid oracle
> boundary plus seven rejection checks (six oracle cases, including signed
> outcome mutation and correctly signed unannounced-outcome rejection, plus one
> transaction-binding case). It does not satisfy the full fixture/readiness
> gate and does not change the production-status assessment above.

---

### #219: [BITVM] Define Groth16 verifier boundary and test-vector contract

**Status:** ⚠️ Partial — the canonical boundary milestone is defined on the focused 2026-07-20 branch; production cryptographic verification remains unimplemented

**Code Verified:**
- ✅ `internal/engine/src/bitcoin/groth16_verifier.rs` — initial trait/types existed on `main`; the focused branch hardens them into a backend-neutral canonical contract with circuit-bound commitment limbs and key association
- ✅ `internal/engine/src/bitcoin/bitvm_adapter.rs` — metadata adapter remains, with an explicit validated Groth16 envelope handoff on the focused branch
- ❌ No production Groth16 pairing backend or prover dependency (intentionally out of scope)

**What Exists:**
- ✅ `risc0_verifier.rs` — RISC Zero STF verifier (Bonsai, Boundless, Local)
- ✅ `bitvm_adapter.rs` — Legacy metadata adapter (height remains `0` in the chain-state path) plus a validated Groth16 envelope handoff on the focused branch
- ✅ `citrea_adapter.rs` — ZK-rollup adapter

**Required Actions:**
- [x] Define and harden the internal Groth16 verification trait/interface
- [x] Specify canonical public-input, witness-commitment, proof, key, and block-context expectations
- [x] Add fixture-driven tests validating the boundary and rejection cases
- [x] Document BitVM adapter → verifier surface integration
- [ ] Add a production cryptographic backend after the boundary is reviewed and merged

---

### #218: [LIQUID] Build Elements-based local harness for peg-in / peg-out E2E tests

**Status:** ⚠️ Partial — Adapter exists, harness missing

**Code Verified:**
- ✅ `liquid_adapter.rs`:
  - RPC integration via `Arc<dyn BitcoinRpc>`
  - `get_latest_height()` via RPC
  - `verify_state_proof()` in shadow mode
  - Prepared transaction skeleton

**Missing:**
1. ❌ **Local Elements harness** — No `elementsd` daemon
2. ❌ **Peg-in fixtures** — No deterministic test vectors
3. ❌ **Peg-out fixtures** — No representative workflow
4. ❌ **CI integration** — No harness in test pipeline

**Recommendation:** Issue correctly reframed from "Liquid testnet E2E" to "local Elements harness". Aligns with #193.

---

### #216: [BABYLON] Implement BTC header-chain query + verification path before EOTS work

**Status:** ❌ Not Implemented — BTC header-chain verification missing; PR #253 remains open

**Code Verified:**
- ✅ `babylon_adapter.rs`:
  - `get_chain_identity()` returns Babylon network ID
  - `prepare_unsigned_transaction()` returns staking payload
  - `verify_state_proof()` in rehearsal mode

**Missing:**
1. ❌ **BTC header-chain query** — `get_latest_height()` returns `0`
2. ❌ **Header verification path** — No SPV-style validation
3. ❌ **EOTS integration** — Not mentioned in code

**Recommendation:** Issue title says "before EOTS" — BTC header-chain is prerequisite.

---

### #202: [RESEARCH] Babylon adapter — Cosmos SDK light client verification

**Status:** 🔬 Research Only — No code implementation

**Code Verified:**
- ❌ No Cosmos SDK integration found
- ❌ No light client verification
- ❌ No Cosmos-related dependencies

**Assessment:** Correctly labeled as research. #216 is the implementation companion.

---

### #199: [RESEARCH] DLC oracle integration — rust-dlc for real CET construction & adaptor signatures

**Status:** ⚠️ Partial — Oracle HTTP client scaffold exists, but cryptographic attestation verification and CET construction are NOT done

**Code Verified:**
- ✅ `dlc_oracle.rs` — Oracle HTTP client with event/public-key/outcome matching
- ✅ Unit tests for oracle verification

**Missing:**
- ❌ `dlc-manager` / `dlc_messages` dependencies
- ❌ Real CET construction
- ❌ rust-dlc integration

**Recommendation:** Align with #220 and the pinned
[`DLC ecosystem research`](research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md).
Do not treat the current adapter as cryptographic attestation verification or
select `dlc-manager` without the dependency/API and vector checkpoint.

---

### #193: [RESEARCH] G-5: Liquid peg-in/peg-out E2E tests

**Status:** ⚠️ Partial — Adapter exists, harness missing

**Code Verified:**
- ✅ `liquid_adapter.rs` — RPC-based adapter
- ✅ SPV header verification documented (per issue)

**Missing:**
- ❌ Local Elements regtest harness
- ❌ Deterministic peg-in/out fixtures
- ❌ CI integration

**Recommendation:** Aligns with #218.

---

### #189: [RESEARCH] G-1/G-20: BitVM3 adapter — garbled circuits & recursive proof verification

**Status:** 🔬 Research Only — No BitVM3 implementation

**Code Verified:**
- ✅ `risc0_verifier.rs` — RISC Zero verifier
- ✅ RISC Zero ecosystem (v3.0.5) per docs
- ❌ No `bitvm3` crate
- ❌ No GC implementation

**Research Status (per issue):**
- ✅ BitVMX-CPU open source (could evaluate)
- ❌ BitVMX-GC targeting 2026 (closed source)
- ❌ GOATNetwork/bitvm2-gc as POC target
- ✅ Issue correctly gated behind toolkit maturity

**Recommendation:** Maintain research gate. Monitor BitVMX-CPU and GOATNetwork.

---

## Cross-Issue Dependencies

```
#236 (SDK) ─────┬── #222 (CI/CD fixes)
                └── #228 (RGB v0.12) ←── Phase 2 blocked

#199 (DLC oracle) ←── #220 (CET construction) ←── pinned SDK/API spike

#193 (Liquid E2E) ←── #218 (Local harness) ←── elementsd

#189 (BitVM3) ←── #219 (Groth16 boundary) ←── ark_groth16

#216 (Babylon) ←── #202 (Cosmos SDK research)
```

---

## Priority Recommendations

### P0 — Immediate Action Required
1. **#236 SDK Version Drift** — Fix SDK `package.json` to `0.1.4`
2. **#236 README Overclaim** — Remove "Production Ready" claim

### P1 — High Priority
3. **#220 DLC CET Construction** — Compare pinned `rust-dlc v0.8.0` and DDK `v1.1.2` APIs/vectors first; add a dependency only after the spike selects and verifies one path
4. **#219 Groth16 Verifier** — Review and merge the canonical boundary milestone; keep cryptographic backend work separate
5. **#216 Babylon BTC Header** — Implement SPV verification

### P2 — Medium Priority
6. **#218/#193 Liquid Harness** — Local Elements setup
7. **#222 Rollback Docs** — Document rollback procedure
8. **#228 Phase 2** — Monitor `rgb-std` ecosystem stabilization

### P3 — Watch & Monitor
9. **#189 BitVM3/GC** — Evaluate BitVMX-CPU, monitor GOATNetwork
10. **#202 Cosmos SDK** — Maintain research status

---

## Files Requiring Updates

| Issue | File | Change Required |
|-------|------|-----------------|
| #236 | `packages/client-sdk/package.json` | Version: `0.1.0` → `0.1.4` |
| #236 | `packages/client-sdk/README.md` | Remove "Production Ready" claim |
| #222 | `RELEASE.md` | Add rollback procedure |
| #222 | `.github/workflows/rust-ci.yml` | Add coverage threshold gate |
| #220 | `internal/engine/Cargo.toml` | Add `dlc-manager` dependency |
| #219 | `internal/engine/src/bitcoin/groth16_verifier.rs`, `bitvm_adapter.rs`, `internal/engine/tests/`, `docs/GROTH16_VERIFIER_CONTRACT.md` | Canonical boundary, BitVM handoff, fixture, and rejection tests on focused branch; production backend remains open |

---

*Report generated by OpenHands review agent — 2026-07-14*
