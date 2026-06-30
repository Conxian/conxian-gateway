# Conxian Gateway: Agent Instructions

You are working on the **Conxian Gateway**, an institutional-grade Rust middleware designed for high-performance Bitcoin/Stacks state logic and enterprise compliance.

## Current State (2026-06-30)
- **PR #210** (`feat/implement-all-recommendations`): **MERGED** — all 8 protocol integrations
- **PR #209** (`feat/issue-hygiene-auth-and-research`): **Open** — hygiene fixes (#208), GitHub OAuth (#196), research consolidation
- **Issues resolved** (#208, #196): Hygiene + GitHub OAuth (implemented in #209)
- **All 14 original issues**: implemented (8 in #210 merged, 2 in #209 open, 4 in earlier merges)
- **CI status**: All 6 workflows green on both main and PR #209

### Protocol Implementations (PR #210)
| Issue | Protocol | Crate | File |
|---|---|---|---|
| #191 | NWC NIP-47 | nwc 0.44.0, nostr-sdk 0.44.1 | `internal/api/src/nwc_backend.rs` |
| #194 | Rootstock JSON-RPC | reqwest 0.12 | `internal/engine/src/ntt/rootstock_adapter.rs` |
| #195 | RISC Zero STF | risc0-zkvm 3.0.5 | `internal/engine/src/bitcoin/risc0_verifier.rs` |
| #198 | NWC relay | — | `internal/api/src/handlers.rs` |
| #200 | ISO 20022 camt | cam0814 1.0.10 | `internal/api/src/camt.rs` |
| #201 | World ID | reqwest 0.12 | `internal/api/src/world_id.rs` |
| #189 | RGB | rgb-lib 0.3.0-beta.6 | `internal/engine/src/bitcoin/rgb_adapter.rs` |
| #188 | DLC Oracle | ddk 1.1.2 | `internal/engine/src/bitcoin/dlc_oracle.rs` |

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

## CI/CD Pipelines (2026-06-29)
- **rust-ci.yml**: Format, clippy, test (incl. mock-integrations), release build — runs on PR/push to main/staged/dev
- **lightning-coverage.yml**: Lightning scoped coverage gate (≥90%) + clippy + fmt
- **cargo-audit.yml**: Weekly dependency audit + on push/PR to main
- **secret-scan.yml**: Gitleaks secret scanning on PR/push
- **node-ci.yml**: TypeScript build + vitest (client-sdk only)
- **release.yml**: Tag-triggered GitHub Release + optional crates.io publish

## Known Gaps & Active Work (see docs/audit/GAP_ANALYSIS_AND_SCORING.md)

### Resolved (Phase 1+2+3 – 2026-06-29)
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
- ✅ G-B4: Sanctions-risk tagging — implemented risk engine + blocking (FIXED)
- ✅ G-B1: CIPS normalization — ISO 20022 CIPS variant support (FIXED)
- ✅ G-B2: Multi-currency FX — RMB/RUB/INR/AED tracking in TreasuryMonitor (FIXED)
- ✅ G-B5: PAPSS settlement — Pan-African rail integration (FIXED)
- ✅ G-23: Lightning coverage — HTML/LCOV reports in CI (FIXED)
- ✅ G-8: control-plane SSO (NextAuth) — implemented in PR #209

### Remaining (Research / Future Roadmap)
- 🔴 G-1: BitVM3 proof verification (garbled circuits)
- 🔴 G-2: RGB contract validation (bulletproofs)
- 🟡 G-3: NWC transport hardening (NIP-47 spontaneous payments)
- 🟡 G-4: Groth16 recursion on Bitcoin (MNT curves)
- 🟡 G-5: Elements/Liquid peg-in/out E2E tests
- 🟡 G-6: Rootstock Powpeg anchor verification
- 🟢 G-7: RISC Zero STF verification

### Test Suite (2026-06-30)
- **125 Rust tests** (base): 0 failures
- **128 Rust tests** (with mock-integrations): 0 failures
- **8 test files**: api_tests (43), nwc_tests (5), offline_pos_tests (1), reorg_simulation_tests (1), identity_tests (16), + inline tests across crates
- **cargo fmt**: clean | **cargo clippy (--all-features)**: clean | **--all-features build**: passes
- **pnpm lockfile**: synced with next-auth@5.0.0-beta.31
- **1 Python script**: verify_gateway.py (7 checks)

## Research Context (2026-06-29)

### Blockchain & Protocol Research (Deep-dive 2026-06-28)
- **BitVM3**: Research paper only (eprint 2026/933). No code, no SDK. Production today = BitVM2+Groth16 (Clementine/Citrea mainnet Jan 2026).
- **RGB Protocol**: Two incompatible forks. v0.12 (LNP-BP, Dr. Orlovsky) STALLED 12 months. v0.11.1 (rgb-protocol org, rgb-lib 0.3.0-beta.6) ACTIVE — Tether USD₮ launched here.
- **Nostr Wallet Connect (NIP-47)**: nwc 0.44.0, nostr-sdk 0.44.1. Stable, 156K downloads. Quickest integration win.
- **Groth16/Citrea**: Clementine v0.6.4, audited, mainnet since Jan 2026. risc0-zkvm 3.0.5 + risc0-groth16 3.0.4 + ark-groth16 0.5.0. First ZK rollup on Bitcoin.
- **RISC Zero**: v2.0.0 YANKED — use v3.0.5 stable. Boundless Market 2.0.1 for decentralized proving.
- **LDK Node**: v0.7.0 production. 151K downloads. BOLT12 offers, LSPS1/2/5 LSP, async payments (experimental). Replaces SimulatedLightningBackend.
- **DLC Dev Kit (DDK)**: v1.1.2 (Jun 29, 2026). Nostr transport (NIP-44), Kormir oracle. High complexity (6-10 wks).
- **World ID**: REST API v4. Millions verified. world-id-primitives 0.12.0. Trivial server-side integration (1-2 wks).
- **Babylon**: Mainnet live, bbn-1. Rust crates stale 12mo (babylon-proto 0.14.0). gRPC/REST approach (2-4 wks).
- **ISO 20022 camt.053/054**: open-payments-iso20022-camt 1.0.10 (vendor from crates.io, GitHub repo deleted).
- **Rootstock**: RSKj Vetiver 9.0.3 (Java). JSON-RPC bridge queries (1-2 wks). No Rust SDK needed.
- **Liquid**: elements 0.26.2 (Jun 2026, 410K downloads). Active. Adapter has real RPC — needs E2E tests.

### Global Financial Systems Research (BRICS+ vs G7)
Full analysis: `docs/research/BRICS_FINANCIAL_SYSTEMS_RESEARCH.md`

- **Western Bloc** (~45% GDP): SWIFT/CHIPS, ISO 20022, USD/EUR dominance. USD FX reserves slipped from ~70% to ~58%.
- **BRICS+ Bloc** (~40% GDP): CIPS ($24.47T in 2024, 1,690 participants), mBridge (MVP phase, EVM-compatible CBDC bridge), SPFS (550 participants, under sanctions), BRICS Pay DCMS (pilot, decentralized messaging).
- **Co-dependence reality**: >80% of CIPS transactions still use SWIFT transport. RMB is ~3% of global payments. Complete decoupling is unlikely short-term.
- **Conxian strategy**: Dual-stack architecture — ISO 20022 for G7 corridors AND BRICS-specific protocols (CIPS, mBridge, SPFS) for alternative rails. Sanctions-resilience by design.
- **Active BRICS gaps**: ✅ G-B1 (CIPS normalization), ✅ G-B2 (multi-currency FX), G-B3 (BRICS Pay research), ✅ G-B4 (sanctions-risk tagging), ✅ G-B5 (PAPSS), G-B6 (mBridge validator).

## Ethical Alignment
The Conxian Protocol is built to empower individuals and institutions within the Stacks/Bitcoin ecosystem. The dual-stack settlement architecture supports financial sovereignty across both Western and BRICS-aligned jurisdictions. Avoid any "dark patterns" or custodial shortcuts.
