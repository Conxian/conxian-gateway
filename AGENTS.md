# Conxian Gateway: Agent Instructions (Session 58, Aug 2026)

> **Archive**: `docs/archive/AGENTS_archive_session_58.md` (full session history)

## Core Philosophy
- **Sovereignty**: All code must prioritize non-custodial logic and user sovereignty.
- **Institutional Grade**: Maintain SLA-grade interfaces, high-performance async Rust, and robust error handling.
- **Compliance Pipe**: The gateway is a pass-through for compliance data (ZKC), not a storage for PII.

## Technical Standards
- **Rust Edition**: 2021, **MSRV**: 1.97
- **Framework**: Axum (HTTP), Tokio (Runtime)
- **Security**: Mandatory Bearer token auth for sensitive endpoints.
- **Observability**: Prometheus metrics and structured tracing required for all new modules.
- **Persistence**: Any stateful component must use the atomic persistence layer.

## Current State
- **Version**: v0.1.5 (Cargo.toml)
- **lib-conxian-core**: v0.3.3 (tag `v0.3.3`, published to crates.io)
- **Dependency**: `lib-conxian-core = { git = "...", tag = "v0.3.3" }`
- **Rust toolchain**: 1.97.1 effective floor (workspace baseline)

## Architecture
The `conxian_core` crate alias maps to `lib-conxian-core` v0.3.3 via Cargo.toml. Use `conxian_core::` as the import prefix throughout gateway code.

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
4. `./scripts/mcp_test_runner.sh --test wiremock_simulation_tests`
5. `pnpm install && pnpm build && pnpm test`
6. `GET /api/v1/health` returns HTTP 200 with `{"status":"ok"}`
7. `python3 scripts/verify_contamination_guard.py`

## API Virtualization, WireMock & Chaos Testing Conventions
- **Stateful API Virtualization**: Use `wiremock` (`MockServer`) following WireMock Cloud / Proxymock methodologies to virtualize external financial and blockchain dependencies (e.g. ISO 20022 clearing gateways, X402 settlement LND nodes, World ID, Web3.bio).
- **Proxymock / WireMock Scenarios**: External mock dependencies must model stateful transitions (`IN_FLIGHT` -> `CLEARED`, or invoice creation -> settlement preimage receipt) using scenario state machines.
- **ISO 20022 (pacs.008) Pathways**: Test FI-to-FI Customer Credit Transfer XML generation (`pacs.008.001.08`) and stateful clearing verification via WireMock virtualized clearing networks.
- **X402 Settlement Middleware**: Test protected capability endpoints (`/api/v1/canton/cbtc/verify`, `/api/v1/dlc/bond`, `/api/v1/m2m/settle`) against virtualized Lightning / Settlement backends with header validation (`x-402-payment`, `payment-required`, `payment-signature`).
- **Chaos Fault Injection**: Test suites MUST simulate failure modes:
  - **Server Faults**: HTTP 500 Internal Server Error injection asserting gateway error propagation (e.g., HTTP 503 `lightning_backend_unavailable`).
  - **High Latency**: Network lag delay injection (e.g., 1500ms delay) asserting timeout handling (e.g., HTTP 504 `lightning_backend_timeout`).
  - **Rate Limiting**: HTTP 429 Too Many Requests response handling.
- **Model Context Protocol (MCP) Execution**: Use `scripts/mcp_test_runner.sh` for agentic test runner execution:
  - Command: `./scripts/mcp_test_runner.sh --test wiremock_simulation_tests`
  - Output: Structured JSON containing execution status, pass/fail test counts, duration, and exit code.

## CI/CD Pipelines
- **rust-ci.yml**: Format, clippy, test (incl. mock-integrations), release build.
- **lightning-coverage.yml**: Lightning scoped coverage gate (≥90%).
- **cargo-audit.yml**: Weekly dependency audit.
- **secret-scan.yml**: Gitleaks secret scanning.
- **node-ci.yml**: TypeScript build + vitest across all Node workspaces.
- **release.yml**: Tag-triggered, fail-closed GitHub Release with production Gateway archive, checksum manifest, CycloneDX 1.5 SBOM, SLSA provenance subjects, and protected publication job.
