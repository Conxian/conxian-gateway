# Conxian Gateway Developer Sandbox

This is the smallest workspace-native developer path for the current Conxian Gateway SDK. It is a **developer preview and rehearsal path**, not a production settlement demo.

## What it runs

The TypeScript entry point uses `@conxian/client-sdk` and `ConxianClient` to make this sequence:

1. `GET /api/v1/health` — liveness and sync visibility only; a healthy response is **not** settlement readiness.
2. `GET /api/v1/chains/list` — the supported-chain list from the Gateway's runtime registry.
3. `POST /api/v1/chains/bitvm/verify` with `{ "root_hash": "0xabc123" }` — BitVM adapter input/rehearsal validation.

The BitVM result is not a cryptographic proof and does not demonstrate production settlement. The example intentionally does not exercise payment, billing, x402 settlement, tokens, DAO, DeFi, Orbit, or enterprise capabilities.

## Prerequisites

- A checkout of this repository.
- A running Gateway instance reachable from the sandbox. The default is `http://localhost:3000`.
- A valid `CONXIAN_API_TOKEN` for that Gateway. The repository does not publish a free hosted token or hosted sandbox URL.
- Node.js and pnpm compatible with the repository's workspace setup.

## Run from the repository root

```bash
pnpm install
pnpm --filter @conxian/client-sdk build
pnpm --filter @conxian/developer-sandbox build

export CONXIAN_API_TOKEN='your-token-for-the-gateway-you-control-or-use'
export CONXIAN_GATEWAY_URL='http://localhost:3000' # optional

pnpm --filter @conxian/developer-sandbox start
```

The package depends on `@conxian/client-sdk` through `workspace:*`; it is not installed from npm. Do not commit real tokens or put them in source files.

Expected output is named `gateway`, `health`, `supported chains`, and `bitvm proof rehearsal` results. The local path itself has no payment step; that does not imply a free hosted service. Future paid or enterprise flows are separate from this sandbox and are not represented here.

## Test the path

```bash
pnpm --filter @conxian/client-sdk test
pnpm --filter @conxian/developer-sandbox test
```

The sandbox test mocks HTTP while using the real SDK class, so it checks request order, URLs, bearer authentication, and the BitVM payload without requiring a running Gateway.
