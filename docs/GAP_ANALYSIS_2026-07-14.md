# Conxian Gateway — Gap Analysis Report
**Date:** 2026-07-14  
**Review Agent:** OpenHands  
**Scope:** All 11 open issues vs. actual codebase

---

## Executive Summary

This report provides a systematic gap analysis comparing all 11 open GitHub issues against the actual codebase implementation. Issues are classified by implementation status.

| Status | Count | Issues |
|--------|-------|--------|
| ✅ Complete | 2 | #228 (Phase 1), #222 (mostly) |
| ⚠️ Partial | 4 | #236, #220, #218, #193, #199 |
| ❌ Not Implemented | 3 | #219, #216, #202 |
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

**Status:** ⚠️ Partial — Oracle abstraction exists, CET construction NOT done

**Code Verified:**
- ✅ `dlc_oracle.rs`:
  - `DlcOracleClient` — HTTP client for oracle communication
  - `OracleAnnouncement` / `OracleAttestation` structs
  - `list_announcements()`, `get_attestation()`, `verify_attestation()`
  - `ThresholdOracleCoordinator` for multi-oracle support
- ✅ `POST /api/v1/dlc/bond` in routes.rs

**Missing:**
1. ❌ **dlc-manager** — NOT in Cargo.toml
2. ❌ **dlc_messages** — NOT in workspace
3. ❌ **CET construction** — No Contract Execution Transaction building
4. ❌ **Real bond-ID generation** — Only stubs

**Recommendation:** Issue correctly identifies work needed. Oracle abstraction is foundation; dlc-manager integration is next step.

---

### #219: [BITVM] Define Groth16 verifier boundary and test-vector contract

**Status:** ❌ Not Implemented — Groth16 verifier boundary missing

**Code Verified:**
- ❌ **Groth16 NOT found** — Searched entire codebase for `Groth16|groth16|ark_groth16` — **zero matches**
- ❌ No Groth16 dependencies in Cargo.toml

**What Exists:**
- ✅ `risc0_verifier.rs` — RISC Zero STF verifier (Bonsai, Boundless, Local)
- ✅ `bitvm_adapter.rs` — Stub adapter (returns 0 for height)
- ✅ `citrea_adapter.rs` — ZK-rollup adapter

**Required Actions:**
- [ ] Define internal Groth16 verification trait/interface
- [ ] Specify public inputs and witness expectations
- [ ] Add fixture-driven tests validating the boundary
- [ ] Document BitVM adapter → verifier surface integration

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

**Status:** ❌ Not Implemented — BTC header-chain verification missing

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

**Status:** ⚠️ Partial — Oracle client done, CET construction NOT

**Code Verified:**
- ✅ `dlc_oracle.rs` — Oracle client with attestation verification
- ✅ Unit tests for oracle verification

**Missing:**
- ❌ `dlc-manager` / `dlc_messages` dependencies
- ❌ Real CET construction
- ❌ rust-dlc integration

**Recommendation:** Aligns with #220. Oracle abstraction is foundation; dlc-manager is next.

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

#199 (DLC oracle) ←── #220 (CET construction) ←── dlc-manager

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
3. **#220 DLC CET Construction** — Add `dlc-manager` dependency
4. **#219 Groth16 Verifier** — Define internal trait boundary
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
| #219 | TBD | Create Groth16 verifier boundary |

---

*Report generated by OpenHands review agent — 2026-07-14*
