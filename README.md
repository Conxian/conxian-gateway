# Conxian Gateway

Institutional-grade middleware bridging Bitcoin and Stacks state logic with enterprise compliance.

[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/Status-v0.1.4-orange.svg)](#status)

## Purpose
The Conxian Gateway provides a single API layer for indexing, verifying, and orchestrating settlement workflows across multiple Bitcoin layers. It simplifies the integration path for institutions and fintechs by providing mathematically verifiable state proofs and non-custodial signing interfaces.

## Status
**Active Development (v0.1.4).** This repository contains production-ready runtime code for Bitcoin and Stacks state tracking. Current focus is on expanding pilot lanes for Liquid and Rootstock adapters and implementing UCV-1 (Universal Chain Verification).

## Audience
- **Institutions & Fintechs**: Seeking a secure, non-custodial entry point into Bitcoin-native liquidity.
- **Infrastructure Engineers**: Deploying and operating Conxian nodes in sovereign or enterprise environments.
- **Integrators**: Building wallets, dapps, or settlement systems on the Conxian stack.

## Workflow & Consumption
1. **Discovery**: Review the [PRD.md](PRD.md) and [ADAPTER_FAMILY_STRATEGY.md](docs/research/ADAPTER_FAMILY_STRATEGY.md).
2. **Pilot**: Integrate via the [`@conxian/client-sdk`](packages/client-sdk) in a sandbox environment.
3. **Production**: Deploy the gateway behind your own institutional security perimeter using the provided [Docker Compose](docker-compose.yml) baseline.
4. **Expansion**: Add custom chain adapters or compliance rules using the [`ChainAdapter`](pkg/conxian-core/src/lib.rs) trait.

## Core Capabilities
- **Universal Verification (UCV-1)**: Unified interface for heterogeneous proofs (BitVM, ZKC, TEE).
- **Institutional Egress**: ISO 20022 (pacs.008) banking-standard messaging.
- **Mempool Orchestration**: Automated RBF/CPFP fee-bumping for high-priority settlements.
- **Identity Bridge**: Unified resolution for BNS, ENS, and World ID.

## Architecture
- `/apps/control-plane`: Next.js management dashboard.
- `/internal/engine`: High-performance chain listeners and RPC clients.
- `/internal/api`: Axum-based REST interface and X402 payment middleware.
- `/internal/compliance`: Zero-Knowledge Compliance (ZKC) and attestation logic.
- `/pkg/conxian-core`: Shared Rust primitives and resilience models.

## Development
```bash
# Rust Gateway
cargo build --release
cargo test --workspace

# TypeScript SDK & Dashboard
pnpm install
pnpm build
```

## Policies
- [LICENSE](LICENSE)
- [SECURITY.md](SECURITY.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [RELEASE.md](RELEASE.md)

## Contact
- Support: [support@conxian-labs.com](mailto:support@conxian-labs.com)
- Security: [security@conxian-labs.com](mailto:security@conxian-labs.com)
