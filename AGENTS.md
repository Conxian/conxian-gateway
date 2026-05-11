# Conxian Gateway: Agent Instructions

You are working on the **Conxian Gateway**, an institutional-grade Rust middleware designed for high-performance Bitcoin/Stacks state logic and enterprise compliance.

## Core Philosophy
- **Sovereignty**: All code must prioritize non-custodial logic and user sovereignty.
- **Institutional Grade**: Maintain SLA-grade interfaces, high-performance async Rust, and robust error handling.
- **Compliance Pipe**: The gateway is a pass-through for compliance data (ZKC), not a storage for PII.

## Technical Standards
- **Rust Edition**: 2021
- **Framework**: Axum (HTTP), Tokio (Runtime)
- **Security**: Mandatory Bearer token auth for sensitive endpoints.
- **Observability**: Prometheus metrics and structured tracing are required for all new modules.
- **Persistence**: Any stateful component must use the atomic persistence layer.

## Verification Protocol
Before submitting changes, you MUST:
1. Run `cargo clippy --all-targets --all-features -- -D warnings`
2. Run `cargo fmt --all -- --check`
3. Run all tests: `cargo test --workspace`
4. Run the contamination guard: `python3 scripts/verify_contamination_guard.py`
5. Verify health check: `GET /api/v1/health` returns `healthy`.

## Module Map
- `/cmd/gateway`: Entry point, configuration, and wiring.
- `/internal/engine`: Blockchain listeners and RPC clients.
- `/internal/api`: REST interface, handlers, and auth middleware.
- `/internal/compliance`: ZKC (Zero-Knowledge Compliance) and MVCR logic.
- `/pkg/conxian-core`: Shared models, error types, and persistence logic.

## Alignment Principles
- **Risk Transparency**: Always ensure that new layer integrations or updates include metadata fields for Data Availability, Settlement, and Bridge Security.
- **Source of Truth**: Refer to `bitcoinlayers.org` for the most up-to-date research on Bitcoin Layer 2 and sidechain trust models.
- **Key Documents**:
  - **PRD**: `docs/PRD.md`
  - **Enhancements**: `docs/ENHANCEMENTS.md`
  - **Portfolio Map**: `docs/PORTFOLIO_MAP.md`
  - **Readiness Gates**: `docs/governance/READINESS_GATES.md`
  - **SAB Migration**: `docs/governance/SAB_MIGRATION.md`

## Workflow Instructions
- **State Monitoring**: Point to the Conxian Gateway API at `/api/v1` for state monitoring and compliance pipes.
- **Service Access**: All sovereign services and Bitcoin layers (Bisq, RGB, BitVM, Changelly, Stacks, Lightning, Liquid, Rootstock) are unified under the Gateway.
- **Infrastructure**: Infrastructure configurations are located in `infrastructure/`.

## Ethical Alignment
The Conxian Protocol is built to empower individuals and institutions within the Stacks/Bitcoin ecosystem. Avoid any "dark patterns" or custodial shortcuts.
