# DLC Stage 1 Deterministic Contract Fixture — 2026-07-22

This record documents the approved isolated fixture milestone for issue [#220](https://github.com/Conxian/conxian-gateway/issues/220).
It extends the earlier Stage 1 conformance checkpoint; it does not replace its
historical vector findings or make a Gateway integration claim.

## Scope and exact implementation pin

The fixture lives at
`experiments/dlc-stage0/rust-dlc-probe/src/bin/rust-dlc-stage1-fixture.rs`.
It is outside the root Gateway Cargo workspace and uses the low-level
`rust-dlc`, `dlc-manager`, and `dlc-messages` `v0.8.0` family at upstream commit
[`8e6a75fbc9685e6eafa348edd45a793fcb63fa4d`](https://github.com/p2pderivatives/rust-dlc/commit/8e6a75fbc9685e6eafa348edd45a793fcb63fa4d).
DDK `v1.1.2` is not part of this milestone because its isolated MSRV result is
outside this slice.

The fixture is one self-contained deterministic regression vector: a
single-oracle, two-outcome enumerated contract. It is not independent
interoperability evidence because it constructs both sides locally with the
pinned low-level APIs.

- event ID: `stage1-fixture-enum-event`;
- outcomes, in order: `no`, `yes`;
- oracle secret key: 32 bytes of `0x07`;
- oracle nonce secret: 32 bytes of `0x08`;
- offer/accept funding secrets: 32 bytes of `0x01` and `0x02`;
- alternate rejection-only secret: 32 bytes of `0x09`;
- native P2WPKH previous outputs derived from the offer/accept funding keys;
  the `FundingInput.redeem_script` values are empty, and the BIP143 script code
  used for signing is the corresponding P2PKH script code derived from each
  compressed funding key;
- synthetic previous-transaction locktimes `161` and `178` are used only to
  keep the two deterministic previous transaction IDs distinct; both
  referenced outputs carry exactly `100,000,000` satoshis;
- fixed input outpoints: offer
  `3e2b1dad8e66e6cba1e762711786a9ee2d9e96dc890b87251eee22821781e69e:0`,
  accept
  `5f420a1f4b9b7e5f9b39c8d1c54a8aa7ba651cd32030792a78bb2417bd0d9de0:0`;
- payout scripts: `[0x53, 0x01]` and `[0x53, 0x02]`;
- change scripts: `[0x52, 0x01]` and `[0x52, 0x02]`;
- native P2WPKH witness stacks are checked for exact two-element shape,
  compressed public-key/address semantics, witness-size bounds, and ECDSA
  validity against the parsed previous output value and script code;
- input amount: `100,000,000` satoshis per party;
- collateral: `50,000,000` satoshis per party;
- fee rate: `5` satoshis/vbyte;
- funding locktime: `0`;
- CET locktime and oracle maturity: `100`;
- refund locktime: `200`;
- funding output serial ID: `5`;
- input serial IDs: offer `10`, accept `20`;
- change serial IDs: offer `11`, accept `21`;
- payout serial IDs: offer `31`, accept `41`.

The fixture uses deterministic no-auxiliary-randomness Schnorr and adaptor
signature paths. Private keys are test-only values held in memory and are not
persisted or exported.

## Concrete artifacts and canonical expectations

The binary serializes each message as its Lightning wire type ID followed by
the `Writeable` message body. The following SHA-256 values are over those exact
serialized message bytes:

| Artifact | Length | SHA-256 / value |
| --- | ---: | --- |
| `OfferDlc` message | `480` bytes | `02429a798cf33c6a15bb8cd738c55ad6d581303fbd2370a2d93c79d1c36b1e4c` |
| `AcceptDlc` message | `597` bytes | `2ecb9087438980c0dc8749bce7da9b34be644038fbce20af667da92c2d00984a` |
| `SignDlc` message | `535` bytes | `2d4bfb0d31aafc3aa57693f58830dd008f8637891dbd3ba625067dfbc72d6c91` |
| funding transaction txid | — | `f4f0d66c02a0491307f545692d7cbeef9aca095b43a63191bddcaebca08a3334` |
| CET for `no` txid | — | `7f724daedb20461ac379dd0784eaad7acbc11099818b2162a94cbb3b756e2a97` |
| CET for `yes` txid | — | `6d3042cc2050c7fc91889c1e34efd940c15f04875e56323296242178ef499ff1` |
| refund transaction txid | — | `43d205a919923fb600c96e777f65e234cfaee8a84b1d48b1d1dc695b82762199` |
| final contract ID | — | `e5e1c77d13b1580216e454783c6daffe8bdb184a52b72080accdbfadb19b2225` |
| funding output index | — | `0` |
| CET count | — | `2` |
| canonical fixture digest | — | `bf13afe7352577f1cd3e28ca92098cd247e5e607948ed1283b1fd9e66ead1f40` |

The fully assembled funding transaction has the same txid as the unsigned
funding transaction because Bitcoin txids exclude witness data. Its two
P2WPKH witness stacks are nevertheless included in the serialized
`signed-fund` artifact and therefore bound by the canonical digest.

The canonical digest is SHA-256 over the ordered artifact stream
`offer`, `accept`, `sign`, `fund`, `signed-fund`, `cet-0`, `cet-1`, `refund`,
`signed-cet-offer-0`, `signed-cet-offer-1`, `signed-cet-accept-0`,
`signed-cet-accept-1`, and `signed-refund`. Each item contributes its UTF-8
label, an unsigned big-endian 64-bit byte length, and its serialized bytes.
This binds the message bytes, transaction bytes, CET ordering, and signed
artifact set without introducing a production persistence format.

## Assertions and rejection coverage

The positive path constructs concrete `OfferDlc`, `AcceptDlc`, and `SignDlc`
values; calls `dlc::create_dlc_transactions`, `dlc::create_cets`, and
`dlc::create_refund_transaction`; creates and verifies adaptor signatures for
both parties and both outcomes; signs both CET sets; creates a signed refund;
and validates the oracle announcement/attestation boundary. It asserts:

- temporary-to-final contract-ID binding using the pinned manager formula;
- parsed previous-transaction and outpoint/vout reconciliation for both
  funding inputs;
- native P2WPKH script/address semantics, empty redeem scripts, exact values,
  witness stack shape, and independent ECDSA verification for both assembled
  funding inputs;
- funding pubkeys, collateral, payout/change scripts and serial IDs, fee rate,
  chain hash, locktimes, signature cardinality/contents, and funding witness
  bytes across the messages;
- payout output ordering from the serial IDs;
- CET/refund locktimes and two-CET cardinality;
- payout and refund collateral conservation;
- independent verification of every signed CET and the signed refund against
  the exact transaction, input, funding script, value, and public keys;
- full type-ID-aware serialization round trips for `OfferDlc`, `AcceptDlc`, and
  `SignDlc`, with equality against the original typed values;
- stable message hashes, transaction IDs, final ID, and canonical digest;
- direct constructor equality for the CET and refund artifacts;
- valid raw refund signatures and exact witness stack shapes.

The fixture has 13 deterministic fail-closed cases:

1. changed temporary contract ID;
2. changed final contract ID;
3. changed funding outpoint field;
4. changed enumerated payout amount;
5. changed payout serial ID;
6. wrong oracle outcome paired with the other outcome's signature;
7. wrong oracle signing key;
8. wrong adaptor outcome (`maybe`);
9. incomplete accept adaptor-signature set;
10. incomplete sign funding-signature set;
11. truncated accept message body;
12. refund locktime mismatch;
13. refund collateral/output mismatch.

The malformed/incomplete checks assert named fixture validation categories or
specific pinned DLC/Lightning decode categories. They exercise message
parsing, cardinality, binding, and artifact invariants; they do not pretend to
be a substitute for manager/provider validation, wallet state, or a full
offer/accept/sign session.

## Reproduction

```bash
cargo +1.96.0 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check
cargo +1.85.1 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check

cargo +1.96.0 test --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-fixture
cargo +1.85.1 test --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-fixture

cargo +1.96.0 clippy --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-fixture -- -D warnings
cargo +1.85.1 clippy --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-fixture -- -D warnings

cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-fixture -- --emit
```

The normal run asserts the recorded constants and reports
`stage1_fixture=passed`; `--emit` prints the artifact expectations for review.
The output is a self-contained deterministic regression vector, not
independent interoperability evidence.

## Boundary decisions

- `localPayout` → `offerPayout` normalization remains only in the existing
  vector-probe compatibility mode. This fixture does not add a general schema
  translation or alter canonical vectors.
- The fixture is **not** Gateway runtime integration, API integration, wallet
  integration, transport, persistence, node/oracle I/O, custody, or a funding
  broadcast path.
- It does not implement numeric outcomes, hyperbola payout curves, numeric
  compression, multi-oracle contracts, Taproot/Schnorr CET settlement, or any
  production signing policy. The earlier hyperbola wire-format mismatch remains
  an open compatibility issue.
- Issue #220 remains open. This is the isolated Stage 1 deterministic fixture
  milestone only; it does not authorize a Gateway dependency, production bond
  issuance, custody, testnet funds, mainnet funds, or institutional readiness.
