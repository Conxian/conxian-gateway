# DLC Stage 0 SDK and Vector Comparison

- **Date:** 2026-07-22
- **Issue:** [Conxian/conxian-gateway#220](https://github.com/Conxian/conxian-gateway/issues/220)
- **Status:** Isolated research checkpoint; no Gateway dependency, manifest, lockfile, or runtime change.
- **Base verified:** [`175ac209f24099c3ff0c4dcd5143ea955007c0d8`](https://github.com/Conxian/conxian-gateway/commit/175ac209f24099c3ff0c4dcd5143ea955007c0d8)

## Scope and non-goals

Stage 0 compares two pinned Rust implementation families against the pinned
working DLC specification before any dependency is considered for the Gateway
workspace. It covers API shape, deterministic low-level transaction/oracle
probes, license and dependency posture, MSRV observations, and direct
consumption of the specification's enumerated vectors.

Stage 0 does **not** select a production dependency, modify a Gateway manifest,
construct a live bond, connect to Bitcoin Core or Electrs, provide persistence
or restart recovery, or make a production/mainnet safety claim. The checked-in
programs use fixed test keys and synthetic inputs only; they are bounded
non-production experiments.

## Exact pins and reviewed surfaces

| Candidate | Exact source and license | Probe surface | Features and resolved versions | Dependency implications |
| --- | --- | --- | --- | --- |
| Upstream `rust-dlc` `v0.8.0` | [`p2pderivatives/rust-dlc` commit `8e6a75f`](https://github.com/p2pderivatives/rust-dlc/commit/8e6a75fbc9685e6eafa348edd45a793fcb63fa4d), tag [`v0.8.0`](https://github.com/p2pderivatives/rust-dlc/tree/v0.8.0); repository MIT license ([license](https://github.com/p2pderivatives/rust-dlc/blob/8e6a75fbc9685e6eafa348edd45a793fcb63fa4d/LICENSE)) | `dlc`: `create_dlc_transactions`, `create_cets`, and refund construction; `dlc-messages`: oracle announcement/attestation and offer/accept/sign message types; `dlc-manager` is included as a compile-only library surface. | `use-serde` with the default `std` path. The committed probe lock resolves Bitcoin `0.32.102`, Lightning `0.0.125`, Miniscript `12.3.7`, and `secp256k1-zkp 0.11.0`. The reviewed manifests declare Bitcoin `^0.32.2` and do not declare `rust-version`. | Low-level and close to the CET-only scope. `dlc-manager` adds storage/provider traits and async manager APIs; its full test graph pulls external-node/provider and development dependencies, so manager integration is deferred. |
| DLC Dev Kit `v1.1.2` | [`bennyhodl/dlcdevkit` commit `e0ead558`](https://github.com/bennyhodl/dlcdevkit/commit/e0ead55870fab97510242b8d6d2a57ce1033008f), release [`v1.1.2`](https://github.com/bennyhodl/dlcdevkit/releases/tag/v1.1.2); workspace MIT license ([license](https://github.com/bennyhodl/dlcdevkit/blob/e0ead55870fab97510242b8d6d2a57ce1033008f/LICENSE)) | `ddk-dlc`: transaction creation and CET signing; `ddk-messages`: tagged oracle messages; `ddk-manager`: enum descriptors, adaptor verification, contract-input validation, and contract-info validation. | `std` and `use-serde` on the three probed crates. The original isolated DDK probe lock resolved Bitcoin `0.32.101`; the committed combined experiment lock unifies both candidates at Bitcoin `0.32.102` and resolves Lightning `0.2.4` plus `secp256k1-zkp 0.11.0`. The workspace declares Bitcoin `^0.32.6` and has no formal `rust-version`. The broader `ddk` package adds BDK wallet/chain/Esplora, Tokio, oracle/transport, and optional sled/SQLx storage surfaces. | Broader application framework than the current CET-only scope. It is useful as a fallback if wallet, transport, or persistence requirements justify it, but its dependency surface is larger and upstream vector compatibility is not established. |
| Working specification | [`discreetlogcontracts/dlcspecs` commit `9cd9148`](https://github.com/discreetlogcontracts/dlcspecs/commit/9cd9148938c616690c79d99ec6f330e213c246c5), including the [`test/test_vectors`](https://github.com/discreetlogcontracts/dlcspecs/tree/9cd9148938c616690c79d99ec6f330e213c246c5/test/test_vectors) directory. The repository describes the specification as in progress. | Offer/accept/sign byte fixtures and protocol schema used as an interoperability gate. | The vector probe consumes a caller-supplied checkout at the exact commit; it does not vendor or path-depend on the checkout. | The vectors are the compatibility authority for the next stage. Passing a library's own tests is not enough if its serde schema or bytes differ from the pinned vectors. |

The probe manifests use exact git revisions for the DLC crates and exact direct
Bitcoin/Lightning/secp256k1 versions shown above. The generated experiment lockfile
contains only canonical git and crates.io sources; it has no path dependency on
the Gateway or on a temporary checkout.

## Reproduction matrix and observed results

Run from the repository root with the pinned toolchains installed:

| Command | Rust `1.85.1` | Rust `1.96.0` | Observed result |
| --- | --- | --- | --- |
| `cargo fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check` | — | **PASS** | Both standalone probes format cleanly. |
| `cargo check --manifest-path experiments/dlc-stage0/Cargo.toml --workspace` | — | **PASS** | Both probe packages compile from the exact git pins. |
| `cargo run --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe --bin rust-dlc-stage0-probe` | **PASS** | **PASS** | Announcement/attestation validation, signed-outcome mutation rejection, funding transaction, two CETs, and refund construction pass. The deterministic funding txid is `c293224d29382edf7a7cd482b0dfb953938eaf9a45ad1ad603cf970bd284561c`. |
| `cargo run --manifest-path experiments/dlc-stage0/Cargo.toml -p ddk-stage0-probe --bin ddk-stage0-probe` | **BLOCKED** by the MSRV failure below | **PASS** | Enum oracle validation, two CETs, refund locktime, adaptor generation/verification, CET signing, and manager validation pass on 1.96.0. |
| `cargo check --manifest-path experiments/dlc-stage0/Cargo.toml -p rust-dlc-stage0-probe` | **PASS** | **PASS** | The bounded upstream low-level surface is compatible with the Gateway MSRV gate. |
| `cargo check --manifest-path experiments/dlc-stage0/Cargo.toml -p ddk-stage0-probe` | **EXPECTED FAIL** | **PASS** | DDK `ddk-messages` uses `usize::is_multiple_of`; Rust 1.85.1 reports the unstable `unsigned_is_multiple_of` library feature. The source is not weakened to force a pass. |

The original pinned `rust-dlc` checkout also showed that a library-only
`dlc-manager` check passes on Rust 1.85.1, while the full manager test/development
graph is blocked by resolved development dependencies requiring Rust 1.86.0
(including `icu_*` and `idna_adapter`). That external-node/provider graph is not
part of the bounded checked-in probe.

## Probe coverage and explicit non-coverage

### `rust-dlc-stage0-probe`

Covered:

- deterministic Schnorr oracle announcement validation;
- deterministic oracle attestation validation and rejection after signed-outcome mutation;
- synthetic offer/accept party parameters with fixed keys and inputs;
- funding transaction creation;
- two payout/CET transactions;
- refund transaction construction;
- compile-time checks for the low-level transaction constructor signatures.

Not covered:

- full manager offer/accept/sign negotiation;
- adaptor-signature generation or adaptation;
- Bitcoin Core, Electrs, wallet, storage, transport, fee estimation, or restart behavior;
- cryptographic or operational production review.

### `ddk-stage0-probe`

Covered:

- tagged enum oracle announcement and attestation validation;
- two DDK CETs and the refund locktime;
- adaptor-info generation and verification for both CETs;
- CET signing and witness population;
- enum descriptor, contract input, and contract info validation.

Not covered:

- a complete persisted manager session or restart/recovery path;
- transport, wallet, BDK/Esplora, Bitcoin Core, Electrs, or regtest integration;
- official `dlcspecs` vector compatibility;
- production security, audit, fee policy, or mainnet readiness.

## `dlcspecs` vector comparison

The exact vector checkout contains **14** JSON files. The bounded
`rust-dlc v0.8.0` vector probe attempts direct serde deserialization into the
upstream `OfferDlc`, `AcceptDlc`, and `SignDlc` types, then compares serialized
message bytes when deserialization succeeds.

### Schema gate

These seven enum/mixed vectors are blocked at deserialization:

- `enum_3_of_3_test.json`
- `enum_3_of_5_test.json`
- `enum_and_numerical_3_of_5_test.json`
- `enum_and_numerical_5_of_5_test.json`
- `enum_and_numerical_with_diff_3_of_5_test.json`
- `enum_and_numerical_with_diff_5_of_5_test.json`
- `enum_single_oracle_test.json`

The exact serde error is `missing field \`offerPayout\``. The pinned
specification vectors use the `localPayout` field in these contract entries,
while the `rust-dlc v0.8.0` serde model requires `offerPayout`. This is a schema
compatibility failure, not evidence that the vectors or the library can be
silently translated without a reviewed mapping.

### Numerical vectors

The seven numerical vectors deserialize directly:

| Vector | Offer bytes | Accept bytes | Sign bytes |
| --- | ---: | ---: | ---: |
| `single_oracle_numerical_hyperbola_test.json` | **FAIL** | PASS | PASS |
| `single_oracle_numerical_test.json` | PASS | PASS | PASS |
| `three_of_five_oracle_numerical_with_diff_test.json` | PASS | PASS | PASS |
| `three_of_three_oracle_numerical_test.json` | PASS | PASS | PASS |
| `three_of_three_oracle_numerical_with_diff_test.json` | PASS | PASS | PASS |
| `two_of_five_oracle_numerical_test.json` | PASS | PASS | PASS |
| `two_of_five_oracle_numerical_with_diff_test.json` | PASS | PASS | PASS |

Thus, **7/14** vectors parse directly, **6/7** directly parsed numerical
vectors round-trip all offer/accept/sign bytes, and the hyperbola vector differs
only in the offer bytes in this bounded comparison. DDK compatibility with the
official vectors was not demonstrated; its embedded fixture/schema surface is
different and remains a separate gate.

## Comparison and decision

| Gate | `rust-dlc v0.8.0` | DDK `v1.1.2` |
| --- | --- | --- |
| CET-only API fit | **Preferred candidate**: direct low-level constructors and message types | Works, but includes a wider manager/application surface |
| Deterministic local probe | Passes on Rust 1.85.1 and 1.96.0 | Passes on Rust 1.96.0 |
| Gateway MSRV 1.85.1 | Bounded probe passes; full manager dev graph needs dependency curation | Fails in source on `unsigned_is_multiple_of` |
| Bitcoin resolution | Original isolated locks: `0.32.102` / `0.32.101`; combined experiment lock: `0.32.102` | Original isolated lock: `0.32.101`; combined experiment lock: `0.32.102` |
| Official vector/schema compatibility | **Blocked** by `localPayout` vs `offerPayout`; 6/7 numerical byte sets match | Not demonstrated; embedded fixture schema differs |
| Persistence/transport implications | Deferred; manager/provider integration still requires external services | Larger framework with wallet, transport, oracle, and optional storage features |
| Current recommendation | Keep as the preferred Stage 1 candidate, gated | Keep as fallback; do not select now |

**Decision:** do not add either dependency to the Gateway workspace. Keep the
low-level `dlc` + `dlc-messages` family as the preferred candidate for Stage 1,
but block Stage 1 until the vector schema decision and deterministic full
offer/accept/sign fixture are resolved. DDK remains a fallback and does not
currently satisfy the Gateway MSRV gate.

## Stage 1 checkpoint acceptance criteria

Before any Gateway manifest or runtime change, the next checkpoint must provide:

1. a documented decision for the `localPayout`/`offerPayout` schema mapping or a
   confirmed upstream schema revision;
2. all enumerated official offer/accept/sign byte fixtures passing, not only the
   seven numerical vectors;
3. all numerical offer/accept/sign bytes passing, including the hyperbola offer;
4. every applicable CET and refund fixture passing with deterministic bytes;
5. malformed, wrong-event, wrong-oracle, signed-outcome-mutation,
   unannounced-enum-domain, invalid-signature, and wrong-transaction rejection
   tests;
6. a locked dependency graph that satisfies the Gateway MSRV of Rust 1.85.1;
7. an explicit decision about manager/provider integration and persistence;
8. persistence, restart/recovery, transport, and production operations deferred
   to a later stage unless the selected implementation makes them unavoidable.

Until these criteria pass, issue #220 remains open and the Gateway's existing
HTTP oracle/bond scaffold remains unchanged.

The next isolated checkpoint is recorded in
[`DLC_STAGE1_CONFORMANCE_2026-07-22.md`](DLC_STAGE1_CONFORMANCE_2026-07-22.md).
It documents the narrowly scoped `localPayout` compatibility normalization,
the deterministic hyperbola wire mismatch, and supported rejection coverage;
it does not change the Stage 0 decision or authorize Gateway integration.

## Canonical sources

- [`DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`](DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md)
- [`rust-dlc` repository](https://github.com/p2pderivatives/rust-dlc)
- [`rust-dlc v0.8.0 API docs`](https://docs.rs/dlc/0.8.0/dlc/)
- [`DLC Dev Kit repository`](https://github.com/bennyhodl/dlcdevkit)
- [`DLC Dev Kit v1.1.2 crate docs`](https://docs.rs/crate/ddk/1.1.2)
- [`dlcspecs` repository](https://github.com/discreetlogcontracts/dlcspecs)
- [`dlcspecs Protocol.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Protocol.md)
- [`dlcspecs test vectors`](https://github.com/discreetlogcontracts/dlcspecs/tree/9cd9148938c616690c79d99ec6f330e213c246c5/test/test_vectors)
