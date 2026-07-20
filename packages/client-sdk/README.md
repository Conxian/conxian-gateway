# Conxian Client SDK

Institutional-grade TypeScript helpers for bridging Bitcoin and Stacks state logic.

## Purpose
Provides a standardized interface for external applications and internal services to interact with the Conxian Gateway. It abstracts the complexity of heterogeneous proof verification and multi-chain state lookups.

## Status
**Developer Preview (v0.1.4).** This package is private and workspace-only; it is not currently published to npm. It supports the current Gateway client surface, including UCV-1 (Universal Chain Verification) and prepared transaction payloads for Tier 1 chain families.

## Audience
- **Developers**: Integrating Conxian services into wallets or dapps.
- **Partners**: Building institutional adapters for the gateway.

## Features
- **Universal Verification**: Unified `verifyStateProof` for Bitcoin, Stacks, Liquid, and Rootstock.
- **Chain Metadata**: Easy access to latest heights and identities across supported chains.
- **Transaction Preparation**: Builds unsigned payloads ready for local-first signing.

## Usage
```typescript
import { ConxianClient } from '@conxian/client-sdk';

const apiToken = process.env.CONXIAN_API_TOKEN;
if (!apiToken) throw new Error('CONXIAN_API_TOKEN is required');

const client = new ConxianClient(
  process.env.CONXIAN_GATEWAY_URL ?? 'http://localhost:3000',
  apiToken,
);
const chains = await client.getSupportedChains();
```

For a short, runnable proof-first path, see the [developer sandbox](../../examples/developer-sandbox/README.md). It uses this workspace package directly and requires a token for a Gateway instance you control or have been given access to; no hosted URL or free token is implied.
