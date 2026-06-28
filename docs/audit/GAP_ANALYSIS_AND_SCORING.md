# Gap Analysis & Risk Scoring (2026-06-28 — Comprehensive Review)

This document tracks identified gaps across the Conxian Gateway portfolio, scored by Risk, Impact, and Effort. **Updated with full repository audit, CI/CD review, test analysis, and web research.**

---

## 1. Scoring Methodology

| Dimension | Scale | Description |
|-----------|-------|-------------|
| **Risk** | 1-5 | Potential for security breach, data loss, regulatory non-compliance, or service disruption |
| **Impact** | 1-5 | Benefit to platform capability, developer adoption, or institutional readiness |
| **Effort** | 1-5 | Estimated engineering hours (1=<4h, 2=1d, 3=2-3d, 4=1wk, 5=>1wk) |
| **Priority** | Risk × Impact | Higher = address first |

---

## 2. Full Gap Matrix (Sorted by Priority)

| ID | Gap Description | Domain | Risk | Impact | Effort | Priority | Status |
|:---|:---|:---|:---:|:---:|:---:|:---:|:---|
| **G-11** | Missing Rust CI workflow (rust-ci.yml) | CI/CD | 4 | 5 | 2 | **20** | 🔴 Open |
| **G-12** | Identity integration tests gated behind mock-integrations feature | Testing | 3 | 4 | 2 | **12** | 🔴 Open |
| **G-13** | Control-Plane smoke test uses Playwright but runs via vitest (config mismatch) | Testing | 2 | 4 | 2 | **8** | 🔴 Open |
| **G-14** | Unmaintained dependencies: derivative v2.2.0, paste v1.0.15 | Security | 3 | 2 | 2 | **6** | 🟡 Open |
| **G-15** | rust-toolchain (1.96.0) vs Cargo.toml rust-version (1.85) mismatch | Hygiene | 2 | 2 | 1 | **4** | 🟡 Open |
| **G-16** | 5 Python verification scripts are stubs with no real validation | Hygiene | 2 | 3 | 2 | **6** | 🟡 Open |
| **G-17** | audit.toml incompatible with newer cargo-audit (missing [database].stale) | CI/CD | 2 | 3 | 1 | **6** | 🟡 Open |
| **G-18** | No Prometheus metrics endpoint or structured tracing in API layer | Observability | 3 | 4 | 3 | **12** | 🔴 Open |
| **G-19** | Treasury tests.rs duplicates treasury/mod.rs (dead code) | Code Quality | 1 | 1 | 1 | **1** | 🟢 Open |
| **G-20** | BitVM3 adapter is research-only — no integration tests or verifying impl | Technical | 2 | 5 | 4 | **10** | 🟡 Research |
| **G-21** | RGB adapter is stub-only (shadow mode, no rgb-core dependency) | Technical | 1 | 4 | 5 | **4** | 🟡 Research |
| **G-22** | NostrWalletConnect has full impl but no relay integration tests | Technical | 2 | 4 | 3 | **8** | 🟡 Open |
| **G-23** | Lightning adapter coverage gate script exists but no coverage report generated in CI | Testing | 2 | 3 | 1 | **6** | 🟡 Open |
| **G-24** | No integration/E2E test for fiat webhook HMAC verification path | Testing | 3 | 4 | 3 | **12** | 🔴 Open |
| **G-25** | No DLC bond integration test (CON-1269 exists in SDK, no backend test) | Testing | 3 | 3 | 3 | **9** | 🟡 Open |
| **G-26** | No MuSig2 key aggregation integration test (CON-1270) | Testing | 2 | 3 | 2 | **6** | 🟡 Open |
| **G-27** | Docker image not published to any registry (no container CI step) | CI/CD | 2 | 4 | 2 | **8** | 🟡 Open |

### Previously Resolved Gaps

| ID | Gap Description | Domain | Status |
|:---|:---|:---|:---|
| G-01 | Missing CI Validation Scripts (CON-1322) | Security | ✅ Done |
| G-02 | Production Lightning Backend Skeleton | Technical | 📋 Backlog |
| G-03 | Missing Flagship Technical Whitepaper | Docs | 📋 Backlog |
| G-04 | Missing Developer Quickstart & Guide | Docs | 📋 Backlog |
| G-05 | Release workflow implemented | CI/CD | ✅ Done (release.yml exists) |
| G-06 | Dependency Review fail-on-error disabled | Security | ✅ Verified |
| G-07 | Actions/Checkout version drift | CI/CD | ✅ Done (pinned to SHAs) |
| G-08 | Tier 2 Adapters (Liquid/Babylon) Shadow-Mode | Technical | 🔄 Active |
| G-09 | BitVM3 / Recursive Proof Research | Research | 🔄 Active |
| G-10 | Missing docs/governance/CHANGELOG.md | Docs | ✅ Exists |

---

## 3. Build & Test Health Dashboard (2026-06-28)

| Check | Status | Notes |
|-------|--------|-------|
| `cargo build --release` | ✅ Pass | All 4 crates + gateway binary |
| `cargo test --workspace` | ✅ 106/106 passed | identity_tests gated (0 run) |
| `cargo clippy -- -D warnings` | ✅ Clean | Zero warnings |
| `cargo fmt -- --check` | ✅ Clean | All files formatted |
| `cargo audit` | ⚠️ 2 warnings | derivative, paste unmaintained |
| `pnpm install` | ⚠️ Warnings | ERR_PNPM_IGNORED_BUILDS |
| `pnpm build` | ✅ Pass | client-sdk, schemas, control-plane |
| `pnpm test` | ❌ 1 fail | smoke.spec.ts — vitest/playwright mismatch |
| `verify_contamination_guard.py` | ✅ Pass | 48 files clean |
| Python verification scripts (×8) | ✅ Pass | 5 are stubs though |

---

## 4. Detailed Remediation Plans

### 🔴 CRITICAL (Priority ≥ 12)

#### G-11: Missing Rust CI Workflow
- **Risk**: No CI enforcement of Rust build, test, clippy, or fmt on PRs. Only Lightning coverage runs Rust checks.
- **Impact**: Regression risk on every PR. No automated quality gate.
- **Fix**: Create `.github/workflows/rust-ci.yml` that runs on PR/push to main/staged/dev:
  ```yaml
  - cargo fmt --all -- --check
  - cargo clippy --workspace --all-targets --all-features -- -D warnings
  - cargo test --workspace
  - cargo build --release
  ```
- **Effort**: 2 (1 day)

#### G-12: Identity Integration Tests Gated
- **Risk**: Identity resolution (ENS, BNS, WorldID) never tested in CI. Feature gates prevent execution.
- **Impact**: Integration regressions in identity bridge go undetected.
- **Fix**: Run `cargo test --workspace --features mock-integrations` in CI, OR refactor tests to use conditional compilation differently.
- **Effort**: 2 (1 day)

#### G-18: Missing Observability (Prometheus + Tracing)
- **Risk**: No production monitoring capability. AGENTS.md requires "Prometheus metrics and structured tracing for all new modules."
- **Impact**: Cannot monitor SLA, cannot detect anomalies in production.
- **Fix**: Add `tower-http` metrics layer, expose `/metrics` endpoint, add `tracing-opentelemetry` or structured JSON tracing subscriber.
- **Effort**: 3 (2-3 days)

#### G-24: No Fiat Webhook HMAC Integration Test
- **Risk**: Fiat webhook verification is a security-critical path. No E2E test validates HMAC-SHA256 verification with real webhook payloads.
- **Impact**: Regulatory risk if fiat webhooks fail silently in production.
- **Fix**: Add integration test that sends a signed webhook payload and verifies the HMAC validation.
- **Effort**: 3 (2-3 days)

### 🟡 HIGH (Priority 8-11)

#### G-13: Control-Plane Test Runner Mismatch
- **Fix**: Either add `playwright.config.ts` and run via `npx playwright test`, OR convert smoke.spec.ts to vitest-compatible format.
- **Effort**: 2

#### G-20: BitVM3 Adapter Research → Pilot
- **Fix**: Based on research (BitVM3 PDFs published, JS/Rust tooling available), begin integrating `bitvm-zk-verifier` toolkit into the BitVM adapter. Write integration tests.
- **Effort**: 4

#### G-22: NostrWalletConnect Relay Integration Tests
- **Fix**: Add tests that connect to a local Nostr relay (or mock) and verify NIP-47 message construction and parsing.
- **Effort**: 3

#### G-25: DLC Bond Integration Test
- **Fix**: Add test for `/api/v1/dlc/bond` endpoint verifying bond creation, oracle pubkey validation, and timelock enforcement.
- **Effort**: 3

#### G-27: Docker Image Publishing
- **Fix**: Add `docker/build-push-action` step to release workflow to publish to GHCR.
- **Effort**: 2

### 🟢 MEDIUM (Priority 4-7)

#### G-14: Unmaintained Dependencies
- **Fix**: Audit dependency tree for `derivative` and `paste` usage. Replace with alternatives (`derive_more`, manual impls). Or accept risk if usage is minimal.
- **Effort**: 2

#### G-16: Skeleton Python Scripts
- **Fix**: Implement real validation logic in `verify_bos_production_boundary.py`, `verify_compose_env_templates.py`, `verify_pr_bos_classification.py`, `verify_submodule_integrity.py`, `verify_submodule_secret_filenames.py`.
- **Effort**: 2

#### G-17: audit.toml Compatibility
- **Fix**: Add `stale = false` under `[database]` in audit.toml to support newer cargo-audit versions.
- **Effort**: 1

#### G-23: Lightning Coverage Gate in CI
- **Fix**: Verify that `lightning-coverage.yml` produces a coverage report. The gate is configured but no evidence of reports being generated.
- **Effort**: 1

### 🟢 LOW (Priority < 4)

#### G-19: Treasury tests.rs Duplicate
- **Fix**: Delete `internal/engine/src/treasury/tests.rs` (duplicate of inline tests in mod.rs).
- **Effort**: 1

#### G-15: Version Mismatch
- **Fix**: Align `rust-toolchain.toml` channel with `Cargo.toml` rust-version. Recommend using 1.85 (MSRV) in toolchain for CI compatibility.
- **Effort**: 1

---

## 5. Research Findings & Opportunity Mapping

### BitVM3 (Q3-Q4 2026 Opportunity)
- **Status**: BitVM3 design published (bitvm.org/bitvm3.pdf). Uses garbled circuits + BitHash for optimistic SNARK verification with >1,000× smaller disputes vs BitVM2.
- **Conxian Impact**: The BitVM adapter references "BitVM2 SNARK checkpoints" and "364-segment verification." BitVM3 reduces this to compact on-chain fraud proofs (~200 bytes).
- **Action**: Monitor `chainwayxyz/bitvm-zk-verifier` toolkit. Plan BitVM3 adapter when toolkit reaches beta.

### RGB Protocol (Q3 2026 Opportunity)
- **Status**: RGB consensus v0.12 (RGB-I.0) released. `rgb-core` v0.12.0 on crates.io. Tether announced USDT on RGB. Breaking changes from pre-v0.12.
- **Conxian Impact**: RFC_RGB_ADAPTER.md correctly identifies dependencies. `rgb-core` is now production-grade. Adapter can move from shadow to active mode.
- **Action**: Add `rgb-core = "0.12"` dependency. Implement contract state monitoring.

### Nostr Wallet Connect / NIP-47 (Active Opportunity)
- **Status**: NIP-47 is draft but widely implemented (Alby, Damus, CoinOS). `nostr-sdk` v0.25.0 has nip47 feature. `cln-nip47` plugin available for Core Lightning.
- **Conxian Impact**: The `NwcConnection` struct in `nostr.rs` already implements URI parsing and NIP-47 request construction. Missing: relay connection, event signing, and response handling.
- **Action**: Add `nostr-sdk` dependency with `nip47` feature. Implement relay subscription and event signing.

### Groth16 Recursive Proofs (Research)
- **Status**: Experimental. MNT-curve recursion demoed on BSV. Citrea completed trusted setup ceremony. No production-grade Bitcoin mainnet deployment yet.
- **Conxian Impact**: Could optimize BitVM2 SNARK verification (currently 364 segments) with recursive aggregation.
- **Action**: Monitor. Not yet production-ready for Bitcoin mainnet.

### ISO 20022 Expansion
- **Status**: Current implementation generates pacs.008 messages from Job Cards. camt.053 (account statement) and camt.054 (credit notification) formats are standard for institutional treasury reporting.
- **Action**: Extend `SettlementForge` to support camt.053 and camt.054 message types.

---

## 6. Verified Assets Summary

| Asset | Count | Status |
|-------|-------|--------|
| Rust source files (.rs) | 46 | All build, test, lint clean |
| Cargo workspace crates | 5 | conxian-core, engine, compliance, api, gateway |
| Unit/integration tests | 106 | 106 passed, 0 failed |
| Documentation files | 40 | All present and current |
| CI/CD workflows | 6 | 5 active + 1 stub (dependency-review) |
| Python verification scripts | 9 | 4 real, 5 stubs |
| TypeScript packages | 2 | client-sdk, schemas |
| Next.js app | 1 | control-plane (8 pages, 3 components) |
| Shell scripts | 3 | alex_rehearsal, lightning_coverage_gate, toggle_bounties |
| Docker services | 3 | sovereign-node, business-gateway, enterprise-gateway |

---

*Last updated: 2026-06-28 — Full repository audit with web research on BitVM3, RGB, NWC, Groth16*
