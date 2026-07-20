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
can be supplied with `LIQUID_DAEMON_CACHE_DIR`; the RPC/P2P ports can be
overridden with `LIQUID_BTC_RPC_PORT`, `LIQUID_ELEMENTS_RPC_PORT`,
`LIQUID_BTC_P2P_PORT`, and `LIQUID_ELEMENTS_P2P_PORT`.

The runner uses fresh temporary datadirs, loopback-only RPC, temporary
credentials, named wallets, readiness polling, and an exit cleanup trap.  On
completion it writes proof envelopes, decoded transactions, daemon logs, and
an assertion/non-claim summary under `target/liquid-e2e-artifacts/`.  The
workflow serializes runs for the same ref; local runs can avoid collisions by
setting the documented `LIQUID_*_PORT` overrides.

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
production `LiquidAdapter` state-proof verification.  It does not claim CT
cryptographic proof validation beyond real daemon transaction acceptance and
decoded commitment/proof fields, and it does **not** test automatic Bitcoin
release, Watchmen release or batching, federation quorum, PAK policy,
production timing, or production federation coverage.
