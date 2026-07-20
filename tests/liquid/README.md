# Liquid peg-in/peg-out harness

This directory contains the opt-in Elements integration harness for issue
`#193` and the daemon/CI requirements tracked by `#218`.

## Local run

From the repository root, run:

```bash
bash tests/liquid/liquid_peg_e2e.sh
```

The harness downloads or reuses architecture-matched official archives for
Bitcoin Core `31.1` and Elements Core `23.3.3`, verifies the pinned SHA256
before extraction, and installs them below `target/liquid-daemons/`.  A cache
can be supplied with `LIQUID_DAEMON_CACHE_DIR`, but both the cache and install
override must resolve to an owned subdirectory inside this repository's
`target/` directory.  `/`, `$HOME`, the repository root, `target/` itself,
symlinked paths, and non-owned directories are rejected.  The default cache
is `target/liquid-daemon-cache`; the default install directory is
`target/liquid-daemons`.

An existing non-default install override is recursively replaced only when it
contains the harness ownership marker
`.conxian-liquid-daemon-install-owner`.  A new safe override receives that
marker before installation.  The canonical default install directory is
harness-owned by location.  These checks prevent a typo or an arbitrary
environment override from causing recursive deletion outside the harness
boundary.

The RPC/P2P ports can be overridden with `LIQUID_BTC_RPC_PORT`,
`LIQUID_ELEMENTS_RPC_PORT`, `LIQUID_BTC_P2P_PORT`, and
`LIQUID_ELEMENTS_P2P_PORT`.  Each value must be a strict decimal integer from
`1` through `65535`, and all four configured ports must be pairwise distinct.

The runner uses fresh temporary datadirs, loopback-only RPC, temporary
credentials, named wallets, readiness polling, and an exit cleanup trap.  On
completion it writes proof envelopes, decoded transactions, daemon logs, and
an assertion/non-claim summary under a unique run directory below
`target/liquid-e2e-artifacts/`.  `LIQUID_E2E_ARTIFACT_DIR` may select a
different parent only when it resolves to an owned subdirectory inside
`target/`; the repository root, `target/`, `/`, `$HOME`, and symlinked or
non-owned paths are rejected.  Every run creates a marker file named
`.conxian-liquid-e2e-artifact-owner`, never clears an arbitrary parent, and
preserves the run directory and partial outputs on failure.  The workflow
uploads the default parent recursively, including each unique run directory.
The workflow serializes runs for the same ref; local runs can avoid daemon
port collisions by setting the documented `LIQUID_*_PORT` overrides.

The Elements peg-in policy is configured with
`LIQUID_PEGIN_CONFIRMATION_DEPTH` (strict decimal integer, default `100`,
range `2..1000`).  The harness passes this value to Elements and verifies the
same value through `getsidechaininfo`.  It retains the compatibility floor of
`102` parent confirmations and derives the claim target as
`max(102, LIQUID_PEGIN_CONFIRMATION_DEPTH + 2)`.  With the default depth of
`100`, the effective target remains `102` confirmations.

## Coverage and limits

The deterministic Rust tests run as ordinary workspace tests and do not need
daemons:

```bash
cargo test -p conxian_engine liquid_adapter --lib
```

The real-daemon script covers:

- live `getsidechaininfo` assertions for the configured parent genesis,
  pegged asset, and peg-in confirmation policy, while preserving the
  previously verified 102-parent-confirmation target;
- a matured Bitcoin regtest coinbase, a 1 BTC peg-in, 102 parent
  confirmations, raw transaction and `gettxoutproof` handling;
- an Elements `claimpegin` that delegates parent proof and claim consensus
  validation to Elements, with marker, witness, parent genesis/txid, pegged
  asset, confirmation, and duplicate-claim rejection assertions; its decoded
  pegged-asset claim output(s) and explicit Elements fee output are converted
  to integer satoshis and reconciled to exactly `100,000,000` sats;
- explicit-output parsing plus confidential-output commitments, surjection
  proofs, and the decoded range-proof bound semantics exposed by Elements;
- a confirmed `sendtomainchain` burn whose decoded raw transaction/output is
  checked for parent genesis, destination address and script, amount, asset,
  and fee.

This is a local protocol harness only.  It does **not** enable or test
production `LiquidAdapter` state-proof verification.  The production adapter
boundary remains fail-closed: caller-supplied metadata, including arbitrary
`{"verified": true, ...}` fields, is never accepted as a proof.  The harness
does not claim CT cryptographic proof validation beyond real daemon transaction
acceptance and decoded commitment/proof fields, and it does **not** test
automatic Bitcoin release, Watchmen release or batching, federation quorum,
PAK policy, production timing, or production federation coverage.
