# RGB Bitcoin Core regtest proof

This opt-in harness proves a real state-changing pinned RGB v0.12 transition:

```bash
bash tests/rgb/rgb_regtest_e2e.sh
```

The lane downloads checksum-pinned Bitcoin Core 31.1, creates a real regtest
genesis UTXO, constructs a `100 -> 40 receiver + 60 change` RGB operation,
commits its MMB/MPC root through OP_RETURN before signing, and asks Bitcoin Core
to fund, sign, mempool-check, broadcast and mine the exact witness transaction.
The receiver imports through `Bip340IssuerPolicy` using a deterministic
test-only key and verifies the exact 32-byte article callback commitment without
rehashing.

Bitcoin Core uses cookie authentication in the isolated temporary datadir. The
harness reads that credential only long enough to configure the Rust RPC client;
it is not passed on a command line or retained. Artifacts are written to
`target/rgb-regtest-artifacts/run.*`, including `proof.json`, sanitized daemon
logs, consignments and the generated stashes. A fail-closed guard scans every
retained file for the exact cookie secret and rejects/removes unsafe diagnostics
before workflow upload. The harness also proves that a bad BIP340 signature and
a transaction with the wrong Bitcoin commitment are rejected without mutating
receiver state, including after a fresh stash reload.

This is a test-only evidence lane. It does not expose a runtime import endpoint,
configure a production issuer key, load a permissive validator, or change the
default `RejectIssuerSignatures` production policy. The loopback Esplora URL is
intentionally unreachable: the receiver seal is witness-relative, so the pinned
verification path validates it against the included Bitcoin witness and does
not perform an Esplora query.
