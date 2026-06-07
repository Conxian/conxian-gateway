# Conxian Gateway

Institutional-grade Rust middleware bridging Bitcoin and Stacks state with compliance, verification, and integration services.

[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/Status-v0.1.3-orange.svg)](#status)

## Purpose

Provide a public middleware and integration surface for indexing, verification, settlement workflows, and institutional connectivity around the Conxian ecosystem.

## Status

**Active development (v0.1.3).** Production intent exists, but deployment decisions should follow the readiness criteria documented in this repository.

### Readiness framing

- This repository contains real runtime code and release history.
- Some components and verification paths have transitioned from mock naming to explicit simulated naming to avoid overstating production enforcement.
- Production readiness should be claimed only for the specific subsystems that satisfy the documented readiness gates and environment-backed verification requirements.

## Scope

This repository contains gateway and middleware code. It is not the protocol source of truth, and it is not a custody authority for user or treasury funds.

## Governance relation

This repository is maintained by Conxian Labs as part of the public Conxian stack. It supports protocol access and integrations, while governance of the protocol is expected to decentralize progressively after mainnet.

## Intended audience

- infrastructure engineers
- institutional integrators
- fintech and settlement partners
- contributors building on Conxian services

## Architecture overview

- `/cmd/gateway`: entry point, configuration, dependency wiring
- `/internal/engine`: chain listeners, treasury monitor, and execution services
- `/internal/api`: REST API, auth middleware, and handlers
- `/internal/compliance`: verification, attestation, and policy enforcement
- `/pkg/conxian-core`: shared models and persistence primitives

## Configuration

Configuration is managed via environment variables.

```bash
cp .env.example .env
```

Use `.env.example` as the template. Do not commit real secrets.

## Development

```bash
cargo build --release
cargo run --bin gateway
cargo test --all-features
```

## Quality checks

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test
```

## Policies

- [LICENSE](LICENSE)
- [SECURITY.md](SECURITY.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [CHANGELOG.md](CHANGELOG.md)
- [CODEOWNERS](CODEOWNERS)
- [RELEASE.md](RELEASE.md)

## Contact

- Support: [support@conxian-labs.com](mailto:support@conxian-labs.com)
- Security: [security@conxian-labs.com](mailto:security@conxian-labs.com)
- General: [info@conxian-labs.com](mailto:info@conxian-labs.com)

## BOS Control Plane & Canonical Mapping (v1.9.2)

The Conxian Gateway serves as the integration surface for the BOS control plane. Canonical references, mapping, and migration paths are maintained in the following documents:

- [ADMIN_CONTRACTS_V1.md](docs/api/ADMIN_CONTRACTS_V1.md) — Admin API contracts for BOS workflows.
- [PORTFOLIO_MAP.md](docs/PORTFOLIO_MAP.md) — Shared repository and domain mapping.
- [SAB_MIGRATION.md](docs/SAB_MIGRATION.md) — Infrastructure sovereignty transition tracking.
- [READINESS_GATES.md](docs/READINESS_GATES.md) — Institutional release criteria.
- [RFC_RGB_ADAPTER.md](docs/RFC_RGB_ADAPTER.md) — Protocol adapter specification for RGB rails.

Admin API v1 (bootstrap) is now available under `/admin/v1` for release governance and audit workflows.
