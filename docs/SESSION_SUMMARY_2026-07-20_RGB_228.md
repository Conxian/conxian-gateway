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

## Phase 2 implementation checkpoint — 2026-07-20

The approved Phase 2 branch adds the strongest safe boundary available from
the pinned `rgb-std` v0.12.0-rc.3 ecosystem:

- `rgb-persist-fs = 0.12.0-rc.3` is exact-pinned and owns a
  `StockpileDir<bp::seals::TxoSeal>` under `RGB_STASH_PATH`.
- A wallet-owned, atomic `AuthToken -> WTxoSeal` registry validates the seal's
  committed auth token, treats identical replay as idempotent, rejects
  overwrite, and fails closed on corrupt persistence.
- Consignment import preflights the binary envelope and contract ID, rejects
  unsigned articles, invokes an application-provided issuer signature
  validator, and delegates operation/codex/witness verification to
  `rgb::Contracts::consume_from_file`. Export is stockpile-backed and carries
  only RGB auth-token terminals.
- Active adapter verification no longer falls through to HTTP or simulation;
  the JSON metadata cache is explicitly descriptive and never proof.
- Deterministic tests cover malformed consignments, contract-ID mismatch,
  invalid/fail-closed signature policy, unknown auth tokens, replay/overwrite,
  and corrupted metadata/registry persistence.

### Honest limitations

The pinned RGB API abstracts issuer cryptography, so no production signature
algorithm is claimed. The repository also does not yet include a complete
signed RGB/Bitcoin regtest fixture harness. Active consignment rollout remains
fail-closed until those fixtures and an approved signature backend exist.

## PR #261 persistence hardening — 2026-07-20

Review of the Phase 2 import path confirmed that pinned
`rgb-persist-fs` creates a contract directory and pile files before
`evaluate_commit` returns. The branch now evaluates unknown-contract imports
in a private same-filesystem staging directory, syncs the staged files, and
promotes the one expected `*.contract` directory with an atomic rename only
after validation succeeds. Import errors, including importer panics, remove
that import's staging tree; cleanup errors are surfaced distinctly. Existing
contract updates are rejected rather than mutating a valid stockpile in place
until a transactional update path exists.

The local stash directory is documented as a filesystem trust boundary rather
than an encryption boundary. Unix-owned directories are mode `0700`; registry
temporary/final files are mode `0600`; file data is synced before replacement
and the parent directory is synced after atomic rename. Existing registry
files are tightened on startup as well.

Regression coverage now uses the pinned RGB APIs and a structurally decodable,
issuer-accepted but semantically invalid consignment. It proves failed import
leaves no contract or staging directory and that a fresh resolver can reopen
the stash. A separate test proves pinned `allow_unknown = true` first-contract
import does not call the seal resolver; wallet-owned seal/Esplora validation is
only asserted on paths where the pinned API invokes that callback.
