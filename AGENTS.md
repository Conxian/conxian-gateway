# Conxian Gateway: Agent Instructions

You are working on the **Conxian Gateway**, an institutional-grade Rust middleware designed for high-performance Bitcoin/Stacks state logic and enterprise compliance.

## Current State (2026-07-05)
- **Status Audit**: Holistic review of Nexus/Gateway alignment complete (CON-1353).
- **Protocol Drift**: Identified that **Fedimint**, **Citrea**, and **Strata** adapters are MISSING from production paths despite being marked Done in Linear.
- **Hardening Stubs**: CON-1276 requirements (Redis auth, token expiry) exist as code comments but are not yet implemented.
- **UCV-1**: Fully implemented and unifying Babylon, BitVM2, Liquid, Rootstock, and RGB.
- **CI status**: All 6 workflows green on main.

### Protocol Implementations (Update 2026-07-05)
| Protocol | Status | File |
|---|---|---|
| NWC NIP-47 | ✅ Integrated | `internal/api/src/nwc_backend.rs` |
| Rootstock | ✅ Integrated | `internal/engine/src/ntt/rootstock_adapter.rs` |
| Babylon | ✅ Integrated | `internal/engine/src/bitcoin/babylon_adapter.rs` |
| BitVM2 | ✅ Integrated | `internal/engine/src/bitcoin/bitvm_adapter.rs` |
| RGB | ✅ v0.12 Native | `internal/engine/src/bitcoin/rgb_adapter.rs` + `rgb_native.rs` |
| Liquid | ✅ Integrated | `internal/engine/src/bitcoin/liquid_adapter.rs` |
| Citrea | ✅ Integrated | `internal/engine/src/bitcoin/citrea_adapter.rs` |
| RISC Zero | 🟡 Unwired | `internal/engine/src/bitcoin/risc0_verifier.rs` |
| Fedimint | 🔴 MISSING | N/A |
| Strata | 🔴 MISSING | N/A |
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
- 🔴 G-1: BitVM3 proof verification (garbled circuits) - Research only.
- 🔴 G-16: Fedimint adapter missing implementation.
- 🔴 G-08: Strata adapter missing implementation (awaiting mainnet Q3 2026).
- 🟡 G-1385: Full rgb-std stash resolver integration (rgb-native is format-validation only).
- 🟡 G-1276: Enforce authenticated Redis and token expiry (documented stubs).
- 🟡 G-1380: Add SBOM and Provenance to release workflow.
- 🟡 G-1389: Reduce technical debt (dead_code suppressions).

## Ethical Alignment
The Conxian Protocol is built to empower individuals and institutions within the Stacks/Bitcoin ecosystem. The dual-stack settlement architecture supports financial sovereignty across both Western and BRICS-aligned jurisdictions. Avoid any "dark patterns" or custodial shortcuts.
