# Conxian Gateway (The Pipe)

Institutional-grade middleware bridging Bitcoin/Stacks state logic with enterprise compliance.

## Features
- **Engine**: Nakamoto-ready indexing and state monitoring for Bitcoin and Stacks.
- **API**: SLA-grade B2B interface with secure authentication (Bearer token).
- **Compliance**: Zero-Knowledge Compliance (ZKC) module for Conxius Wallet attestation with cryptographic verification (ECDSA & Schnorr).
- **Metrics**: Built-in Prometheus-compatible metrics endpoint.
- **Audit-Ready**: Clean codebase optimized for security audits.
- **Testable**: Decoupled RPC layers with traits for mocking and unit testing.
- **Robust**: Graceful shutdown and improved state monitoring.

## Architecture
- `/cmd/gateway`: Entry point and configuration.
- `/internal/engine`: State listeners (Bitcoin & Stacks) and block ingestion.
- `/internal/api`: Institutional API, Auth middleware, and handlers.
- `/internal/compliance`: ZKC attestation verifier with secp256k1 support (ECDSA & Schnorr).
- `/pkg/conxian-core`: Shared libraries and common types.

## API Endpoints
- `GET /api/v1/health`: Service health check.
- `GET /api/v1/metrics`: Prometheus-compatible metrics.
- `GET /api/v1/state`: Current chain state (Authorized).
- `POST /api/v1/verify`: Verify cryptographic attestations (Authorized).

## Configuration
The following environment variables can be used to configure the gateway:
- `BITCOIN_RPC_URL`: URL of the Bitcoin node RPC (default: http://localhost:18332)
- `BITCOIN_RPC_USER`: Bitcoin RPC username
- `BITCOIN_RPC_PASS`: Bitcoin RPC password
- `STACKS_RPC_URL`: URL of the Stacks node API (default: https://api.mainnet.hiro.so)
- `API_PORT`: Port for the Gateway API (default: 3000)
- `API_TOKEN`: Bearer token for institutional API access (default: institutional-default-token)

## Getting Started
```bash
# Run the gateway
cargo run --bin gateway

# Run tests
cargo test
```
