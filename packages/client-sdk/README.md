# Conxian Client SDK

Institutional-grade TypeScript helpers for bridging Bitcoin and Stacks state logic.

## Purpose
Provides a standardized interface for external applications and internal services to interact with the Conxian Gateway. It abstracts the complexity of heterogeneous proof verification and multi-chain state lookups.

## Status
**Developer Preview (v0.1.4).** Supports UCV-1 (Universal Chain Verification) and prepared transaction payloads for Tier 1 chain families.

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

const client = new ConxianClient('https://gateway.conxian-labs.com', 'your-api-token');
const chains = await client.getSupportedChains();
```
