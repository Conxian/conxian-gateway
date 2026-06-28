# Conxian Gateway: Agent Instructions

You are working on the **Conxian Gateway**, an institutional-grade Rust middleware designed for high-performance Bitcoin/Stacks state logic and enterprise compliance.

## Core Philosophy
- **Sovereignty**: All code must prioritize non-custodial logic and user sovereignty.
- **Institutional Grade**: Maintain SLA-grade interfaces, high-performance async Rust, and robust error handling.
- **Compliance Pipe**: The gateway is a pass-through for compliance data (ZKC), not a storage for PII.

## Technical Standards
- **Rust Edition**: 2021
- **MSRV**: 1.85 (toolchain: 1.96.0)
- **Framework**: Axum (HTTP), Tokio (Runtime)
- **Security**: Mandatory Bearer token auth for sensitive endpoints.
- **Observability**: Prometheus metrics and structured tracing are required for all new modules.
- **Persistence**: Any stateful component must use the atomic persistence layer.

## Verification Protocol
Before submitting changes, you MUST:
1. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
2. Run `cargo fmt --all -- --check`
3. Run all tests: `cargo test --workspace` AND `cargo test --workspace --features mock-integrations`
4. Run `pnpm install && pnpm build && pnpm test`
5. Verify health check: `GET /api/v1/health` returns `healthy`.
6. Run `python3 scripts/verify_contamination_guard.py`

## Module Map
- `/cmd/gateway`: Entry point, configuration, and wiring.
- `/internal/engine`: Blockchain listeners and RPC clients.
- `/internal/api`: REST interface, handlers, and auth middleware.
- `/internal/compliance`: ZKC (Zero-Knowledge Compliance) and MVCR logic.
- `/pkg/conxian-core`: Shared models, error types, and persistence logic.

## CI/CD Pipelines (2026-06-28)
- **rust-ci.yml**: Format, clippy, test (incl. mock-integrations), release build — runs on PR/push to main/staged/dev
- **lightning-coverage.yml**: Lightning scoped coverage gate (≥90%) + clippy + fmt
- **cargo-audit.yml**: Weekly dependency audit + on push/PR to main
- **secret-scan.yml**: Gitleaks secret scanning on PR/push
- **node-ci.yml**: TypeScript build + vitest (client-sdk only)
- **release.yml**: Tag-triggered GitHub Release + optional crates.io publish

## Known Gaps & Active Work (see docs/audit/GAP_ANALYSIS_AND_SCORING.md)

### Resolved (Phase 1+2 – 2026-06-28)
- ✅ G-11: Rust CI workflow — created `rust-ci.yml` (build, test, clippy, fmt, audit)
- ✅ G-12: Identity tests — added to CI via mock-integrations feature
- ✅ G-18: Prometheus metrics + structured tracing — `/metrics` endpoint, `RUST_LOG_FORMAT=json`
- ✅ G-24: Fiat webhook HMAC tests — 3 integration tests (invalid sig, missing sig, tampered)
- ✅ G-25: DLC bond endpoint — `POST /api/v1/dlc/bond` + 2 tests
- ✅ G-26: MuSig2 key aggregation — `POST /api/v1/musig2/aggregate-keys` + 1 test
- ✅ G-16: Python verification scripts — `scripts/verify_gateway.py` (7 checks)
- ✅ G-22: NWC relay tests — `nwc_tests.rs` (5 tests: success, unavailable, rejected, partial, retry)
- ✅ G-13: Control-plane test — playwright.config.ts (FIXED)
- ✅ G-14: Unmaintained deps — documented ignore in audit.toml
- ✅ G-17: Toolchain/Dockerfile mismatch — docker now rust:1.96 (FIXED)
- ✅ G-19: Duplicate test file — deleted (FIXED)
- ✅ G-21: audit.toml stale — added stale=false (FIXED)

### Remaining (Research / Future Roadmap)
- 🔴 G-1: BitVM3 proof verification (garbled circuits)
- 🔴 G-2: RGB contract validation (bulletproofs)
- 🟡 G-3: NWC transport hardening (NIP-47 spontaneous payments)
- 🟡 G-4: Groth16 recursion on Bitcoin (MNT curves)
- 🟡 G-5: Elements/Liquid peg-in/out E2E tests
- 🟡 G-6: Rootstock Powpeg anchor verification
- 🟢 G-7: RISC Zero STF verification
- 🟢 G-8: control-plane SSO (NextAuth)

### Test Suite (2026-06-28)
- **119 Rust tests**: 0 failures
- **8 test files**: api_tests (37), nwc_tests (5), offline_pos_tests (1), reorg_simulation_tests (1), identity_tests (16), + inline tests across crates
- **1 Python script**: verify_gateway.py (7 checks)
- **Node.js**: client-sdk (1 test pass), control-plane (1 smoke test — Playwright browser needed in CI)

## Research Context (2026-06-28)

### Blockchain & Protocol Research
- **BitVM3**: Published design (bitvm.org/bitvm3.pdf). Garbled circuits + BitHash. >1,000× smaller disputes vs BitVM2. Monitor chainwayxyz/bitvm-zk-verifier for beta.
- **RGB Protocol**: v0.12 (RGB-I.0) production release. rgb-core v0.12.0 on crates.io. Tether announced USDT on RGB.
- **Nostr Wallet Connect (NIP-47)**: Draft but widely implemented. nostr-sdk v0.25.0 has nip47 feature. Conxian has NwcConnection skeleton ready.
- **Groth16 Recursion**: Experimental on Bitcoin. MNT-curve demo on BSV. Not mainnet-ready. Monitor Citrea/Clementine progress.

### Global Financial Systems Research (BRICS+ vs G7)
Full analysis: `docs/research/BRICS_FINANCIAL_SYSTEMS_RESEARCH.md`

- **Western Bloc** (~45% GDP): SWIFT/CHIPS, ISO 20022, USD/EUR dominance. USD FX reserves slipped from ~70% to ~58%.
- **BRICS+ Bloc** (~40% GDP): CIPS ($24.47T in 2024, 1,690 participants), mBridge (MVP phase, EVM-compatible CBDC bridge), SPFS (550 participants, under sanctions), BRICS Pay DCMS (pilot, decentralized messaging).
- **Co-dependence reality**: >80% of CIPS transactions still use SWIFT transport. RMB is ~3% of global payments. Complete decoupling is unlikely short-term.
- **Conxian strategy**: Dual-stack architecture — ISO 20022 for G7 corridors AND BRICS-specific protocols (CIPS, mBridge, SPFS) for alternative rails. Sanctions-resilience by design.
- **Active BRICS gaps**: G-B1 (CIPS normalization), G-B2 (multi-currency FX), G-B3 (BRICS Pay research), G-B4 (sanctions-risk tagging, Priority 16), G-B5 (PAPSS), G-B6 (mBridge validator).

## Ethical Alignment
The Conxian Protocol is built to empower individuals and institutions within the Stacks/Bitcoin ecosystem. The dual-stack settlement architecture supports financial sovereignty across both Western and BRICS-aligned jurisdictions. Avoid any "dark patterns" or custodial shortcuts.
