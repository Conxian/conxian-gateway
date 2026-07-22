# DLC Ecosystem and Mainnet Evidence

- **As of:** 2026-07-22
- **Scope:** Conxian Gateway issue [#220](https://github.com/Conxian/conxian-gateway/issues/220)
- **Status:** Research and readiness alignment only; no DLC dependency or protocol code is added by this document.

## Executive summary

Conxian Gateway currently has a DLC-shaped API surface and an HTTP oracle adapter,
but it does **not** have a live DLC transaction engine. On `main` at
[`4a0433a`](https://github.com/Conxian/conxian-gateway/commit/4a0433ad92b83bb59d69cb64f86128c1e0212a8e):

- `internal/engine/Cargo.toml` and `Cargo.lock` contain no `dlc`, `dlc-manager`,
  `dlc-messages`, DDK, or equivalent DLC crate.
- `internal/engine/src/bitcoin/dlc_oracle.rs` fetches announcements and
  attestations and checks event ID, oracle public key, and expected outcome. It
  does **not** cryptographically verify the supplied signature.
- `pkg/conxian-core/src/lib.rs::DlcManager::create_dlc_bond` and the API bond
  handler still generate UUID-shaped mock identifiers; they do not construct a
  funding transaction, CET, refund transaction, or adaptor signature set.
- Earlier CET/dependency attempts in
  [`453a15a`](https://github.com/Conxian/conxian-gateway/commit/453a15ae8281adfd7678104fb910e552702ec673),
  [`8ef9d05`](https://github.com/Conxian/conxian-gateway/commit/8ef9d052d1979b032148c2c3574c5334e50a87e1),
  and the revert/removal in
  [`cb8b680`](https://github.com/Conxian/conxian-gateway/commit/cb8b680d205a8981229afaa24882c299aaab4b24)
  are historical evidence, not live implementation.

The practical conclusion is narrow: issue #220 should first produce a
deterministic, low-level CET spike and fixture suite. It should not yet select a
production dependency, expose a production bond endpoint, or make a mainnet or
institutional-readiness claim.

## 1. What this document does and does not claim

This document records source-backed research and promotion gates for a future
implementation. It deliberately separates:

1. **Protocol evidence:** specifications, standards, papers, vectors, and source
   code that explain how a DLC is constructed and verified.
2. **Implementation evidence:** SDKs and applications that can be inspected or
   compiled, without treating every implementation as canonical or compatible.
3. **Mainnet evidence:** an official project report bound to transaction IDs and
   block-explorer records. A transaction that merely looks like a normal Bitcoin
   spend is not, by itself, proof that it was a DLC.
4. **Gateway readiness:** what Conxian has actually implemented and tested. The
   gateway remains below the implementation gates listed in section 9.

The terms **DLC**, **Contract Execution Transaction (CET)**, **oracle
announcement**, **oracle attestation**, **adaptor signature**, and **refund
transaction** are used in the sense defined by the pinned specification below.

## 2. Gateway status at the research boundary

| Surface | Verified state on `main` | Readiness implication |
| --- | --- | --- |
| Engine dependencies | No DLC crates in `internal/engine/Cargo.toml` or `Cargo.lock` | No protocol implementation is wired into the workspace. |
| Oracle adapter | `dlc_oracle.rs` performs HTTP I/O and field matching | Useful transport-shaped scaffold; not signature verification. |
| Bond API | `POST /api/v1/dlc/bond` exists | Endpoint is an application scaffold, not a DLC constructor. |
| Bond manager | UUID-shaped mock IDs | Must remain explicitly non-production until backed by real state and transactions. |
| CET/refund flow | No live `dlc_cet.rs`, funding builder, CET builder, or refund builder | Issue #220 remains open. |
| Prior implementation attempts | Reverted after API/CI incompatibility | Dependency choice is unsettled and must be tested in isolation first. |

The dated continuity correction is recorded in
[`docs/SESSION_SUMMARY_2026-07-20.md`](../SESSION_SUMMARY_2026-07-20.md). This
document supersedes the older recommendation to add `dlc-manager` immediately:
the next checkpoint compares exact pinned APIs before any workspace change.

## 3. Canonical DLC specification

### 3.1 Source status and pin

The canonical working specification is
[`discreetlogcontracts/dlcspecs`](https://github.com/discreetlogcontracts/dlcspecs).
Its README calls the material an **in-progress specification** that is still
being drafted. The repository had no GitHub releases when reviewed. This report
pins the reviewed source to commit
[`9cd9148`](https://github.com/discreetlogcontracts/dlcspecs/commit/9cd9148938c616690c79d99ec6f330e213c246c5),
created on 2023-02-13, rather than relying on a mutable branch URL.

The pinned source tree and release ledger are:

| Source | Pin / link | Why it matters |
| --- | --- | --- |
| Specification tree | [`9cd9148`](https://github.com/discreetlogcontracts/dlcspecs/tree/9cd9148938c616690c79d99ec6f330e213c246c5) | Reproducible review point. |
| Specification README | [`README.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/README.md) | WIP/drafting status and implementation map. |
| GitHub releases | [releases](https://github.com/discreetlogcontracts/dlcspecs/releases) | No release artifact was listed at review time. |
| Test vectors | [`test/`](https://github.com/discreetlogcontracts/dlcspecs/tree/9cd9148938c616690c79d99ec6f330e213c246c5/test) | Serialization, transaction, adaptor, and contract fixtures. |
| Vector subdirectory | [`test/test_vectors/`](https://github.com/discreetlogcontracts/dlcspecs/tree/9cd9148938c616690c79d99ec6f330e213c246c5/test/test_vectors) | Enumerated, numerical, and multi-oracle vectors. |

### 3.2 Required protocol flow

The pinned [`Protocol.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Protocol.md)
defines the negotiation sequence:

1. **Offer:** the initiator sends contract information, oracle information,
   collateral, funding inputs, payout/change scripts, serial IDs, and locktimes.
2. **Accept:** the counterparty validates the offer and returns its funding
   information, adaptor signatures for every CET, and a refund signature.
3. **Sign:** the initiator returns the remaining signatures, including the
   funding transaction signature and its CET/refund signatures.
4. **Funding:** after both parties have committed to the same transaction set,
   the funding transaction is broadcast.
5. **Execution:** once the oracle attests an outcome, the matching adaptor
   signature can be adapted and the corresponding CET can be broadcast.
6. **Refund:** after the refund locktime, the pre-signed refund path remains the
   recovery path when the contract is not executed.

This ordering is important for issue #220: constructing a single transaction
that resembles a payout is not sufficient. The implementation must derive and
validate the complete offer/accept/sign state, funding transaction, every
required CET, and refund transaction from the same canonical contract data.

### 3.3 Oracle announcements and attestations

The pinned [`Oracle.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Oracle.md)
and [`Oracle-Validation.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Oracle-Validation.md)
make the following checks part of the contract boundary:

- An event descriptor identifies the event and the possible enumerated or
  numeric outcomes.
- An oracle announcement commits to the event, oracle key, and nonce/R-value
  material. The announcement contains a signature over its serialization and
  must validate against the stated oracle public key.
- An oracle attestation signs the outcome after maturity. The client must bind
  the attestation to the announced event and verify the signature, not merely
  compare strings supplied by the HTTP response.
- Oracle key selection and identity policy are separate from parsing. An
  implementation must reject an announcement whose key is not accepted by local
  policy or whose signature is invalid.
- The current protocol assumes a trusted oracle-selection policy; multi-oracle
  support reduces, but does not erase, oracle and implementation risk.

The first Conxian fixture milestone should use a small **enumerated outcome**
event. Numeric outcome compression and multi-oracle support should be added only
after the single-oracle vector path is interoperable.

### 3.4 Adaptor signatures and transaction adaptation

The pinned [`ECDSA-adaptor.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/ECDSA-adaptor.md)
and [`Transactions.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Transactions.md)
describe why each party can pre-sign the full CET set without the oracle
interacting with the contract:

- Each CET has a payout assignment for a possible outcome.
- The counterparty provides an adaptor signature for each CET, using the oracle
  point associated with that outcome.
- The final oracle signature reveals the adaptation secret for the matching CET.
- The adapted signature must validate for the exact serialized CET and funding
  outpoint; a signature for a different outcome or transaction must fail.

The oracle does not co-sign a contract-specific transaction. That distinction is
the privacy and operational property the gateway must preserve.

### 3.5 Serialization, ordering, and vectors are part of the protocol

The following pinned documents are implementation inputs, not optional reading:

| Document | Required use |
| --- | --- |
| [`Messaging.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Messaging.md) | TLV/message type, field, and fundamental-type serialization. |
| [`Non-Interactive-Protocol.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Non-Interactive-Protocol.md) | Non-interactive construction and signing boundaries. |
| [`NumericOutcome.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/NumericOutcome.md) | Numeric outcome decomposition and payout mapping. |
| [`NumericOutcomeCompression.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/NumericOutcomeCompression.md) | Reducing the number of CETs without changing the payout function. |
| [`PayoutCurve.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/PayoutCurve.md) | Deterministic payout interpolation and endpoint handling. |
| [`MultiOracle.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/MultiOracle.md) | Threshold and multi-oracle outcome composition. |
| [`v0Milestone.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/v0Milestone.md) | What the ecosystem considered complete, in progress, or future. |
| [`test/`](https://github.com/discreetlogcontracts/dlcspecs/tree/9cd9148938c616690c79d99ec6f330e213c246c5/test) | Exact byte-level fixtures for compatibility tests. |

Conxian must pin the source commit and vector files used by every test. A
dependency's passing unit tests are not a substitute for compatibility with the
canonical vectors.

## 4. Bitcoin standards and boundaries

DLC is **not itself a Bitcoin Improvement Proposal (BIP)**. The implementation
uses Bitcoin primitives and transaction rules covered by several BIPs:

| Standard | Relevance to issue #220 | Canonical source |
| --- | --- | --- |
| BIP340 | Schnorr signatures and tagged hashing used by the oracle/signature boundary. | [BIP340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki) |
| BIP141 | Segregated Witness transaction/input rules used by the DLC transaction specification. | [BIP141](https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki) |
| BIP67 | Lexicographic public-key ordering for multisig script construction where applicable. | [BIP67](https://github.com/bitcoin/bips/blob/master/bip-0067.mediawiki) |
| BIP125 | Later fee-bumping/recovery policy context, not a DLC construction requirement. | [BIP125](https://github.com/bitcoin/bips/blob/master/bip-0125.mediawiki) |

Taproot/Tapscript DLCs remain future work in the pinned specification. Issue
220's first milestone should therefore target the exact transaction family and
signature scheme supported by the selected vectors, rather than silently mixing
Taproot-era assumptions with the current ECDSA-adaptor vectors.

## 5. Research papers

These papers explain the protocol and its security model; they do not certify a
particular SDK or Conxian integration.

| Paper | Canonical link | Relevance | Conxian scope |
| --- | --- | --- | --- |
| Thaddeus Dryja, *Discreet Log Contracts* (2017) | [PDF](https://adiabat.github.io/dlc.pdf); [MIT DCI project](https://www.dci.mit.edu/projects/discreet-log-contracts) | Original construction: oracle-attested conditional Bitcoin payments with privacy and reduced oracle interaction. | Foundational model. |
| Lloyd Fournier, *One-Time Verifiably Encrypted Signatures / Adaptor Signatures* (2019) | [Repository](https://github.com/LLFourn/one-time-VES); [paper PDF](https://raw.githubusercontent.com/LLFourn/one-time-VES/master/main.pdf) | Formalizes the adaptor-signature primitive and the secret-recovery property used by DLC settlement. | Required cryptographic background; implementation still needs review. |
| Thibaut Le Guilly, Nadav Kohen, Ichiro Kuwahara, *Bitcoin Oracle Contracts: Discreet Log Contracts in Practice* (ICBC 2022) | [DOI](https://doi.org/10.1109/ICBC54727.2022.9805512) | Explains the practical protocol, adaptor signatures, numeric outcomes, and multi-oracle construction. | Directly informs the CET spike and vector plan. |
| Thibaut Le Guilly, Ichiro Kuwahara, Naoki Nakagawa, *Discreet Log Contracts Channels and Integration in the Lightning Network* (2020) | [Repository](https://github.com/p2pderivatives/offchain-dlc-paper); [PDF](https://raw.githubusercontent.com/p2pderivatives/offchain-dlc-paper/master/offchaindlc.pdf) | DLC channels and Lightning integration. | Future/out of scope for issue #220's CET-only, no-transport milestone. |
| Varun Madathil et al., *Cryptographic Oracle-Based Conditional Payments* (ePrint 2022 / NDSS 2023) | [IACR ePrint](https://eprint.iacr.org/2022/499); [NDSS page](https://www.ndss-symposium.org/ndss-paper/cryptographic-oracle-based-conditional-payments/) | Compares a threshold-oriented cryptographic construction for conditional payments and distributed oracle trust. | Comparison and future threshold research; not a reason to expand the first spike. |

## 6. SDK and implementation map

### 6.1 Primary Rust candidates

| Stack | Reviewed version and source | Shape | Strengths | Warnings / compatibility limits |
| --- | --- | --- | --- | --- |
| Upstream `rust-dlc` | `v0.8.0`, tag commit [`8e6a75f`](https://github.com/p2pderivatives/rust-dlc/commit/8e6a75fbc9685e6eafa348edd45a793fcb63fa4d), tagged 2025-12-13; [repo](https://github.com/p2pderivatives/rust-dlc/tree/v0.8.0); [docs.rs](https://docs.rs/dlc/0.8.0/dlc/) | Low-level Rust crates: `dlc`, `dlc-manager`, `dlc-messages`, `dlc-trie`, with storage/provider components. | Closest narrow fit for deterministic CET construction and direct vector/API inspection; MIT licensed; depends on Bitcoin `0.32.2` in the reviewed manifests. | Upstream README says it is early-stage, not thoroughly tested in production, and not fully spec-compliant. Do not use mainnet funds without a separate approval and review. |
| DLC Dev Kit (DDK) | `v1.1.2`, released 2026-06-29; [release](https://github.com/bennyhodl/dlcdevkit/releases/tag/v1.1.2); [repo](https://github.com/bennyhodl/dlcdevkit/tree/v1.1.2); [docs.rs](https://docs.rs/crate/ddk/1.1.2) | Higher-level application framework around wallet, manager, messages, trie, payouts, oracle, transport, and storage components: `ddk`, `ddk-manager`, `ddk-dlc`, `ddk-messages`, `ddk-trie`, `ddk-payouts`, `kormir`. | Broader BDK/Esplora, persistence, transport, and oracle integration; MIT licensed; reviewed workspace targets Bitcoin `0.32.6`. | Treat as a fork/evolution of the ecosystem, not as a wrapper or wire-compatible re-export. Compatibility with upstream vectors must be demonstrated. Its broader runtime is not justified by a CET-only spike unless persistence/state requirements demand it. |

DDK's release notes describe `v1.1.2` as a maintenance release and publish the
workspace crate list. The reviewed workspace manifests do not declare a formal
`rust-version`. The same is true for the reviewed `rust-dlc` manifests. A
dependency compiling against Bitcoin `0.32.x` does not prove compatibility with
Conxian's MSRV 1.85, the full gateway workspace, or the intended transaction
flow.

### 6.2 Bindings and secondary implementations

These projects are useful for comparison, fixtures, or operational ideas. They
are not all canonical, and none should be treated as proof that an SDK is safe
for Conxian production use.

| Project | Reviewed reference | Role / caveat |
| --- | --- | --- |
| Bitcoin-S | [release 1.9.12](https://github.com/bitcoin-s/bitcoin-s/releases/tag/1.9.12); [DLC execution docs](https://bitcoin-s.org/docs/next/wallet/wallet-election-example) | Active Scala implementation with a full wallet flow and practical offer/accept/sign/execute/refund documentation. Useful interoperability reference, not a Rust dependency. |
| DDK FFI | [release v0.3.41](https://github.com/bennyhodl/ddk-ffi/releases/tag/v0.3.41); [repo](https://github.com/bennyhodl/ddk-ffi/tree/v0.3.41) | Language bindings / low-level access for DDK. It does not remove the need to review the underlying Rust stack or wire compatibility. |
| `cfd-dlc` | [tag v0.0.8](https://github.com/p2pderivatives/cfd-dlc/tree/v0.0.8) | Older C++ reference and bindings family. GitHub metadata did not declare a license in this review; licensing must be verified before reuse. |
| NDLC | [release branch `releases/1.0.1`](https://github.com/dgarage/NDLC/tree/releases/1.0.1) | Experimental C# implementation; secondary reference only. |
| Kormir | [repository](https://github.com/bennyhodl/kormir) | Oracle implementation, not a complete contract engine. |
| Pythia | [official repository](https://github.com/dlc-markets/pythia) | Open-source numeric/oracle implementation for DLC-related price announcements and attestations; use as an oracle/application reference, not as a CET library. |
| Atomic Finance `node-dlc` | [repository](https://github.com/AtomicFinance/node-dlc) | TypeScript reference/application component; secondary evidence only. |

### 6.3 Isolated compatibility finding

An isolated dependency-level check against the exact `rust-dlc v0.8.0` and DDK
`v1.1.2` families compiled with the installed Rust 1.89.0 toolchain and Bitcoin
`0.32.x` dependency lines. This is deliberately a small claim:

- it does not prove the full offer/accept/sign flow;
- it does not prove serialization or vector compatibility between the families;
- it does not prove the repository MSRV 1.85;
- it does not prove API stability, persistence correctness, restart behavior, or
  mainnet safety.

The result supports a **checkpoint 0 API spike**, not a dependency decision.

### 6.4 Recommendation for issue #220

1. Compare pinned `rust-dlc v0.8.0` and DDK `v1.1.2` APIs in isolated, checked-in
   or reproducible spikes before modifying the workspace.
2. For the requested CET-only/no-transport scope, start with low-level upstream
   `rust-dlc` because it minimizes new runtime and persistence assumptions.
3. Keep DDK as the fallback if the gateway's required state model, wallet,
   Esplora, restart, or transport boundaries make the higher-level framework a
   better fit.
4. Record the selected exact crate versions, feature flags, Bitcoin version,
   vector commit, and MSRV result before adding dependencies.

This is a recommendation, not settled dependency selection or production
approval.

## 7. Mainnet evidence ledger

### 7.1 Evidence policy

The strongest public evidence is an official project report that names the DLC
semantics and binds them to transaction IDs that can be independently inspected.
Block explorers provide the immutable transaction record, but the on-chain
transaction alone cannot prove which off-chain contract or oracle semantics
produced it: DLC settlement is intentionally designed to be difficult to
distinguish from ordinary Bitcoin spends.

For every evidence item below:

- the official article is the semantic binding source;
- the Blockstream link is the transaction record;
- the evidence is a demonstration, not a security audit or production approval.

### 7.2 Crypto Garage / Skew S&P 500 option (2019)

**Official binding source:** Crypto Garage, [*skew. & Crypto Garage trade
peer-to-peer Bitcoin-settled S&P500 derivatives*](https://medium.com/crypto-garage/skew-crypto-garage-trade-peer-to-peer-bitcoin-settled-s-p500-derivatives-f9958db011dd),
published 2019-10-09. The company also describes its earlier Blockstream
derivative execution in its [official 2019 press release](https://cryptogarage.co.jp/en/news/20190419/).

| DLC role | Transaction | Blockstream record | Review result |
| --- | --- | --- | --- |
| Funding transaction | `afb9276cd578f96a1ba7fd45116ccbbe9ad5041da609ad5593f62a3cfb4d5fbd` | [funding tx](https://blockstream.info/tx/afb9276cd578f96a1ba7fd45116ccbbe9ad5041da609ad5593f62a3cfb4d5fbd) | Confirmed at block 593506 when checked through Blockstream's Esplora API on 2026-07-22; article identifies it as the fund transaction. |
| Contract Execution Transaction | `09e430468bca724cc01ec4fb1f9e66e1e3965ae3030e9510582f4660e9c232eb` | [CET](https://blockstream.info/tx/09e430468bca724cc01ec4fb1f9e66e1e3965ae3030e9510582f4660e9c232eb) | Confirmed at block 595771; article binds it to the settlement path. |
| Closing transaction | `bb2f4fb7c2c2ae0fae60d0efc763fab817d64bfef2a4f30d5584f752effc676d` | [close](https://blockstream.info/tx/bb2f4fb7c2c2ae0fae60d0efc763fab817d64bfef2a4f30d5584f752effc676d) | Confirmed at block 595772; article explains the oracle-signature-dependent close. |

This is the strongest evidence in this review because the official article
explicitly names the fund/CET/close decomposition and provides the transaction
IDs. It still remains a proof-of-concept trade, not a general production
certification.

### 7.3 Crypto Garage DLC inside a direct Lightning channel (2022 execution)

**Official binding source:** Crypto Garage's official article [*DLC on
Lightning*](https://note.com/crypto_garage/n/n8dcfc035c717), published 2025-04-01,
which documents a 2022 mainnet execution and explains that the implementation
was unstable and could lose mainnet funds. The article covers a direct channel
embedding a DLC; it does not claim routed DLC over Lightning.

| DLC/channel role | Transaction | Blockstream record | Review result |
| --- | --- | --- | --- |
| Channel funding | `f307a6330c25ff4a43290803b088754b03ffd9c90c556aebca9a89d0b0ff9988` | [funding tx](https://blockstream.info/tx/f307a6330c25ff4a43290803b088754b03ffd9c90c556aebca9a89d0b0ff9988) | Confirmed at block 764103 when checked through Blockstream's Esplora API on 2026-07-22. |
| Final CET | `c0819ea9d8fe73ce3ad79e7aedbcbe8931258e4961b456317a64420ae402aa7e` | [final CET](https://blockstream.info/tx/c0819ea9d8fe73ce3ad79e7aedbcbe8931258e4961b456317a64420ae402aa7e) | Confirmed at block 764970; article describes oracle-assisted CET execution during force close. |

This is valuable feasibility evidence for DLC composition with Lightning, but it
is explicitly **not** a production-readiness signal and is outside issue #220's
initial no-transport scope.

### 7.4 Application evidence without transaction-level proof

The following are useful application or implementation references, but they are
not promoted to transaction-level mainnet proof unless an official source binds
the semantics to specific transaction IDs:

- Bitcoin-S documents a [US 2020 election DLC example](https://bitcoin-s.org/docs/next/wallet/wallet-election-example)
  and a complete developer flow. This is application/implementation evidence.
- Bitcoin-S has an active [1.9.12 release](https://github.com/bitcoin-s/bitcoin-s/releases/tag/1.9.12)
  and remains a useful full-flow interoperability reference.
- Atomic Finance's [node-dlc repository](https://github.com/AtomicFinance/node-dlc)
  is a TypeScript implementation/reference; it is not used here as independent
  evidence of a specific settlement transaction.
- DLC Markets' public [Pythia repository](https://github.com/dlc-markets/pythia)
  is application/oracle evidence only. It does not establish a specific
  settlement transaction or replace protocol vectors or a security review.

A public Lygos page was not used as proof because its amount, script, and
transaction description did not align consistently enough to form a reliable
evidence binding.

### 7.5 Audit statement

For the principal `rust-dlc` and DDK stacks, the reviewed repositories, release
notes, crate documentation, and targeted public search did not reveal a public
independent security audit. This means **not found in the reviewed source
ledger**, not “none exists.” Any production or institutional claim still needs a
fresh audit search and an explicit security review of the exact pinned build.

## 8. Recommended staged plan

| Stage | Deliverable | Exit criteria | Status |
| --- | --- | --- | --- |
| 0. API/dependency spike | Compare exact `rust-dlc v0.8.0` and DDK `v1.1.2` APIs without changing gateway manifests. | Both compile at the dependency level; selected API, Bitcoin version, feature flags, vector pin, and MSRV result recorded. | Research checkpoint. |
| 1. Enumerated local fixture | Build a tiny single-oracle enumerated contract from a pinned vector. | Deterministic offer/accept/sign produces funding, every CET, and refund bytes; vector hashes and negative cases are checked. | Isolated conformance checkpoint: compatibility parse and rejection coverage recorded; full fixture gate remains open. |
| 2. Oracle cryptography | Replace field-only matching with announcement and attestation signature verification. | Wrong key, event, outcome, nonce, signature, and serialization all fail closed; valid vectors pass. | Not started. |
| 3. Gateway state boundary | Add typed contract state and atomic persistence only after the protocol core is stable. | Restart, idempotency, duplicate messages, funding disconnect, CET disconnect, and refund recovery are deterministic. | Not started. |
| 4. Public testnet / Mutinynet | Fund and execute a public testnet contract, including refund path. | Public txids, exact source/vector pin, logs, recovery procedure, and reproducible replay are archived. | Not started. |
| 5. Review and promotion | Security, operations, legal/compliance, and mainnet-trial review. | Written approval for the exact release; mainnet trial remains a separately approved future scope. | Not started. |

No stage authorizes custody, production bond issuance, institutional marketing,
or mainnet funds by itself.

### 8.1 Stage 1 isolated checkpoint — 2026-07-22

The focused [`DLC_STAGE1_CONFORMANCE_2026-07-22.md`](DLC_STAGE1_CONFORMANCE_2026-07-22.md)
checkpoint keeps all work under `experiments/dlc-stage0/`. It adds an explicit,
in-memory `localPayout` → `offerPayout` compatibility path for the seven
enumerated/mixed official vectors, without rewriting fixtures, and records
`14` parsed vectors with `13` complete offer/accept/sign byte matches. The
remaining hyperbola offer mismatch is deterministic: first difference at byte
offset `104` (`0x01` expected by the pinned spec, `0x40` emitted by
`rust-dlc`), with the spec's fixed-point sign/`u64`/`u16` encoding differing
from the library's IEEE-754 `f64` encoding.

The same isolated checkpoint has `7` deterministic tests passing: one valid
oracle boundary, wrong event/key/outcome rejection, invalid announcement and
attestation signature rejection, and mutated-CET transaction-binding
rejection. The pinned upstream attestation validator does not compare event
IDs, so the experiment wrapper enforces that binding explicitly. None of this
changes the Gateway's HTTP oracle/UUID scaffold or authorizes a dependency,
custody, CET, funding, refund, or production integration.

## 9. Readiness and security gates

Before a production-facing DLC path can be considered, all of the following
must be true:

- **Dependency/API pin:** exact crate versions, feature flags, transitive Bitcoin
  version, source commit, and license posture are recorded.
- **Enumerated-outcome fixtures:** local deterministic fixtures cover offer,
  accept, sign, funding, every CET, refund, and malformed inputs.
- **Official-vector compatibility:** the implementation matches the pinned
  `dlcspecs` serialization and test vectors byte-for-byte where applicable.
- **Cryptographic oracle verification:** announcement and attestation signatures
  are verified against the canonical serialization, key, event, outcome, and
  nonce material.
- **Deterministic transaction flow:** funding, CET, adaptor-signature
  adaptation, and refund construction are deterministic and fail closed.
- **Typed state and persistence:** contract state is typed, atomically persisted,
  restart-safe, idempotent, and safe against duplicate or out-of-order messages.
- **Public testnet evidence:** a public testnet/Mutinynet funding, CET, and
  refund sequence is reproducible and linked to exact source/vector pins.
- **Security/operations/legal review:** key custody boundaries, oracle policy,
  reorgs, fee/timelock policy, monitoring, incident recovery, and applicable
  legal/compliance review are complete.
- **Separate mainnet approval:** any mainnet trial is separately approved; it is
  not implied by passing local tests or by the existence of historical mainnet
  demonstrations.

## 10. Unresolved questions

1. Which exact `dlcspecs` commit and vector subset will Conxian support first?
2. Does the first milestone require only ECDSA adaptor signatures, or is there a
   reviewed need for the newer Schnorr/Taproot paths?
3. Which oracle announcement/attestation wire format and keyring policy should
   the gateway accept?
4. Should the first contract support only enumerated outcomes, or is there a
   concrete requirement for numeric payout curves?
5. Does Conxian need a low-level transaction library only, or will wallet,
   persistence, Esplora, and transport requirements justify DDK?
6. Which state transitions and persistence records are required for a safe
   restart after funding broadcast or oracle maturity?
7. What fee, locktime, reorg, and refund policy is acceptable for the intended
   settlement product?
8. What independent review and legal/compliance sign-off is required before any
   institutional or mainnet claim?

## 11. Canonical link index

### Protocol and standards

- [`dlcspecs` pinned tree](https://github.com/discreetlogcontracts/dlcspecs/tree/9cd9148938c616690c79d99ec6f330e213c246c5)
- [`Protocol.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Protocol.md)
- [`Oracle.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Oracle.md)
- [`Oracle-Validation.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Oracle-Validation.md)
- [`Transactions.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Transactions.md)
- [`ECDSA-adaptor.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/ECDSA-adaptor.md)
- [`Messaging.md`](https://github.com/discreetlogcontracts/dlcspecs/blob/9cd9148938c616690c79d99ec6f330e213c246c5/Messaging.md)
- [BIP340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki), [BIP141](https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki), [BIP67](https://github.com/bitcoin/bips/blob/master/bip-0067.mediawiki), [BIP125](https://github.com/bitcoin/bips/blob/master/bip-0125.mediawiki)

### SDKs and implementations

- [`rust-dlc v0.8.0`](https://github.com/p2pderivatives/rust-dlc/tree/v0.8.0) and [`dlc` docs.rs](https://docs.rs/dlc/0.8.0/dlc/)
- [`DLC Dev Kit v1.1.2`](https://github.com/bennyhodl/dlcdevkit/releases/tag/v1.1.2)
- [`DDK FFI v0.3.41`](https://github.com/bennyhodl/ddk-ffi/releases/tag/v0.3.41)
- [Bitcoin-S 1.9.12](https://github.com/bitcoin-s/bitcoin-s/releases/tag/1.9.12)
- [`cfd-dlc v0.0.8`](https://github.com/p2pderivatives/cfd-dlc/tree/v0.0.8), [NDLC 1.0.1](https://github.com/dgarage/NDLC/tree/releases/1.0.1), [Kormir](https://github.com/bennyhodl/kormir), [node-dlc](https://github.com/AtomicFinance/node-dlc)

### Mainnet evidence

- [Crypto Garage / Skew S&P 500 article](https://medium.com/crypto-garage/skew-crypto-garage-trade-peer-to-peer-bitcoin-settled-s-p500-derivatives-f9958db011dd)
- [Crypto Garage 2019 official press release](https://cryptogarage.co.jp/en/news/20190419/)
- [Crypto Garage DLC on Lightning](https://note.com/crypto_garage/n/n8dcfc035c717)
- [Blockstream Esplora API](https://github.com/Blockstream/esplora/blob/master/API.md)

## Conclusion

The ecosystem has credible protocol research, multiple implementations, and
officially documented mainnet demonstrations. It does not follow that a new
gateway can safely add a dependency and issue DLC bonds. Conxian's next
defensible move is a reproducible, low-level, enumerated-outcome CET spike with
cryptographic oracle verification and canonical vectors, followed by explicit
state, testnet, security, operations, and legal gates.
