# Session Summary — RGB #228 Bitcoin Core Regtest Fixture

**Date:** 2026-07-26
**Issue:** [#228](https://github.com/Conxian/conxian-gateway/issues/228)

## Delivered in this slice

- Added an opt-in, path-filtered Bitcoin Core 31.1 regtest lane under
  `tests/rgb/` and `.github/workflows/rgb-regtest-e2e.yml`.
- The harness creates a real mined genesis UTXO and a pinned RGB v0.12
  `100 -> 40 receiver + 60 change` operation, derives the MMB/MPC commitment
  before signing, embeds it through OP_RETURN, and has Bitcoin Core fund, sign,
  mempool-check, broadcast, and mine the exact witness transaction.
- A deterministic test-only BIP340 key signs the exact 32-byte
  `ArticlesId::commit_id()` callback value without rehashing. Imports use
  `Bip340IssuerPolicy`; no accept-all validator is used by this lane.
- The receiver first imports genesis, registers the receiver auth-token/seal
  binding, then imports the state-changing consignment. Assertions cover the
  receiver amount, change amount, operation ID, Bitcoin txid, vouts, and a fresh
  filesystem reload.
- Bad BIP340 signatures and Bitcoin transactions carrying the wrong commitment
  reject without state mutation, including after resolver drop/reopen.
- Bitcoin Core uses cookie authentication from the isolated ephemeral datadir.
  The cookie is read only to configure the Rust RPC client, never passed on a
  process command line, and never retained in uploaded artifacts.
- Before success/cleanup, a fail-closed guard validates that the artifact tree
  is owned, readable and contains only regular files/directories; forbids any
  `.cookie` entry; and scans each regular file for the exact loaded cookie
  secret. Relative artifact paths are checked with the same protected pattern
  source, and scanner/traversal errors are failures, not clean no-matches.
- Credential matches, unreadable entries, symlinks, FIFOs/devices/sockets and
  other unsafe objects atomically quarantine the complete run outside the
  workflow upload tree. Only a sanitized failure marker is recreated, and trap
  cleanup preserves the guard failure without printing the credential.
- `tests/rgb/test_rgb_artifact_guard.sh` adds executable regressions for a
  readable binary leak, a credential-bearing filename, a forbidden `.cookie`,
  a mode-`000` secret-bearing file, symlink and FIFO objects, an injected
  scanner error, a diagnostic-copy I/O failure, and a normal clean tree. The RGB
  regtest workflow runs this focused suite before the full Bitcoin Core proof.

## Boundaries preserved

- `RejectIssuerSignatures` remains the production/runtime default.
- No public or admin import endpoint, runtime policy loader, production private
  key, permissive validator, or mutable external indexer was added.
- The unreachable loopback Esplora constructor value is deliberate and tested:
  the receiver seal is witness-relative, so the pinned verifier checks the
  included Bitcoin witness and performs no Esplora request.
- This is reproducible test evidence, not a claim that Active-mode import is
  production-ready.

## Reproduction and verification

Run the opt-in proof with:

```bash
bash tests/rgb/test_rgb_artifact_guard.sh
bash tests/rgb/rgb_regtest_e2e.sh
```

The focused issuer-policy tests, 42 stash tests, ignored integration target,
full harness, workspace format/clippy/tests (with and without mock integrations),
Node install/build/tests, action-pin tests, contamination guard, and simulated
Gateway health probe all passed. The first Node test attempt found no installed
Playwright browser; after installing the declared Chromium runtime, the complete
Node suite passed without repository changes.
