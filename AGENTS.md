# Conxian Gateway: Agent Instructions

You are working on the **Conxian Gateway**, an institutional-grade Rust middleware designed for high-performance Bitcoin/Stacks state logic and enterprise compliance.

## Current State (2026-07-05, updated)
- **Status Audit**: Holistic review of Nexus/Gateway alignment complete (CON-1353).
- **Protocol Drift**: Resolved — Fedimint, Citrea, and Strata adapters implemented and in production paths.
- **RGB G-1385 (Phase 1)**: StashResolver delivered (commit `124d17e`) with `rgb-std` v0.12.0-rc.3 + `bp-esplora` v0.12.0-rc.3 behind `rgb-native` feature. Phase 2 (ContractVerify, consignment) blocked on rgb-std ecosystem stabilization.
- **PR #233**: Tech debt reduction approved, flaky persistence test fixed, awaiting CI green on `con-1389` branch.
- **Hardening Stubs**: CON-1276 requirements (Redis auth, token expiry) exist as code comments but are not yet implemented.
- **UCV-1**: Fully implemented and unifying Babylon, BitVM2, Liquid, Rootstock, and RGB.
- **CI status**: All 6 workflows green on main.

### Protocol Implementations (2026-07-05)
| Protocol | Status | File |
|---|---|---|
| NWC NIP-47 | ✅ Integrated | `internal/api/src/nwc_backend.rs` |
| Rootstock | ✅ Integrated | `internal/engine/src/ntt/rootstock_adapter.rs` |
| Babylon | ✅ Integrated | `internal/engine/src/bitcoin/babylon_adapter.rs` |
| BitVM2 | ✅ Integrated | `internal/engine/src/bitcoin/bitvm_adapter.rs` |
| RGB | ✅ v0.12 + Stash (P1) | `internal/engine/src/bitcoin/rgb_adapter.rs` + `rgb_native.rs` + `rgb_stash.rs` |
| Liquid | ✅ Integrated | `internal/engine/src/bitcoin/liquid_adapter.rs` |
| Citrea | ✅ Integrated | `internal/engine/src/bitcoin/citrea_adapter.rs` |
| RISC Zero | 🟡 Unwired | `internal/engine/src/bitcoin/risc0_verifier.rs` |
| Fedimint | ✅ Integrated | `internal/engine/src/bitcoin/fedimint_adapter.rs` |
| Strata | ✅ Testnet | `internal/engine/src/bitcoin/strata_adapter.rs` |
| BitVMX GC | 🟡 Pending 2026 | N/A |
| BRICS Pay | 🟡 Research only | N/A |
| mBridge | 🟡 Research only | N/A |

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

## CI/CD Pipelines
- **rust-ci.yml**: Format, clippy, test (incl. mock-integrations), release build.
- **lightning-coverage.yml**: Lightning scoped coverage gate (≥90%).
- **cargo-audit.yml**: Weekly dependency audit.
- **secret-scan.yml**: Gitleaks secret scanning.
- **node-ci.yml**: TypeScript build + vitest (client-sdk only).
- **release.yml**: Tag-triggered GitHub Release.

## Known Gaps (2026-07-05 Update)
- 🟡 #228: Full rgb-std stash resolver (rgb-native is format-validation only)
- 🟡 #189: BitVMX GC adapter — pending 2026 garbled circuits release
- 🟡 #231: BRICS Pay research — DCMS settlement rail classification
- 🟡 #232: mBridge research — BIS multi-CBDC DLT assessment
- 🟡 G-1276: Enforce authenticated Redis and token expiry
- 🟡 G-1380: Add SBOM and Provenance to release workflow
- 🟡 G-1389: Reduce technical debt (dead_code suppressions)

Protocol drift resolved — 9 of 9 identified protocols now have adapters.
All pending gaps have corresponding GitHub issues for autonomous pickup.

## Ethical Alignment
The Conxian Protocol is built to empower individuals and institutions within the Stacks/Bitcoin ecosystem. The dual-stack settlement architecture supports financial sovereignty across both Western and BRICS-aligned jurisdictions. Avoid any "dark patterns" or custodial shortcuts.
