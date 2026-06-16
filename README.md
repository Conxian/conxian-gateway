# Conxian Gateway

Integration and middleware surface for the broader Conxian ecosystem.

[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/Status-v0.1.4-orange.svg)](#status)

## Purpose

Provide a public middleware and integration surface for indexing, verification, settlement workflows, and external connectivity around Conxian.

## Status

**Active development (v0.1.4).** Production intent exists, but deployment decisions should follow the readiness criteria documented in this repository.

### Readiness framing

- This repository contains real runtime code and release history.
- Some components and verification paths have transitioned from mock naming to explicit simulated naming to avoid overstating production enforcement.
- Production readiness should be claimed only for the specific subsystems that satisfy the documented readiness gates and environment-backed verification requirements.

## Scope

This repository contains gateway and middleware code. It is a support and integration layer. It is not the protocol source of truth, and it is not the governance or ownership center of the ecosystem.

## Governance relation

This repository is maintained by Conxian-Labs as a public support and integration surface around Conxian. It helps systems interact with the protocol without replacing the protocol's DAO-facing authority.

## Intended audience

- infrastructure engineers
- integrators
- fintech and settlement partners
- contributors building on Conxian services

## Architecture overview

- `/apps/control-plane`: Next.js management dashboard for the gateway.
- `/cmd/gateway`: entry point, configuration, dependency wiring.
- `/internal/engine`: chain listeners, treasury monitor, and execution services.
- `/internal/api`: REST API, auth middleware, and handlers.
- `/internal/compliance`: verification, attestation, and policy enforcement.
- `/packages/client-sdk`: TypeScript SDK for Conxian integrations.
- `/packages/schemas`: Shared JSON-LD schemas and type definitions.
- `/pkg/conxian-core`: shared Rust models and persistence primitives.

## Configuration

Configuration is managed via environment variables.

```bash
cp .env.example .env
```

Use `.env.example` as the template. Do not commit real secrets.

## Development

### Rust (Gateway)
```bash
cargo build --release
cargo run --bin gateway
cargo test --workspace
```

### TypeScript (Control-Plane & SDK)
```bash
pnpm install
pnpm build
pnpm --filter control-plane dev
```

## Quality checks

### Rust
```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
```

### TypeScript
```bash
pnpm lint
pnpm test
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
