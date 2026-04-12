# Conxian Gateway: Institutional Compliance Pipe

Institutional-grade middleware bridging Bitcoin/Stacks state logic with enterprise compliance, featuring mathematically verifiable state proofs and ZK-compliant auditing.

[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/Status-v0.1.0-orange.svg)](#project-status)

## 0. Governance & BOS Role
**Business Unit**: Protocol & Institutional Infrastructure
**BOS Role**: Canonical Blockchain State Listener & Compliance Pipe
**Status**: Mainnet-Ready (Production)
**Ownership**: @botshelomokoka @admin-conxian-labs

## 1. Overview & Vision
Conxian Gateway is a high-performance Rust middleware designed to bridge Bitcoin/Stacks state logic with enterprise compliance. It succeeds `Anya-core` and `OPSource`, consolidating their core functionalities into a singular, audit-ready binary.

Conxian is designed to capture the Total Addressable Market (TAM) of Bitcoin-native liquidity ($10B+), moving beyond the initial Stacks Serviceable Addressable Market (SAM).

### Intended Audience
- **Institutional Developers**: Building high-integrity Bitcoin/Stacks infrastructure.
- **Fintech Integrators**: Normalizing traditional finance (ISO 20022) with blockchain state.
- **Sovereign Node Operators**: Running non-custodial gateway infrastructure.

### Industry Enhancement Pillars
- **A. sBTC "Suction" Pattern**: Incentivize native BTC-to-sBTC migrations via the Sovereign Yield Index (SYI).
- **B. BitVM & DLC Bonds**: Trustless cross-chain state verification and non-custodial Bitcoin debt.
- **C. Institutional ISO 20022 Egress**: Banking-standard messaging (pacs.008) for legacy egress.
- **D. Workload Identity Federation (WIF)**: TEE-based agent authentication without static keys.

## 2. Project Status
Current Version: **v0.1.0**

This project is in active development. While it implements production-grade security and compliance features, users should consult the [Readiness Gates](docs/READINESS_GATES.md) before mainnet deployment.

## 3. Architecture & Modules
- `/cmd/gateway`: Entry point, configuration, and dependency injection wiring.
- `/internal/engine`: Blockchain listeners (Bitcoin/Stacks), Treasury monitor, and ALEX DEX client.
- `/internal/api`: Institutional REST API, Auth middleware, and Axum handlers.
- `/internal/compliance`: ZKC (Zero-Knowledge Compliance) attestation, BitVM verifier, and Identity Manager (WIF).
- `/pkg/conxian-core`: Shared models, CJCS v2.0 schema, and atomic persistence layer.

## 4. API Endpoints
The gateway exposes an institutional REST API at `/api/v1`. Most endpoints require Bearer token authentication.

- `GET /api/v1/health`: Service health check.
- `GET /api/v1/metrics`: Prometheus-compatible metrics (Authorized).
- `POST /api/v1/verify`: Verify cryptographic attestations (ECDSA, Schnorr, ZKML, BitVM) (Authorized).
- `POST /api/v1/identity/resolve`: Resolve identity for ENS, BNS, World ID, or Web3.bio (Authorized).
- `POST /api/v1/iso20022/payment`: Generate standardized ISO 20022 egress messages (Authorized).
- `POST /api/v1/fiat/session`: Initiate fiat on-ramp sessions (Ramp, Investec, etc.) (Authorized).
- `POST /api/v1/otp/send`: Send stateless OTP via Infobip (Authorized).
- `GET /api/v1/alex/quote`: Fetch swap quote from ALEX DEX (Authorized).
- `POST /api/v1/alex/swap`: Execute ALEX swap operation (Authorized; returns `501` until signer integration exists).
- `POST /api/v1/bounties/payouts/toggle`: Maintainer control for bounty activation (Authorized).
- `POST /api/v1/ingress/iso20022`: Ingest ISO 20022 settlement signals (Authorized).
- `POST /api/v1/ingress/papss`: Ingest PAPSS settlement signals (Authorized).
- `POST /api/v1/ingress/brics`: Ingest BRICS settlement signals (Authorized).

Full endpoint documentation can be found by inspecting the routes in `internal/api/src/routes.rs`.

## 5. Configuration
Configuration is managed via environment variables. Copy the template to get started:

```bash
cp .env.example .env
```

### Key Variables
- `API_TOKEN`: Mandatory Bearer token for institutional access.
- `BITCOIN_RPC_URL`: URL for the Bitcoin node (e.g., `https://bitcoin-rpc.publicnode.com`).
- `STACKS_RPC_URL`: URL for the Stacks API (e.g., `https://api.mainnet.hiro.so`).
- `FIAT_WEBHOOK_SECRET`: HMAC secret for verifying fiat provider webhooks.

Refer to [.env.example](.env.example) for the full list of configuration options.

## 6. Development & Testing

### Prerequisites
- [Rust](https://www.rust-lang.org/) (latest stable)
- [Cargo](https://doc.rust-lang.org/cargo/)

### Quick Start
```bash
# Build the gateway
cargo build --release

# Run the gateway
cargo run --bin gateway

# Run all tests
cargo test --all-features
```

### Quality Assurance
Before submitting a pull request, ensure your changes pass the following checks:
```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test
```

## 7. Governance & Policies
- **[LICENSE](LICENSE)**: Licensed under the MIT License.
- **[SECURITY.md](SECURITY.md)**: Security policy and vulnerability reporting.
- **[CONTRIBUTING.md](CONTRIBUTING.md)**: Contribution guidelines and coding standards.
- **[CHANGELOG.md](CHANGELOG.md)**: Tracking development progress and releases.
- **[CODEOWNERS](CODEOWNERS)**: Repository ownership and review assignments.

## 8. Support & Contact
- **Institutional Support**: [support@conxian.io](mailto:support@conxian.io)
- **Security Reports**: [security@conxian.io](mailto:security@conxian.io)
