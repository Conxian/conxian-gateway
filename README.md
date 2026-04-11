# Conxian Gateway: Institutional Compliance Pipe

Institutional-grade middleware bridging Bitcoin/Stacks state logic with enterprise compliance, featuring mathematically verifiable state proofs and ZK-compliant auditing.

## 0. Governance & BOS Role
**Business Unit**: Protocol & Institutional Infrastructure
**BOS Role**: Canonical Blockchain State Listener & Compliance Pipe
**Status**: Mainnet-Ready (Production)
**Ownership**: @botshelomokoka @admin-conxian-labs

## 1. Vision & Strategy
Conxian is designed to capture the Total Addressable Market (TAM) of Bitcoin-native liquidity ($10B+), moving beyond the initial Stacks Serviceable Addressable Market (SAM).

### Industry Enhancement Pillars
- **A. sBTC "Suction" Pattern**: Incentivize native BTC-to-sBTC migrations via the Sovereign Yield Index (SYI).
- **B. BitVM & DLC Bonds**: Trustless cross-chain state verification and non-custodial Bitcoin debt.
- **C. Institutional ISO 20022 Egress**: Banking-standard messaging (pacs.008) for legacy egress.
- **D. Workload Identity Federation (WIF)**: TEE-based agent authentication without static keys.

## 2. Architecture
- `/cmd/gateway`: Entry point and wiring.
- `/internal/engine`: Blockchain listeners, Treasury monitor, and ALEX DEX client.
- `/internal/api`: Institutional API, Auth middleware, and handlers.
- `/internal/compliance`: ZKC attestation verifier, BitVM verifier, and Identity Manager (WIF).
- `/pkg/conxian-core`: Shared models, error types, and persistence layer.

## 3. API Endpoints
- `GET /api/v1/health`: Service health check.
- `GET /api/v1/metrics`: Prometheus-compatible metrics (includes uptime, treasury, and SYI) (Authorized).
- `GET /api/v1/state`: Current chain state and gateway metrics (Authorized).
- `POST /api/v1/verify`: Verify cryptographic attestations (ECDSA, Schnorr, ZKML, BitVM) (Authorized).
- `POST /api/v1/identity/exchange`: Exchange OIDC token for GCP access token (WIF) (Authorized).
- `POST /api/v1/identity/resolve`: Resolve identity for ENS, BNS, or World ID (Authorized).
- `POST /api/v1/iso20022/payment`: Generate standardized ISO 20022 egress messages (Authorized).
- `POST /api/v1/alex/quote`: Fetch swap quote from ALEX DEX (Authorized).
- `POST /api/v1/alex/swap`: Execute ALEX swap operation (Authorized; returns `501` until signer integration exists).
- `POST /api/v1/bounties/payouts/toggle`: Maintainer control for bounty activation (Authorized).
- `POST /api/v1/ingress/iso20022`: Ingest ISO 20022 settlement signals (Authorized).
- `POST /api/v1/ingress/papss`: Ingest PAPSS settlement signals (Authorized).
- `POST /api/v1/ingress/brics`: Ingest BRICS settlement signals (Authorized).

## 4. Configuration
The following environment variables can be used to configure the gateway:
- `BITCOIN_RPC_URL`: URL of the Bitcoin node RPC.
- `STACKS_RPC_URL`: URL of the Stacks node API.
- `API_TOKEN`: Bearer token for institutional API access.
- `FIAT_WEBHOOK_SECRET`: Secret for verifying fiat provider webhooks.
- `SETTLEMENT_INGRESS_SECRET`: Secret for verifying settlement ingress payloads.
- `ALEX_API_URL`: Base URL for ALEX API integration (default: https://api.alexlab.co).

## 5. Development
```bash
# Run the gateway
cargo run --bin gateway

# Run tests
cargo test --all-features
```
