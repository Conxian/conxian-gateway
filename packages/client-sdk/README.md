# Conxian Client SDK

TypeScript helpers for calling the Conxian Gateway API from this monorepo.

## Purpose
Provides a shared interface for applications and internal services to call current Gateway routes. A route being listed or callable does not mean its result is authoritative verification or production-ready settlement evidence.

## Status
**Developer Preview (v0.1.4).** This package has `private: true`, is consumed through `workspace:*`, and is not published or installable from npm. The version identifies the current workspace package; it is not an npm release claim.

## Audience
- **Developers**: Integrating Conxian services into wallets or dapps.
- **Partners**: Building institutional adapters for the gateway.

## Features
- **Verification route client**: `verifyStateProof` sends metadata to a chain-specific Gateway route; availability and verification strength depend on the server adapter.
- **Chain Metadata**: Easy access to latest heights and identities across supported chains.
- **Transaction Preparation**: Builds unsigned payloads ready for local-first signing.

## Verification boundaries

- `GET /api/v1/chains/list` reports registered routes; it does not certify that each route provides authoritative verification.
- Generic `POST /api/v1/chains/bitvm/verify` is intentionally unavailable and returns typed HTTP `501 Not Implemented` with `code: "verifier_unavailable"` and `authoritative: false`.
- Without `BABYLON_API_URL`, Babylon verification is rehearsal-mode shape validation for `type: "finality_gadget"` only.
- With a configured Babylon header source, Babylon requires a positive `btc_height` within six blocks of the verified source tip. This bounded check is not EOTS, cryptographic finality, or full finality-proof validation.

## Usage

From the repository root:

```bash
pnpm install --frozen-lockfile --ignore-scripts
pnpm --filter @conxian/client-sdk build
```

Then import the workspace package from another monorepo workspace:

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

For the short health → supported chains → Babylon rehearsal-validation path, see the [developer sandbox](../../examples/developer-sandbox/README.md). A live run requires an operator-provided Gateway URL and valid token; mocked tests are contract checks, not live proof evidence. No hosted/free public sandbox URL or token is currently provided.
