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

The fixture is one deterministic, single-oracle, two-outcome enumerated
contract:

- event ID: `stage1-fixture-enum-event`;
- outcomes, in order: `no`, `yes`;
- oracle secret key: 32 bytes of `0x07`;
- oracle nonce secret: 32 bytes of `0x08`;
- offer/accept funding secrets: 32 bytes of `0x01` and `0x02`;
- alternate rejection-only secret: 32 bytes of `0x09`;
- synthetic previous-transaction script bytes: `[0x51, 0xa1]` and
  `[0x51, 0xb2]`;
- fixed input outpoints: offer
  `1f104165c17e18a495b8fb914718d6243c0a57f49a0f52a0c936eb63d385bb37:0`,
  accept
  `56cb847e68bb1d7261e7641614a76a20a2bbc6a0d2958abcae6b45cfbffccab0:0`;
- payout scripts: `[0x53, 0x01]` and `[0x53, 0x02]`;
- change scripts: `[0x52, 0x01]` and `[0x52, 0x02]`;
- empty synthetic redeem scripts for the fixed funding inputs;
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
| `OfferDlc` message | `460` bytes | `9f0d2968dfd08ba10a0cbc19e2cf781661cf7be22890070221c4ec1d7071e0dd` |
| `AcceptDlc` message | `577` bytes | `b73821f2f01c874c527cf3efecb250ea167598ba45017b6a51c5c45ea5f54fd6` |
| `SignDlc` message | `535` bytes | `c1cdd23db825f8adb4955872b55be63697fa971fc3486d1617552f49e570c104` |
| funding transaction txid | — | `34d0f8da92837a82ce313ef0edbdba6ed7f123f4c045c95555d83b43f6748a88` |
| CET for `no` txid | — | `3ca9d16d505bc7be6104a0e92a0d4b740e5a1aa7427c5d1427af1a98cd2bec2e` |
| CET for `yes` txid | — | `f98808178ba0e6ee43c7d7a20529ee132a2ba2534a7ed29ea84cd27d42889361` |
| refund transaction txid | — | `b18f3b6bf9a2b0652d99028ebd0948ebbc80fc8d4692d706274dd67345bdba44` |
| final contract ID | — | `25c1e9cb83926b93df202fe1fcacab7fc6e032e5d154d84444c92a52e7659b99` |
| funding output index | — | `0` |
| CET count | — | `2` |
| canonical fixture digest | — | `9f8ae3bf3098d69ef6dbf986df3348acc929b4df8b9f02169a300766f6f3443a` |

The canonical digest is SHA-256 over the ordered artifact stream
`offer`, `accept`, `sign`, `fund`, `cet-0`, `cet-1`, `refund`,
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
- funding input/outpoint order and funding output index;
- payout output ordering from the serial IDs;
- CET/refund locktimes and two-CET cardinality;
- payout and refund collateral conservation;
- stable message hashes, transaction IDs, final ID, and canonical digest;
- direct constructor equality for the CET and refund artifacts;
- non-empty signed CET/refund witnesses and valid refund signatures.

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

The malformed/incomplete checks are intentionally local boundary checks: they
exercise message parsing, cardinality, binding, and artifact invariants. They
do not pretend to be a substitute for manager/provider validation, wallet
state, or a full offer/accept/sign session.

## Reproduction

```bash
cargo +1.96.0 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check
cargo +1.85.1 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check

cargo +1.96.0 test --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-fixture
cargo +1.85.1 test --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-fixture

cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-fixture -- --emit
```

The normal run asserts the recorded constants and reports
`stage1_fixture=passed`; `--emit` prints the artifact expectations for review.

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
