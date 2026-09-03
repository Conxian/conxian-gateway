# Gap Analysis & Risk Scoring (2026-06-29 — Comprehensive Review)

This document tracks identified gaps across the Conxian Gateway portfolio, scored by Risk, Impact, and Effort. **Updated with BRICS financial systems research, multi-currency settlement mapping, and full CI/CD reconciliation.**

---

## 1. Scoring Methodology

| Dimension | Scale | Description |
|-----------|-------|-------------|
| **Risk** | 1-5 | Potential for security breach, data loss, regulatory non-compliance, or service disruption |
| **Impact** | 1-5 | Benefit to platform capability, developer adoption, or institutional readiness |
| **Effort** | 1-5 | Estimated engineering hours (1=<4h, 2=1d, 3=2-3d, 4=1wk, 5=>1wk) |
| **Priority** | Risk × Impact | Higher = address first |

---

## 2. Resolved Gap Matrix (Phase 1+2+3 — 2026-06-29)

| ID | Gap Description | Domain | Resolution |
|:---|:---|:---|:---|
| **G-11** | Missing Rust CI workflow | CI/CD | ✅ Created `rust-ci.yml` (build, test, clippy, fmt, audit) |
| **G-12** | Identity integration tests gated | Testing | ✅ Added `--features mock-integrations` to CI |
| **G-13** | Control-Plane smoke test config mismatch | Testing | ✅ Added `playwright.config.ts` for Playwright test runner |
| **G-14** | Unmaintained dependencies (derivative, paste) | Security | ✅ Documented ignore in `audit.toml`; upstream fork available |
| **G-15** | rust-toolchain vs Cargo.toml version mismatch | Hygiene | ✅ Docker was aligned to `rust:1.96`; the former MSRV 1.85 policy was superseded on 2026-07-26 by the declared and CI-tested Rust 1.96 baseline |
| **G-16** | Skeleton Python verification scripts | Hygiene | ✅ Created `scripts/verify_gateway.py` (7 checks) |
| **G-17** | audit.toml compatibility with cargo-audit | CI/CD | ✅ Added `stale = false` under `[database]` |
| **G-18** | Missing Prometheus metrics + structured tracing | Observability | ✅ Added `/metrics` endpoint, `RUST_LOG_FORMAT=json` |
| **G-19** | Duplicate treasury `tests.rs` file | Code Quality | ✅ Deleted duplicate; inline tests in `mod.rs` |
| **G-22** | NWC relay integration tests missing | Testing | ✅ Created `nwc_tests.rs` (5 tests) |
| **G-24** | No fiat webhook HMAC integration test | Testing | ✅ Added 3 HMAC integration tests |
| **G-25** | No DLC bond integration test | Testing | ✅ Added `POST /api/v1/dlc/bond` + 2 tests |
| **G-26** | No MuSig2 key aggregation test | Testing | ✅ Added `POST /api/v1/musig2/aggregate-keys` + 1 test |
| **G-27** | Docker image not published | CI/CD | ✅ Added `docker/build-push-action` to release workflow |

---

## 3. Open Gap Matrix (Sorted by Priority)

| ID | Gap Description | Domain | Risk | Impact | Effort | Priority | Status |
|:---|:---|:---|:---:|:---:|:---:|:---:|:---|
| **G-20** | BitVM3 adapter is research-only — no integration tests or verifying impl | Technical | 2 | 5 | 4 | **10** | 🟡 Research |
| **G-21** | RGB adapter is stub-only (shadow mode, no rgb-core dependency) | Technical | 1 | 4 | 5 | **4** | 🟡 Research |
| **G-B3** | No BRICS Pay DCMS connector research or feasibility study | BRICS | 2 | 3 | 2 | **6** | 🟡 Research |
| **G-B6** | No mBridge validator node deployment capability | BRICS | 2 | 5 | 5 | **10** | 🟡 Research |

### Previously Resolved Gaps (Historical)

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
| **G-B4** | Sanctions-risk tagging — implemented risk engine + blocking | BRICS | ✅ Resolved (Phase 3) |
| **G-B1** | CIPS normalization — ISO 20022 CIPS variant support | BRICS | ✅ Resolved (Phase 3) |
| **G-B2** | Multi-currency FX — RMB/RUB/INR/AED tracking | BRICS | ✅ Resolved (Phase 3) |
| **G-B5** | PAPSS settlement — Pan-African rail integration | BRICS | ✅ Resolved (Phase 3) |
| **G-23** | Lightning coverage — HTML/LCOV reports in CI | Testing | ✅ Resolved (Phase 3) |

---

## 4. Build & Test Health Dashboard (2026-06-29)

| Check | Status | Notes |
|-------|--------|-------|
| `cargo build --release` | ✅ Pass | All 5 crates + gateway binary |
| `cargo test --workspace` | ✅ 125/125 passed | Including mock-integrations |
| `cargo clippy --all-targets` | ✅ Clean | 2 minor warnings in test files only |
| `cargo fmt --check` | ✅ Clean | All files formatted |
| `cargo audit` | ✅ Clean | 369 deps, 0 vulnerabilities |
| `pnpm install` | ⚠️ Warnings | `unrs-resolver` build scripts (harmless) |
| `pnpm build` | ✅ Pass | client-sdk, schemas, control-plane (11 routes) |
| `pnpm test` | ⚠️ 1 fail | smoke.spec.ts — Playwright browser missing in CI (G-13 resolved; env issue) |
| `verify_contamination_guard.py` | ✅ Pass | 48+ files clean |

---

## 5. Detailed Remediation Plans (Open Gaps Only)

### 🟢 RESOLVED — BRICS Sanctions & Settlement (Priority ≥ 12)
All critical gaps in this domain (Sanctions Risk, CIPS Normalization) have been resolved in Phase 3.


---

## 6. Current Audit & Adapter Verification Resolution Update

| ID | Gap Description | Domain | Resolution |
|:---|:---|:---|:---|
| **G-FM1** | Fedimint blind signature verification missing | Technical | ✅ Resolved — Schnorr verification against guardian pubkeys implemented in `internal/engine/src/bitcoin/fedimint_adapter.rs` |
| **G-SB3** | sBTC L1 tx / block header verification missing | Technical | ✅ Resolved — Double-SHA256 raw tx verification & 80-byte block header PoW verification implemented in `internal/engine/src/stacks/sbtc.rs` |
| **G-BB1** | Babylon EOTS double-sign key extraction | Technical | ✅ Resolved — Schnorr attestation & key extraction $x = (s_1 - s_2)/(e_1 - e_2) \pmod n$ implemented |
| **G-FI2** | ISO 20022 pacs.008 payment initiation | Technical | ✅ Resolved — `pacs.008.001.08` XML builder and schema validation implemented |
