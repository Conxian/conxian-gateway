# Conxian Gateway (The Pipe)

Institutional-grade middleware bridging Bitcoin/Stacks state logic with enterprise compliance. The gateway is designed to capture the Total Addressable Market (TAM) of Bitcoin-native liquidity while maintaining sovereign alignment.

## Purpose

Provide a unified, authenticated `/api/v1` interface for chain state, compliance verification, and institutional integrations across the Conxian stack.

## Status

Active development. Interfaces and module boundaries may evolve as protocol, wallet, and platform requirements converge.

## Audience

- Backend engineers integrating chain-state monitoring and institutional egress.
- Platform operators running the stack locally or in production environments.
- Wallet and UI engineers consuming Gateway APIs.

## Relationship to the Conxian stack

- Primary API surface for Conxian UI, Conxius Wallet, and the `conxius-platform` orchestration stack.
- Core shared logic is centralized in `lib-conxian-core/`.

## Features
- **Engine**: Nakamoto-ready indexing and state monitoring for Bitcoin and Stacks. Enhanced with the sBTC "Suction" pattern and Sovereign Yield Index (SYI) tracking.
- **API**: SLA-grade B2B interface with hardened authentication (constant-time comparison) and DoS protection. Supports ISO 20022 banking egress and identity exchange (WIF).
- **Compliance**: Zero-Knowledge Compliance (ZKC) module for Conxius Wallet attestation with cryptographic verification (ECDSA, Schnorr, ZKML, and BitVM).
- **Metrics**: Built-in Prometheus-compatible metrics endpoint with detailed chain state and treasury telemetry (Protected).
- **Persistence**: Atomic file-based persistence for reliable state monitoring across restarts.

## Architecture
- `/cmd/gateway`: Entry point and wiring.
- `/internal/engine`: Blockchain listeners and Treasury monitor.
- `/internal/api`: Institutional API, Auth middleware, and handlers.
- `/internal/compliance`: ZKC attestation verifier, BitVM verifier, and Identity Manager (WIF).
- `/pkg/conxian-core`: Shared models, error types, and persistence layer.

## API Endpoints
- `GET /api/v1/health`: Service health check.
- `GET /api/v1/metrics`: Prometheus-compatible metrics (includes uptime, treasury, and SYI) (Authorized).
- `GET /api/v1/state`: Current chain state and gateway metrics (Authorized).
- `POST /api/v1/verify`: Verify cryptographic attestations (ECDSA, Schnorr, ZKML, BitVM) (Authorized).
- `POST /api/v1/identity/exchange`: Exchange OIDC token for GCP access token (WIF) (Authorized).
- `POST /api/v1/iso20022/payment`: Generate standardized ISO 20022 egress messages (Authorized).
- `POST /api/v1/iso20022/pacs008`: Ingest ISO 20022 pacs.008 settlement signals (Authorized).
- `POST /api/v1/iso20022/pacs009`: Ingest ISO 20022 pacs.009 settlement signals (Authorized).
- `POST /api/v1/settlement/papss`: Ingest PAPSS settlement signals (Authorized).
- `POST /api/v1/settlement/brics`: Ingest BRICS settlement signals (Authorized).
- `GET /api/v1/settlements/external`: View the in-memory, normalized settlement log (Authorized).

## Configuration
The following environment variables can be used to configure the gateway:
- `BITCOIN_RPC_URL`: URL of the Bitcoin node RPC (default: https://bitcoin-rpc.publicnode.com)
- `BITCOIN_RPC_USER`: Bitcoin RPC username (optional; set both user+pass, or leave both empty)
- `BITCOIN_RPC_PASS`: Bitcoin RPC password (optional; set both user+pass, or leave both empty)
- `STACKS_RPC_URL`: URL of the Stacks node API (default: https://api.mainnet.hiro.so)
- `API_TOKEN`: Bearer token for institutional API access (default: institutional-default-token)
- `BITCOIN_SYNC_INTERVAL`: Sync interval for Bitcoin (default: 10s)
- `STACKS_SYNC_INTERVAL`: Sync interval for Stacks (default: 30s)
- `RAMP_API_KEY`: API key for Ramp Network integration.
- `INVESTEC_CLIENT_ID`: Client ID for Investec Open Banking.
- `INVESTEC_SECRET`: Client secret for Investec Open Banking.
- `ALCHEMY_PAY_APP_ID`: App ID for Alchemy Pay.
- `ALCHEMY_PAY_SECRET`: App secret for Alchemy Pay.
- `BANXA_API_KEY`: API key for Banxa.
- `BANXA_SECRET`: API secret for Banxa.
- `INFOBIP_API_KEY`: API key for Infobip A2P messaging.
- `INFOBIP_BASE_URL`: Base URL for Infobip API.
- `HMAC_SECRET`: Secret used for stateless OTP HMAC generation.
- `FIAT_WEBHOOK_SECRET`: Secret used for verifying fiat provider webhooks.
- `SETTLEMENT_INGRESS_SECRET`: Secret used for verifying settlement ingress payloads.

## Getting Started
```bash
# Run the gateway
cargo run --bin gateway

# Run tests
cargo test
```
