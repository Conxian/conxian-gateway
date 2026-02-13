# Conxian Gateway (The Pipe)

Institutional-grade middleware bridging Bitcoin/Stacks state logic with enterprise compliance.

## Features
- **Engine**: Nakamoto-ready indexing and state monitoring for Bitcoin and Stacks.
- **API**: SLA-grade B2B interface with secure authentication.
- **Compliance**: Zero-Knowledge Compliance (ZKC) module for Conxius Wallet attestation.
- **Audit-Ready**: Clean codebase optimized for security audits.

## Architecture
- `/cmd/gateway`: Entry point.
- `/internal/engine`: State listeners and block ingestion.
- `/internal/api`: Institutional API and Auth.
- `/internal/compliance`: ZKC attestation verifier.
- `/pkg/conxian-core`: Shared libraries.

## Getting Started
```bash
cargo run --bin gateway
```
