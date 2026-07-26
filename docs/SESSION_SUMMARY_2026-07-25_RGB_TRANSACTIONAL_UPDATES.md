# Session Summary — RGB #228 Transactional Existing-Contract Updates

**Date:** 2026-07-25

**Issue:** [#228](https://github.com/Conxian/conxian-gateway/issues/228)

## Delivered in this slice

- Existing-contract consignments no longer mutate the live
  `rgb-persist-fs::StockpileDir` in place or fail solely because the contract is
  already known.
- The target contract is copied into isolated same-filesystem state and passed
  through the unchanged issuer-signature preflight and
  `rgb::Contracts::consume_from_file` consensus path.
- A durable phase journal plus old-contract backup makes promotion recoverable.
  Startup deterministically restores the verified old contract for incomplete
  pre-commit transactions. A durably persisted `promoted` journal is the
  irreversible commit point: recovery retains the verified new contract, and a
  missing committed live contract fails closed rather than rolling back.
- Candidate data and relevant directories are synced before promotion.
  Post-commit cleanup errors never enter pre-commit rollback. The live
  `StockpileDir` is reloaded before returning a committed-but-cleanup-incomplete
  or cleanup-durability-uncertain error, including the case where transaction
  directory deletion succeeded but the final stockpile-root sync failed.
- Recovery derives the only accepted transaction-directory basename from the
  validated journal contract ID. Prefixed files, mismatched transaction names,
  corrupt/unsupported journals, unsafe or mismatched contract directories, and
  symlinked transaction/staged/backup/contract paths fail closed before
  mutation.
- Focused tests cover successful update/restart, invalid and signature-rejected
  updates, prepared/backed-up/promoted interruption recovery, residue cleanup,
  fail-closed corrupt state, byte-preservation of unrelated root files, exact
  post-commit cleanup fault sequences, resolver reload, restart safety, and
  recovery identity/path rejection.

## Security boundaries preserved

- `RejectIssuerSignatures` remains the default fail-closed policy.
- Exact RGB v0.12 RC dependency pins are unchanged.
- No HTTP or simulation proof fallback was added.
- The JSON metadata cache remains descriptive and non-authoritative.
- Unknown-contract staging remains separate and unchanged.

## Still open

- A production issuer-signature verification backend.
- A complete signed Bitcoin/RGB regtest fixture and end-to-end harness. The
  current generated fixture and existing-contract replay exercise the pinned
  consignment API plus filesystem state machine; they do not perform a real
  state-changing signed RGB transition and do not replace the required signed
  Bitcoin/RGB transition/regtest proof.
