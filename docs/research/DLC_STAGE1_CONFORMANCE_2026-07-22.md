# DLC Stage 1 Isolated Conformance Checkpoint — 2026-07-22

This is the next bounded, non-production checkpoint for GitHub issue [#220](https://github.com/Conxian/conxian-gateway/issues/220).
It extends the Stage 0 comparison without changing the Gateway workspace,
runtime, endpoints, custody boundary, or production dependency graph.

## Scope and decision

The low-level `rust-dlc v0.8.0` candidate remains the preferred isolated
implementation surface. This checkpoint adds:

- an in-memory compatibility path for the seven enumerated/mixed vectors whose
  JSON uses `localPayout` while the pinned `rust-dlc` serde model requires
  `offerPayout`;
- deterministic first-difference diagnostics for the numerical hyperbola offer;
- seven deterministic oracle/transaction checks, including five oracle
  rejection cases and a transaction-binding rejection case.

The compatibility path is an experiment-only normalization, not a production
schema decision. It does not rewrite the pinned fixtures and it does not make
the hyperbola mismatch pass. Stage 1 is therefore **not complete**, and no
Gateway integration decision is authorized by this record.

## Exact pins and evidence

| Input | Pin |
| --- | --- |
| `rust-dlc`, `dlc`, `dlc-messages` | `v0.8.0`, commit [`8e6a75fbc9685e6eafa348edd45a793fcb63fa4d`](https://github.com/p2pderivatives/rust-dlc/commit/8e6a75fbc9685e6eafa348edd45a793fcb63fa4d) |
| `dlcspecs` vectors | commit [`9cd9148938c616690c79d99ec6f330e213c246c5`](https://github.com/discreetlogcontracts/dlcspecs/commit/9cd9148938c616690c79d99ec6f330e213c246c5) |
| Vector source history | `localPayout` introduced/updated in `dlcspecs` commits `db5b4fc` and `8ecdf2a` |

The experiment remains under `experiments/dlc-stage0/`; its dependencies are
not members of the Gateway Cargo workspace. The exact vector checkout is
caller-supplied at `DLC_SPECS_CHECKOUT/test/test_vectors`.

## `localPayout` / `offerPayout` evidence and compatibility path

Direct serde parsing still reproduces the Stage 0 schema failure:

- seven enumerated/mixed vectors stop with `missing field \\`offerPayout\\``;
- the pinned enum offer entries use `localPayout` in objects containing an
  `outcome`, for example
  `offer_message.message.contractInfo.singleContractInfo.contractInfo.contractDescriptor.enumeratedContractDescriptor.payouts[]`;
- the selected `rust-dlc v0.8.0` serde model requires `offerPayout` for the
  corresponding offer-side payout structure.

The new `--compatibility` mode parses each fixture into an in-memory
`serde_json::Value`, traverses only `offer_message.message`, and renames a key
only when all of these conditions hold:

1. the object contains `outcome` and `localPayout`;
2. the object does not already contain `offerPayout`;
3. the object is inside the offer message being normalized.

An ambiguous object containing both payout keys, or a `localPayout` key inside
the traversed offer message without an `outcome`, is rejected. The source JSON
text and checkout are never modified. This is a narrowly evidenced
compatibility experiment; it is not a general translation rule for future
message schemas.

## Vector results

The 14 pinned vectors produce the following results:

| Mode | Parsed | Blocked | All offer/accept/sign byte sets matching | Normalized fields |
| --- | ---: | ---: | ---: | ---: |
| Direct serde | 7 | 7 | 6 | 0 |
| In-memory compatibility mode | 14 | 0 | 13 | 28 |

The seven normalized vectors each contain four `localPayout` fields. After
normalization, all seven enum/mixed vectors match their offer, accept, and sign
bytes. Six of seven numerical vectors also match all three message byte sets.

### Hyperbola mismatch — deterministic incompatibility, not a pass

`single_oracle_numerical_hyperbola_test.json` remains:

- offer bytes: **FAIL**;
- accept bytes: **PASS**;
- sign bytes: **PASS**;
- expected offer length: `986` bytes;
- `rust-dlc` offer length: `968` bytes;
- first differing byte: offset `104`, expected `0x01`, actual `0x40`.

The first differing field is `translate_outcome`. The pinned `dlcspecs`
`PayoutCurve.md` hyperbola serialization specifies a sign byte, `u64` integer,
and `u16` extra precision for that field (and each subsequent numeric field).
The pinned `rust-dlc` source defines `translate_outcome` as `f64` and routes it
through `write_f64`, which writes an IEEE-754 big-endian eight-byte value. The
observed bytes therefore identify an upstream/spec wire-format incompatibility,
not a local construction bug that can be safely fixed in this experiment.

The vector probe records the first offset, byte windows, lengths, and field
classification on every reproduction. No mismatch is labeled as a pass.

## Deterministic rejection coverage

`rust-dlc-stage1-conformance` uses fixed test keys, a deterministic enumerated
oracle fixture, synthetic DLC transactions, and the selected library's actual
validation/adaptor-signature functions. It does not persist or export private
keys.

Seven tests pass on both supported toolchains:

1. valid announcement and attestation boundary accepts;
2. wrong event ID rejects at the experiment's binding wrapper;
3. wrong oracle key rejects;
4. wrong outcome rejects;
5. invalid announcement signature rejects;
6. invalid attestation signature rejects;
7. a mutated CET funding input/outpoint rejects adaptor-signature verification.

The wrong-event test records an important upstream boundary: the pinned
`OracleAttestation::validate` implementation verifies lengths, oracle key,
outcome signatures, and nonce points, but does not compare
`OracleAttestation.event_id` with `OracleEvent.event_id`. The experiment
wrapper adds that comparison and fails closed. This wrapper is not wired into
the Gateway.

The transaction test first verifies a valid adaptor signature, then mutates the
CET input's `previous_output` and confirms verification fails. This exercises
the transaction sighash binding; it is not a full funding transaction
reconciliation or broadcast test.

## Reproduction commands

From the repository root, with the exact `dlcspecs` checkout available:

```bash
cargo +1.96.0 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check
cargo +1.85.1 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check

cargo +1.96.0 test --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe
cargo +1.85.1 test --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe

cargo +1.96.0 test --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-conformance
cargo +1.85.1 test --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-conformance

cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe -- \
  --vectors "$DLC_SPECS_CHECKOUT/test/test_vectors"
cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe -- \
  --compatibility --vectors "$DLC_SPECS_CHECKOUT/test/test_vectors"

cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage1-conformance
python3 scripts/verify_contamination_guard.py
```

Observed targeted results on 2026-07-22:

- vector probe unit tests: `2 passed, 0 failed` on Rust 1.85.1 and 1.96.0;
- Stage 1 conformance tests: `7 passed, 0 failed` on Rust 1.85.1 and 1.96.0;
- direct vectors: `parsed:7 blocked:7 all_bytes_match:6`;
- compatibility vectors: `parsed:14 blocked:0 all_bytes_match:13 normalized_local_payouts:28`;
- the Stage 1 report: `total passed=7 failed=0`;
- the contamination guard result is recorded in the session summary for this
  checkpoint.

## Unresolved gates and next integration decision

The following remain open and prevent a Gateway dependency or runtime change:

- the hyperbola offer wire-format mismatch must be resolved by an explicit
  upstream/spec decision or a separately reviewed compatibility implementation;
- a full deterministic enumerated offer/accept/sign fixture and expected
  funding/CET/refund bytes are not yet established against an authoritative
  vector set;
- wrong nonce, malformed message, full funding-outpoint reconciliation, and
  complete CET/refund negative coverage remain open;
- manager/provider, persistence, restart/recovery, wallet, transport, and
  public testnet evidence remain unimplemented;
- the non-custodial signing boundary still needs a reviewed API decision;
- the Gateway's HTTP oracle and UUID/mock bond scaffold remains unchanged and
  is not cryptographic DLC verification or CET readiness.

**Decision:** keep `rust-dlc v0.8.0` as the preferred isolated low-level
candidate, retain the compatibility normalization and deterministic rejection
coverage as research evidence, and do not add any DLC dependency to the
Gateway workspace until the remaining wire-format and full-fixture gates are
resolved.

## Related records

- [`DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`](DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md)
- [`DLC_STAGE0_SDK_COMPARISON_2026-07-22.md`](DLC_STAGE0_SDK_COMPARISON_2026-07-22.md)
- [`SESSION_SUMMARY_2026-07-22_DLC_STAGE1.md`](../SESSION_SUMMARY_2026-07-22_DLC_STAGE1.md)
- [`experiments/dlc-stage0/README.md`](../../experiments/dlc-stage0/README.md)
