# Conxian Gateway Developer Sandbox

This is the smallest workspace-native developer path for the current Conxian Gateway SDK. It is a **developer preview and rehearsal-validation path**, not a cryptographic proof, finality, or production settlement demo.

## What it runs

The TypeScript entry point uses `@conxian/client-sdk` and `ConxianClient` to make this sequence:

1. `GET /api/v1/health` — liveness only; a healthy response is exactly `{"status":"ok"}` and is **not** chain-sync or settlement readiness. Use `/api/v1/state` and `/metrics` for operational detail.
2. `GET /api/v1/chains/list` — the supported-chain list from the Gateway's runtime registry.
3. `POST /api/v1/chains/babylon/verify` with `{ "type": "finality_gadget", "evidence": "sandbox-rehearsal" }` — Babylon rehearsal validation.

Without `BABYLON_API_URL`, the Babylon adapter accepts this rehearsal-mode proof-type shape. This is not authoritative, cryptographic, EOTS, or full finality verification. With a configured Babylon header source, the adapter instead requires a positive `btc_height` no more than six blocks behind its verified header-source tip; the sandbox's minimal rehearsal payload does not satisfy that configured-source path. The bounded height check still does not perform EOTS or full finality proof validation. The generic BitVM verification route is intentionally unavailable and returns typed HTTP `501 Not Implemented` rather than a successful proof result.

## Prerequisites

- A checkout of this repository.
- A running, operator-provided Gateway instance reachable from the sandbox. The default is `http://localhost:3000`.
- `CONXIAN_GATEWAY_URL` pointing to that instance and a valid `CONXIAN_API_TOKEN` issued for it. No hosted/free public sandbox URL or token is currently provided.
- Node.js and pnpm compatible with the repository's workspace setup.

## Run from the repository root

```bash
pnpm install --frozen-lockfile --ignore-scripts
pnpm --filter @conxian/client-sdk build
pnpm --filter @conxian/developer-sandbox build

export CONXIAN_GATEWAY_URL='http://localhost:3000'
export CONXIAN_API_TOKEN='your-valid-operator-provided-token'

pnpm --filter @conxian/developer-sandbox start
```

The sandbox depends on the private, workspace-only `@conxian/client-sdk` package through `workspace:*`. The SDK is not published or installable from npm; build and use it from this monorepo. Do not commit real tokens or put them in source files.

Expected output is named `gateway`, `health`, `supported chains`, and `Babylon rehearsal validation`. This is a free/local workspace evaluation flow only. Payment, billing, settlement, monetization, and enterprise entitlements are out of scope and may be future or operator-specific concerns; no free hosted service is implied.

## Test the path

```bash
pnpm --filter @conxian/client-sdk test
pnpm --filter @conxian/developer-sandbox test
```

The sandbox test mocks HTTP while using the real SDK class. It proves request order, route construction, bearer authentication, and the exact Babylon rehearsal payload, including a regression assertion that the BitVM verify route is not used as the success path. It does **not** prove that a live Gateway accepted the request or that any cryptographic/finality proof was verified.

A live run requires the operator-provided `CONXIAN_GATEWAY_URL` and valid `CONXIAN_API_TOKEN` above. Its result reflects that Gateway's actual configuration, including the Babylon source-dependent behavior described earlier.
