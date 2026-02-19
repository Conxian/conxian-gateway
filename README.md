# Conxian Gateway (The Pipe)

Institutional-grade middleware bridging Bitcoin/Stacks state logic with enterprise compliance.

## Features
- **Engine**: Nakamoto-ready indexing and state monitoring for Bitcoin and Stacks.
- **API**: SLA-grade B2B interface with secure authentication (Bearer token).
- **Compliance**: Zero-Knowledge Compliance (ZKC) module for Conxius Wallet attestation.
- **Audit-Ready**: Clean codebase optimized for security audits.
- **Testable**: Decoupled RPC layers with traits for mocking and unit testing.

## Architecture
- `/cmd/gateway`: Entry point and configuration.
- `/internal/engine`: State listeners (Bitcoin & Stacks) and block ingestion.
- `/internal/api`: Institutional API, Auth middleware, and handlers.
- `/internal/compliance`: ZKC attestation verifier.
- `/pkg/conxian-core`: Shared libraries and common types.

## Configuration
The following environment variables can be used to configure the gateway:
- `BITCOIN_RPC_URL`: URL of the Bitcoin node RPC (default: http://localhost:18332)
- `BITCOIN_RPC_USER`: Bitcoin RPC username
- `BITCOIN_RPC_PASS`: Bitcoin RPC password
- `API_PORT`: Port for the Gateway API (default: 3000)
- `API_TOKEN`: Bearer token for institutional API access (default: institutional-default-token)

## Getting Started
```bash
# Run the gateway
cargo run --bin gateway

# Run tests
cargo test
```
