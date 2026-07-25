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
  transactions or retains the verified new contract after a committed
  promotion. Unprovable states fail closed.
- Candidate data and relevant directories are synced before promotion.
  `StockpileDir` is reloaded only after successful promotion or completed
  recovery cleanup.
- Focused tests cover successful update/restart, invalid and signature-rejected
  updates, prepared/backed-up/promoted interruption recovery, residue cleanup,
  fail-closed corrupt state, and byte-preservation of unrelated root files.

## Security boundaries preserved

- `RejectIssuerSignatures` remains the default fail-closed policy.
- Exact RGB v0.12 RC dependency pins are unchanged.
- No HTTP or simulation proof fallback was added.
- The JSON metadata cache remains descriptive and non-authoritative.
- Unknown-contract staging remains separate and unchanged.

## Still open

- A production issuer-signature verification backend.
- A complete signed Bitcoin/RGB regtest fixture and end-to-end harness. The
  pinned fixture can exercise a consensus-accepted existing-contract replay,
  but it does not replace the required real signed transition/regtest proof.
