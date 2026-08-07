# Conxian Gateway: Agent Instructions (Session 58, Aug 2026)

> **Archive**: `docs/archive/AGENTS_archive_session_58.md` (full session history)

## Core Philosophy
- **Sovereignty**: All code must prioritize non-custodial logic and user sovereignty.
- **Institutional Grade**: Maintain SLA-grade interfaces, high-performance async Rust, and robust error handling.
- **Compliance Pipe**: The gateway is a pass-through for compliance data (ZKC), not a storage for PII.

## Technical Standards
- **Rust Edition**: 2021, **MSRV**: 1.96
- **Framework**: Axum (HTTP), Tokio (Runtime)
- **Security**: Mandatory Bearer token auth for sensitive endpoints.
- **Observability**: Prometheus metrics and structured tracing required for all new modules.
- **Persistence**: Any stateful component must use the atomic persistence layer.

## Current State
- **Version**: v0.1.5 (Cargo.toml)
- **lib-conxian-core**: v0.3.2 (tag `v0.3.2`, published to crates.io)
- **Dependency**: `lib-conxian-core = { git = "...", tag = "v0.3.2" }`
- **Rust toolchain**: 1.97.1 effective floor (workspace baseline)

## Architecture
The `conxian_core` crate alias maps to `lib-conxian-core` v0.3.2 via Cargo.toml. Use `conxian_core::` as the import prefix throughout gateway code.

## Module Map
- `/cmd/gateway`: Entry point, configuration, and wiring.
- `/internal/engine`: Blockchain listeners and RPC clients.
- `/internal/api`: REST interface, handlers, and auth middleware.
- `/internal/compliance`: ZKC (Zero-Knowledge Compliance) and MVCR logic.
- `/pkg/conxian-core`: Shared models, error types, and persistence logic.

## Verification Protocol
Before submitting changes:
1. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
2. `cargo fmt --all -- --check`
3. `cargo test --workspace && cargo test --workspace --features mock-integrations`
4. `pnpm install && pnpm build && pnpm test`
5. `GET /api/v1/health` returns HTTP 200 with `{"status":"ok"}`
6. `python3 scripts/verify_contamination_guard.py`

## CI/CD Pipelines
- **rust-ci.yml**: Format, clippy, test (incl. mock-integrations), release build.
- **lightning-coverage.yml**: Lightning scoped coverage gate (≥90%).
- **cargo-audit.yml**: Weekly dependency audit.
- **secret-scan.yml**: Gitleaks secret scanning.
- **node-ci.yml**: TypeScript build + vitest across all Node workspaces.
- **release.yml**: Tag-triggered, fail-closed GitHub Release with production Gateway archive, checksum manifest, CycloneDX 1.5 SBOM, SLSA provenance subjects, and protected publication job.
