# Gap Analysis & Risk Scoring (2026-09-04 — Comprehensive Review)

This document tracks identified gaps across the Conxian Gateway portfolio, scored by Risk, Impact, and Effort. **Updated with Canton eUTXO / CBTC non-custodial attestation research, BRICS financial systems mapping, and full cross-repository CI/CD reconciliation.**

---

## 1. Scoring Methodology

| Dimension | Scale | Description |
|-----------|-------|-------------|
| **Risk** | 1-5 | Potential for security breach, data loss, regulatory non-compliance, or service disruption |
| **Impact** | 1-5 | Benefit to platform capability, developer adoption, or institutional readiness |
| **Effort** | 1-5 | Estimated engineering hours (1=<4h, 2=1d, 3=2-3d, 4=1wk, 5=>1wk) |
| **Priority** | Risk × Impact | Higher = address first |

---

## 2. Resolved Gap Matrix

| ID | Gap Description | Domain | Resolution |
|:---|:---|:---|:---|
| **G-11** | Missing Rust CI workflow | CI/CD | ✅ Created `rust-ci.yml` (build, test, clippy, fmt, audit) |
| **G-12** | Identity integration tests gated | Testing | ✅ Added `--features mock-integrations` to CI |
| **G-13** | Control-Plane smoke test config mismatch | Testing | ✅ Added `playwright.config.ts` for Playwright test runner |
| **G-14** | Unmaintained dependencies (derivative, paste) | Security | ✅ Documented ignore in `audit.toml`; upstream fork available |
| **G-15** | rust-toolchain vs Cargo.toml version mismatch | Hygiene | ✅ Aligned to Rust 1.97 baseline across `rust-toolchain.toml` & workflows |
| **G-16** | Skeleton Python verification scripts | Hygiene | ✅ Created `scripts/verify_gateway.py` (7 checks) |
| **G-17** | audit.toml compatibility with cargo-audit | CI/CD | ✅ Added `stale = false` under `[database]` |
| **G-18** | Missing Prometheus metrics + structured tracing | Observability | ✅ Added `/metrics` endpoint, `RUST_LOG_FORMAT=json` |
| **G-19** | Duplicate treasury `tests.rs` file | Code Quality | ✅ Deleted duplicate; inline tests in `mod.rs` |
| **G-22** | NWC relay integration tests missing | Testing | ✅ Created `nwc_tests.rs` (5 tests) |
| **G-24** | No fiat webhook HMAC integration test | Testing | ✅ Added 3 HMAC integration tests |
| **G-25** | No DLC bond integration test | Testing | ✅ Added `POST /api/v1/dlc/bond` + 2 tests |
| **G-26** | No MuSig2 key aggregation test | Testing | ✅ Added `POST /api/v1/musig2/aggregate-keys` + 1 test |
| **G-27** | Docker image not published | CI/CD | ✅ Added `docker/build-push-action` to release workflow |
| **G-FI1** | ISO 20022 XML Schema Validation | Technical | ✅ Implemented structural XML validation & namespace checks in `zkc.rs` |
| **G-FI2** | ISO 20022 pacs.008 Payment Initiation | Technical | ✅ Implemented `pacs.008.001.08` XML generator & schema validator |
| **G-BB1** | Babylon EOTS double-sign key extraction | Technical | ✅ Schnorr attestation & key extraction $x = (s_1 - s_2)/(e_1 - e_2) \pmod n$ |
| **G-FM1** | Fedimint blind signature verification missing | Technical | ✅ Schnorr blind signature verification against guardian pubkeys implemented |
| **G-SB3** | sBTC L1 tx / block header verification missing | Technical | ✅ Double-SHA256 raw tx verification & 80-byte header PoW verification implemented |
| **G-DL1** | DLC Schnorr oracle attestation verification | Technical | ✅ Schnorr BIP340 verification active in `dlc_oracle.rs` |

---

## 3. Open Gap Matrix (Sorted by Priority)

| ID | Gap Description | Domain | Risk | Impact | Effort | Priority | Status |
|:---|:---|:---|:---:|:---:|:---:|:---:|:---|
| **G-C1** | CBTC non-custodial attestation verification for Canton wrapped BTC | Canton | 2 | 5 | 2 | **10** | 🟢 Initiated (Candidate I) |
| **G-C4** | Canton state translation adapter (Daml ACS anchor → Bitcoin UCR) | Canton | 2 | 4 | 4 | **8** | 🟡 Research |
| **G-20** | BitVM3 adapter is research-only — no integration tests or verifying impl | Technical | 2 | 4 | 4 | **8** | 🟡 Fail-Closed |
| **G-B6** | No mBridge validator node deployment capability | BRICS | 2 | 4 | 5 | **8** | 🟡 Research |
| **G-21** | RGB adapter is stub-only (shadow mode, no rgb-core dependency) | Technical | 1 | 4 | 5 | **4** | 🟡 Research |
| **G-1** | Dependency re-pinning (`conxius-enclave-sdk` v2.0.17 via `lib-conxian-core`) | Hygiene | 1 | 3 | 1 | **3** | 🟡 Pending Release |

---

## 4. Build & Test Health Dashboard (2026-09-04)

| Check | Status | Notes |
|-------|--------|-------|
| `cargo test --workspace` | ✅ Pass | All crates & integration test suites compile and pass |
| `verify_contamination_guard.py` | ✅ Pass | All production paths clean (92 files scanned) |
| `verify_tracked_artifacts.py` | ✅ Pass | Zero prohibited artifacts or cache binaries tracked |
| `verify_release_hygiene.py` | ✅ Pass | Release hygiene verified against v0.1.5 baseline |
| `pnpm --filter @conxian/client-sdk test` | ✅ Pass | Vitest suite 6/6 passed |

---

## 5. Current Audit & Adapter Verification Summary

All core settlement adapters (Babylon, Fedimint, sBTC, DLC, ISO 20022, World ID, Web3.bio) are active and verified with structural or cryptographic checks. High-level research candidates (CBTC Non-Custodial Reserve Verification Candidate I, Canton State Translation Candidate J) are prioritized for non-custodial institutional integration.
