# Session Summary — RGB #228 Phase 1.5 Hardening

**Date:** 2026-07-20  
**Issue:** [#228](https://github.com/Conxian/conxian-gateway/issues/228)  
**Prior delivery:** `124d17e8c1cc02dce5bef7be3d8cab28630ee38b`

## Expected vs. actual

The prior Phase 1 delivery introduced the feature-gated RGB stash path and
declared the locked `rgb-std`/`bp-esplora` dependencies. Review found that the
runtime path still used ad hoc `rgb:` validation, swallowed stash persistence
errors, collapsed Esplora failures into “unspent,” allowed Active-mode
simulation fallback, and synthesized an RGB contract ID from a Bitcoin txid.
Gateway configuration also did not expose or wire the native stash settings.

## Phase 1.5 repair scope

- Parse canonical `contract:` Baid64 IDs with `rgb::ContractId` and reject
  legacy/mutated IDs.
- Return typed RGB errors for stash initialization, corruption, persistence,
  configuration, and Esplora failures; persist metadata atomically.
- Use the locked blocking Esplora client off Tokio worker threads and preserve
  spent/unspent/not-found/error distinctions.
- Make Active fail closed and keep simulation fallback Shadow-only, with
  injectable node clients for deterministic tests.
- Add `RGB_STASH_PATH`/`RGB_ESPLORA_URL`, native feature forwarding, startup
  wiring, and accurate documentation.
- Remove the invalid mempool `rgb:{bitcoin_txid}` lookup boundary.

## Remaining blockers

This repair does **not** claim full issue completion. Phase 2 still requires:

- `rgb-persist-fs::StockpileDir` integration.
- Consignment import/export and receiver `AuthToken` → seal-definition
  registration.
- Signature policy and full `ContractVerify` consensus verification.
- A deterministic Bitcoin/RGB regtest fixture and end-to-end harness.

No fake signature verification or no-op consensus proof was added.
