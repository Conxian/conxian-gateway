# BitVM3 / BitVMX Evidence and Cross-Repository Triage

> **Research / Evaluation Only** — evidence refresh date: 2026-07-22
>
> This report is the canonical refresh for [Gateway issue #189](https://github.com/Conxian/conxian-gateway/issues/189). It records upstream claims, public artifacts, and Conxian source boundaries separately. It does not authorize production integration, settlement, custody, compliance decisions, or a mainnet deployment.

## Executive verdict

**Keep #189 open and research-only. Do not add a BitVM3, BitVMX-GC, or garbled-circuit production adapter in the current milestone.**

The evidence now supports five distinct conclusions:

1. **BitVM3 is authoritative as a paper/protocol result, not as a shipped SDK.** IACR ePrint 2026/933 records the paper as received on 2026-05-11 and revised on 2026-06-08. The paper describes BitVM3 bridge/core constructions and garbled-circuit verification; no stable BitVM3 SDK, release, or verified production deployment was found.
2. **The official BitVM Rust repository is a BitVM2 developer preview.** Its README warns not to use it in production, and the official public demo graph is BitVM signet/`bitvmnet`, not Bitcoin mainnet. Its releases are alpha/development labels rather than evidence of a production bridge.
3. **A BitVMX mainnet SNARK prototype transaction exists, but it is not BitVM3-GC.** The official BitVMX article links transaction [`75eb2ad4f22263440fc4ceb61c51b0bb77721661dbfbec961358520b04107ec3`](https://mempool.space/tx/75eb2ad4f22263440fc4ceb61c51b0bb77721661dbfbec961358520b04107ec3) and describes an interactive SNARK-verifier execution. This is upstream prototype evidence, not a stable SDK, production bridge, Conxian verifier, or audit.
4. **BitVMX-GC has public design/integration claims but no verified stable GC API or release.** The current official article is [`/knowledge/implementing-garbled-circuits-for-bitvmx`](https://bitvmx.org/knowledge/implementing-garbled-circuits-for-bitvmx); the prior `/blog/...` path is stale. The article establishes architecture and client-integration intent, not a versioned SDK contract.
5. **No verified BitVM3 or BitVMX-GC production deployment was found.** Union Bridge is documented upstream as Rootstock testnet/experimental, with dispute mechanisms inactive in V1.5, no formal audit, and a 2027 mainnet roadmap. That is not production-mainnet evidence.

The Gateway posture is therefore unchanged: retain the isolated BitVMX-CPU evaluator and backend-neutral Groth16 boundary as research/interface artifacts, keep production verification and settlement paths fail-closed, and use the linked cross-repository issues for remediation.

## Phase 4 refresh record — 2026-07-22

This documentation consolidation preserves the Phase 1 cross-repository contract
findings, the Phase 2 upstream research refresh, and the implementation handoff
for [Gateway PR #278](https://github.com/Conxian/conxian-gateway/pull/278).

- The verified research base before PR #278 was Gateway commit
  [`d7032ab621ad038f247566f820ac664a6c8c071c`](https://github.com/Conxian/conxian-gateway/commit/d7032ab621ad038f247566f820ac664a6c8c071c).
- The fail-closed implementation is commit
  [`114b857ed9d400beaf474cb68e7ac5f25ef58d78`](https://github.com/Conxian/conxian-gateway/commit/114b857ed9d400beaf474cb68e7ac5f25ef58d78)
  on `charlie/issue-189-bitvm-fail-closed`.
- Before this documentation consolidation, the existing PR branch was at
  [`c893cbb39ea9d680b229a89035ab38f29ed51b8b`](https://github.com/Conxian/conxian-gateway/commit/c893cbb39ea9d680b229a89035ab38f29ed51b8b),
  including the `main` merge at
  [`81d175540922b25192b683e95c9b48230c009454`](https://github.com/Conxian/conxian-gateway/commit/81d175540922b25192b683e95c9b48230c009454).
- The Phase 4 continuity checkpoint predates the external merge of PR #278.
  GitHub reports that it was merged externally on
  2026-07-22T19:57:47Z by `botshelomokoka` as merge commit
  [`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b).
  Charlie did not merge PR #278.
- The Phase 4 documentation commit
  [`e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005`](https://github.com/Conxian/conxian-gateway/commit/e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005)
  was created at 2026-07-22T20:07:46Z and pushed after that merge, so these
  docs are on the post-merge branch head and are not part of merged `main`.
  This documentation recovery is carried by a separate follow-up PR that
  remains pending review/merge until it lands in `main`.
  This report remains a research and implementation handoff; it does not
  resolve [issue #189](https://github.com/Conxian/conxian-gateway/issues/189)
  or authorize a cryptographic backend, settlement, custody, compliance, or
  production deployment.

## 1. Evidence taxonomy

| Evidence class | Meaning in this report | What it does not prove |
|---|---|---|
| **Source-verified** | The linked paper, repository, issue, release, article, or local file was inspected at the stated access date/ref. | Correctness, security, production suitability, or independent reproduction of the source's claims. |
| **Upstream-reported** | A source states a benchmark, capability, roadmap, or deployment fact that was not independently reproduced by Conxian. | A Conxian performance result, audit result, or production authorization. |
| **On-chain artifact** | A transaction is publicly addressable on a named network and linked by the upstream project. | That the transaction proves the advertised protocol, was generated by a secure implementation, or represents BitVM3-GC. |
| **Local implementation evidence** | Current Conxian source/tests demonstrate a boundary, parser, fixture, or simulation behavior. | A cryptographic backend, protocol soundness, or deployment readiness unless explicitly tested and reviewed. |
| **Unknown / unresolved** | Release, license, security, network, or implementation evidence was not available or was contradictory. | A basis for optimistic assumptions. |

## 2. Terminology and non-interchangeability

| Subject | Correct classification | Conxian boundary |
|---|---|---|
| **Groth16** | A pairing-based SNARK proof system. In BitVM designs it may be the computation being verified or compiled into a larger verification construction. | Arkworks dependencies or a trait name are not evidence that a production pairing backend is wired. |
| **Recursive Groth16** | A Groth16 proof verified inside another proof/verification composition. It is not synonymous with BitVM3, BitVMX, or garbled circuits. | Do not describe the current Gateway boundary or Citrea adapter as a BitVM3 recursive-Groth16 implementation. |
| **Nova / IVC / folding** | A separate recursive/folding proof family with its own commitments, curves, and assumptions. | Nova is a comparison track, not a substitute for a BitVM3/GC backend. |
| **Garbled circuits** | A circuit-evaluation technique used by BitVM3/BitVMX-GC designs and reference projects. | A public paper, article, or toy repository is not a stable GC SDK. |
| **BitVM2** | An optimistic challenge/verification protocol. The official BitVM Rust repository and signet demo are BitVM2-oriented. | BitVM2 proof/demo evidence must not be relabeled as BitVM3 or BitVMX-GC evidence. |
| **BitVM3** | The BitVM3 paper/protocol family: garbled-circuit-based bridge/core research. | Paper/prototype evidence does not provide a release, API, or production bridge. |
| **BitVMX-CPU** | A Rust/RISC-V emulator and challenge-response execution protocol. | The Gateway evaluator is an isolated CPU harness; it is not GC or Groth16 verification. |
| **BitVMX-GC** | A BitVMX garbled-circuit/DV-SNARK plug-in described by upstream design material. | No stable public revision, release, narrow API, vectors, or production deployment was verified. |

## 3. Official SDK, repository, and release maturity matrix

Release labels below are reproduced as upstream metadata; labels such as `alpha`, `dev`, and `review` are not treated as stable production releases.

| Surface | Official source | Verified maturity on 2026-07-22 | Triage disposition |
|---|---|---|---|
| **BitVM Rust repository** | [`BitVM/BitVM`](https://github.com/BitVM/BitVM) | MIT metadata; releases include `v0.1.2-dev`, `v0.1.1-dev`, and `v0.1.0-alpha`. README says **DO NOT USE IN PRODUCTION** and describes the implementation as BitVM2/SNARK-verifier work. | Developer preview/reference only; not a BitVM3 SDK or production bridge dependency. |
| **BitVM official demo** | [`BitVM/bitvm.github.io/demo`](https://github.com/BitVM/bitvm.github.io/tree/main/demo) and [`DEMO_INSTRUCTIONS.md`](https://github.com/BitVM/BitVM/blob/main/DEMO_INSTRUCTIONS.md) | Calls itself a BitVM Developer Preview and identifies a public BitVM signet/`bitvmnet` for development/testing. The demo code tag is `v0.1.0-alpha`. | Signet demo evidence only; never call its graph Bitcoin mainnet evidence. |
| **BitVMX platform** | [`bitvmx.org/platform`](https://bitvmx.org/platform) | Public platform description: BitVMX-CPU is the first integrated verification protocol; BitVMX-GC support is described as coming in 2026. No versioned GC SDK/API was verified. | Roadmap/design evidence; monitor, do not integrate. |
| **BitVMX-CPU** | [`FairgateLabs/BitVMX-CPU`](https://github.com/FairgateLabs/BitVMX-CPU), [pinned `main`/`v0.7.0`](https://github.com/FairgateLabs/BitVMX-CPU/tree/d390832c8e0f2a01453e8ef4bf65dbe715fb9236), [tag `v0.8.0`](https://github.com/FairgateLabs/BitVMX-CPU/tree/v0.8.0), [release `v0.5.11`](https://github.com/FairgateLabs/BitVMX-CPU/releases/tag/v0.5.11) | README says under development, unaudited, not production-ready, and breaking changes may occur. The pinned/default-branch commit is `d390832c8e0f2a01453e8ef4bf65dbe715fb9236` (`v0.7.0`), newer tag `v0.8.0` resolves to `e23fbfccb0b50b52c882e6ba4f57eba3b7c3887f`, and the latest GitHub Release is `v0.5.11`; default branch, tag, and release therefore diverge. Repository metadata and `LICENSE` say Apache-2.0 while README says MIT. | Isolated evaluator only; license and release maturity remain unresolved. |
| **BitVMX client** | [`FairgateLabs/rust-bitvmx-client`](https://github.com/FairgateLabs/rust-bitvmx-client) | MIT metadata; public CLI. Releases include `v0.5.1` and `v0.1.4-alpha`; open issues cover counterproofs, dispute slots, SPV signaling, disablement, DOS, keys, and transaction speedups. | Public alpha/reviewing ecosystem component, not evidence of a stable GC verifier or production bridge. |
| **BitVMX workspace** | [`FairgateLabs/rust-bitvmx-workspace`](https://github.com/FairgateLabs/rust-bitvmx-workspace) | Public workspace release `v0.1.2-alpha`; no stable production contract verified. | Use as source organization/release context only. |
| **Protocol builder** | [`FairgateLabs/rust-bitvmx-protocol-builder`](https://github.com/FairgateLabs/rust-bitvmx-protocol-builder) | Public DAG transaction-template library; releases include `v0.0.2-review.1` and earlier pre-refactor labels. | Component-level review artifact; not a GC verifier or production readiness signal. |
| **ZK-proof component** | [`FairgateLabs/rust-bitvmx-zk-proof`](https://github.com/FairgateLabs/rust-bitvmx-zk-proof) | Public component with `v0.1.0-review.1`; no stable production contract verified. | Review component only; do not infer a complete BitVMX-GC SDK. |
| **Key manager** | [`FairgateLabs/rust-bitvmx-key-manager`](https://github.com/FairgateLabs/rust-bitvmx-key-manager) | Public component with `v0.0.2-review.1`; no stable production contract verified. | Supporting component only; requires protocol, key, and security review. |
| **Storage backend** | [`FairgateLabs/rust-bitvmx-storage-backend`](https://github.com/FairgateLabs/rust-bitvmx-storage-backend) | Public component with `v0.1.1-review.1`; no stable production contract verified. | Supporting component only; not proof verification. |
| **Transaction monitor** | [`FairgateLabs/rust-bitvmx-transaction-monitor`](https://github.com/FairgateLabs/rust-bitvmx-transaction-monitor) | Public component with `v0.0.2-review.1`; no stable production contract verified. | Operational component only; not proof verification. |
| **ZK verifier toolkit** | [`FairgateLabs/bitvmx-zk-verifier`](https://github.com/FairgateLabs/bitvmx-zk-verifier) | Repository description says **WIP**; no GitHub release was verified. | WIP/reference only. |
| **Toy garbling project** | [`FairgateLabs/BitVM3-garbling-toy`](https://github.com/FairgateLabs/BitVM3-garbling-toy) | Public toy repository with no GitHub release verified. | Educational/reference material; not a BitVM3 SDK. |
| **GOAT GC reference** | [`GOATNetwork/bitvm2-gc`](https://github.com/GOATNetwork/bitvm2-gc), [`Cargo.toml`](https://github.com/GOATNetwork/bitvm2-gc/blob/main/Cargo.toml) | Public source; no GitHub release, root license artifact, or GitHub-detected license was verified. The Cargo workspace declares `MIT OR Apache-2.0`. README reports roughly 10.4B gates and 51–374 GB peak memory for listed workloads; those figures are upstream-reported. | Research/reference only; license, release, resource, and reproducibility gates block vendoring, CI integration, or production claims. |
| **Garbled SNARK verifier** | [`BitVM/garbled-snark-verifier`](https://github.com/BitVM/garbled-snark-verifier), [Cargo package `0.5.0`](https://crates.io/crates/garbled-snark-verifier/0.5.0), [tag `v0.5.0-alpha.6`](https://github.com/BitVM/garbled-snark-verifier/tree/v0.5.0-alpha.6), [older tag `v0.3.0`](https://github.com/BitVM/garbled-snark-verifier/tree/v0.3.0), [`LICENCE`](https://github.com/BitVM/garbled-snark-verifier/blob/main/LICENCE) | Cargo package `0.5.0`; tag `v0.5.0-alpha.6` and older `v0.3.0`; GPL-3.0-only repository metadata/`LICENCE`; no GitHub Release. Open subgroup, commitment/on-chain-data, gate-hash, garbling-table-hash, and ARM64 issues remain. | GPL reference implementation; adoption blocked pending security, format, license, and platform review. |

The FairgateLabs ecosystem is useful for source and component triage, but the presence of many public repositories does not establish a stable, audited, production BitVMX-GC SDK.

## 4. Primary paper and specification matrix

| Source | What it establishes | Engineering interpretation |
|---|---|---|
| [IACR ePrint 2026/933](https://eprint.iacr.org/2026/933) | BitVM3 paper record; received 2026-05-11, revised 2026-06-08. Describes BitVM3-bridge/core, garbled circuits, and a Bitcoin light-client construction. | Authoritative research/paper evidence. It does not provide a software API, release, audit, or production deployment. |
| [BitVM3 paper PDF](https://bitvm.org/bitvm3.pdf) | “Efficient Bitcoin Bridges via Garbled Circuits” and its protocol/bridge construction. | Paper/prototype evidence. A Groth16 verifier appearing as a circuit component is not recursive Groth16 verification by Conxian. |
| [BitVM2 design](https://bitvm.org/bitvm2) | Optimistic challenge/verification design and the role of a SNARK verifier such as Groth16. | Protocol design, not proof that an available production Groth16 backend exists. |
| [SNARK verifier in Bitcoin Script](https://bitvm.org/snark.html) | Design notes for Groth16/FFlonk-style verification and script/field-operation trade-offs. | Upstream design estimates; not a current Gateway capacity or security result. |
| [BitVMX whitepaper](https://bitvmx.org/files/bitvmx-whitepaper.pdf) | CPU/trace-based universal computation and challenge-response verification on Bitcoin. | BitVMX-CPU/trace protocol research; distinct from GC and BitVM3. |
| [BitVMX GC article](https://bitvmx.org/knowledge/implementing-garbled-circuits-for-bitvmx) | Current article describes GC plus DV-SNARK layering and client/platform integration intent. | Architecture/announcement evidence only; no versioned stable GC SDK/API is supplied. The prior `/blog/implementing-garbled-circuits-for-bitvmx` URL is stale. |
| [BitVMX SNARK prototype article](https://bitvmx.org/knowledge/a-new-era-for-bitcoin-successful-snark-proof-verification-with-bitvmx) | Upstream announcement of interactive SNARK-verifier execution on Bitcoin mainnet and a linked transaction. | Mainnet prototype evidence for BitVMX-CPU-style execution; not BitVM3-GC, a production bridge, or a Conxian verifier. |
| [Union Bridge testnet article](https://bitvmx.org/knowledge/union-bridge-reaches-testnet-a-milestone-for-bitvmx-powered-bitcoin-bridging) | Upstream says Union Bridge is on Rootstock Testnet/experimental; V1.5 dispute/fraud mechanisms are inactive, no formal audit has been conducted, and mainnet is planned for 2027. | Testnet/roadmap evidence; not production-mainnet evidence. |
| [Microsoft Nova](https://github.com/microsoft/Nova) | Recursive SNARK/IVC and folding-system implementation/research. | Separate proof-system comparison track; not BitVM3, BitVMX-GC, or a Gateway dependency. |

## 5. Network and proof evidence matrix

| Network/evidence class | Representative artifact | Verified classification | Explicit non-claim |
|---|---|---|---|
| **Bitcoin mainnet prototype** | [BitVMX article](https://bitvmx.org/knowledge/a-new-era-for-bitcoin-successful-snark-proof-verification-with-bitvmx) and [transaction `75eb2ad4...`](https://mempool.space/tx/75eb2ad4f22263440fc4ceb61c51b0bb77721661dbfbec961358520b04107ec3) | The transaction page confirms the artifact at Bitcoin block `853871` with block time `2024-07-25`; the upstream article describes it as an interactive SNARK-verifier execution. This is historical prototype evidence. | Not BitVM3-GC, not a stable SDK, not a production bridge, not a Conxian verifier, and not audit evidence. |
| **BitVM public signet** | [Official demo graph](https://github.com/BitVM/bitvm.github.io/blob/main/demo/README.md), [BitVM signet-network PR #228](https://github.com/BitVM/BitVM/pull/228), and representative [peg-in](https://mempool.bitvmnet.org/tx/4dd5d195073af820875b5f19dc2ab30862798af2ea63fc37aecbe1051f1e8688), [assert-final](https://mempool.bitvmnet.org/tx/e7da86777532342521f80bbf2bfc477ebbab289866b6c2842673a006ec34512a), and [disprove](https://mempool.bitvmnet.org/tx/ee29855315760b5b839ad20c9ce19a1e235c54afc2431b2a527b97458c0ab8e5) transactions | Official BitVM Developer Preview graph on BitVM signet/`bitvmnet` for development/testing. | These are signet transactions, not Bitcoin mainnet transactions and not BitVM3-GC deployment evidence. |
| **Rootstock testnet** | [Union Bridge testnet announcement](https://bitvmx.org/knowledge/union-bridge-reaches-testnet-a-milestone-for-bitvmx-powered-bitcoin-bridging) | Experimental Rootstock Testnet bridge milestone; V1.5 dispute mechanisms inactive, no formal audit, 2027 mainnet roadmap. | Not a production bridge or Bitcoin mainnet deployment. |
| **Paper/prototype** | [BitVM3 ePrint record](https://eprint.iacr.org/2026/933) and [paper PDF](https://bitvm.org/bitvm3.pdf) | Research paper with prototype/cost discussion. | Not a release, SDK, verified deployment, audit, or Conxian implementation. |
| **Announcement/design** | [BitVMX platform](https://bitvmx.org/platform) and [GC article](https://bitvmx.org/knowledge/implementing-garbled-circuits-for-bitvmx) | Public roadmap/design claims for BitVMX-CPU and BitVMX-GC. | Not a versioned SDK, reproducible build, security review, or production deployment. |

**Bottom line:** no verified BitVM3 or BitVMX-GC production deployment was found in this refresh. The mainnet prototype and signet graph must remain separately labeled by network and protocol generation.

## 6. Upstream issue and pull-request dependency matrix

These are dependencies and blockers, not endorsements. All listed items were open when inspected on 2026-07-22 unless stated otherwise.

### BitVM repository

| Link | Upstream item | Why it matters to #189 |
|---|---|---|
| [BitVM #285](https://github.com/BitVM/BitVM/issues/285) | **SPV verifier may accept Merkle proofs for Bitcoin transactions not in the chain** | Mainnet inclusion/ SPV soundness ambiguity blocks bridge or proof-system adoption. |
| [BitVM #376](https://github.com/BitVM/BitVM/issues/376) | **[Zellic Audit] Usage of point types with malformed values** | Malformed BN254/curve-point handling is a cryptographic validation gate. |
| [BitVM #415](https://github.com/BitVM/BitVM/issues/415) | **Security Advisory: Self-Hosted Runner Risk** | CI/supply-chain security remains relevant before trusting release artifacts. |

### `garbled-snark-verifier`

| Link | Upstream item | Why it matters to #189 |
|---|---|---|
| [PR #80](https://github.com/BitVM/garbled-snark-verifier/pull/80) | **G1/G2 Subgroup Checks** | Subgroup validation is a direct verifier soundness gate. |
| [Issue #76](https://github.com/BitVM/garbled-snark-verifier/issues/76) | **Validate data committed onchain before groth16-verify** | On-chain commitment binding must precede proof acceptance. |
| [Issue #82](https://github.com/BitVM/garbled-snark-verifier/issues/82) | **Gate hash may be unsuitable** | Hash-domain and binding choices affect circuit integrity. |
| [Issue #87](https://github.com/BitVM/garbled-snark-verifier/issues/87) | **Garbling table hash may be unsuitable** | Garbling-table integrity is a protocol-critical input. |
| [Issue #43](https://github.com/BitVM/garbled-snark-verifier/issues/43) | **ARM64 support** | The target environment and reproducible-build path need an explicit architecture decision before adoption. |

### GOAT reference implementation

| Link | Upstream item | Why it matters to #189 |
|---|---|---|
| [GOAT `bitvm2-gc` PR #48](https://github.com/GOATNetwork/bitvm2-gc/pull/48) | **bump artifact version** | Artifact/release reproducibility remains unresolved for a resource-heavy reference implementation. |

### FairgateLabs client/protocol ecosystem

| Link | Upstream item | Why it matters to #189 |
|---|---|---|
| [Client #325](https://github.com/FairgateLabs/rust-bitvmx-client/issues/325) | **Support counterproofs in the Union protocols and Union client** | Dispute completeness is not settled. |
| [Client #331](https://github.com/FairgateLabs/rust-bitvmx-client/issues/331) | **Dispute core and dispute channel slot ID verification** | State/channel binding remains an open protocol concern. |
| [Client #317](https://github.com/FairgateLabs/rust-bitvmx-client/issues/317) | **StopOperatorWon SPV signaling** | SPV and terminal-state signaling need explicit verification. |
| [Client #316](https://github.com/FairgateLabs/rust-bitvmx-client/issues/316) | **InputNotRevealed SPV signaling** | Input availability and challenge signaling remain open. |
| [Client #321](https://github.com/FairgateLabs/rust-bitvmx-client/issues/321) | **Self-disabler directory** | Disablement and safety recovery are not complete. |
| [Client #322](https://github.com/FairgateLabs/rust-bitvmx-client/issues/322) | **Full disablement on Bitcoin** | Bitcoin-side fail-safe execution remains open. |
| [Client #336](https://github.com/FairgateLabs/rust-bitvmx-client/issues/336) | **DOS sending `OP Disabler Tx`** | Denial-of-service resilience affects protocol operations. |
| [Client #338](https://github.com/FairgateLabs/rust-bitvmx-client/issues/338) | **Update operator claim transactions with aggregate take key** | Key/claim semantics are still evolving. |
| [Client #339](https://github.com/FairgateLabs/rust-bitvmx-client/issues/339) | **Add speed up to disabler transactions** | Operational transaction handling is incomplete. |

## 7. Conxian cross-repository classifications and durable triage

### Gateway and issue #189

| Area | Current verified classification | Durable reference |
|---|---|---|
| Gateway main | `internal/engine/src/bitcoin/groth16_verifier.rs` defines a versioned, backend-neutral BN254 envelope whose circuit, verification-key, public-input, witness-commitment, and block-context fields are bound by the canonical contract. `MockGroth16Verifier` performs no pairings. | [PR #255](https://github.com/Conxian/conxian-gateway/pull/255) merged; [Groth16 contract](https://github.com/Conxian/conxian-gateway/blob/main/docs/GROTH16_VERIFIER_CONTRACT.md) |
| Gateway PR #278 | The generic BitVM route now propagates typed `VerifierUnavailable` and returns HTTP `501` with an unsupported/non-authoritative response instead of treating metadata as cryptographic verification. The implementation was merged at [`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b); the Phase 4 documentation commit is a post-merge branch update carried by a separate follow-up PR. | [PR #278](https://github.com/Conxian/conxian-gateway/pull/278), implementation commit [`114b857ed9d400beaf474cb68e7ac5f25ef58d78`](https://github.com/Conxian/conxian-gateway/commit/114b857ed9d400beaf474cb68e7ac5f25ef58d78), docs commit [`e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005`](https://github.com/Conxian/conxian-gateway/commit/e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005) |
| BitVM adapter | `internal/engine/src/bitcoin/bitvm_adapter.rs` parses/validates the envelope and delegates to an injected verifier. The legacy `ChainAdapter::verify_state_proof` path is explicitly fail-closed with `VerifierUnavailable`; it is not a metadata-verification path. | [PR #255](https://github.com/Conxian/conxian-gateway/pull/255), [PR #278](https://github.com/Conxian/conxian-gateway/pull/278) |
| BitVMX evaluator | `tools/bitvmx-eval/` is a feature-gated, isolated BitVMX-CPU subprocess evaluator with research-only contract tests. | [PR #259](https://github.com/Conxian/conxian-gateway/pull/259) |
| Universal verifier | `internal/compliance/src/verifier.rs` performs generic adapter dispatch; no production BitVM3, BitVMX-GC, recursive-SNARK, or pairing backend is wired. | [PR #267](https://github.com/Conxian/conxian-gateway/pull/267) |
| Gateway issue | #189 is open and remains research-only. | [Issue #189](https://github.com/Conxian/conxian-gateway/issues/189) |
| Related Gateway state | #216 Babylon header-chain/SPV implementation is merged in PR #253; #219 boundary/handoff is merged in PR #255. Neither implies a BitVM3/GC backend or production pairing backend. | [PR #253](https://github.com/Conxian/conxian-gateway/pull/253), [PR #255](https://github.com/Conxian/conxian-gateway/pull/255) |

### Linked Conxian repositories

> **Issue-state refresh — 2026-07-22:** Platform #1187 and Nexus #169 remain
> open. Wallet #427, `.github` #41, and Core #188 are closed. Enclave #202
> remains open. These state changes update the durable triage references only;
> they do not change the research conclusions or readiness gates below.

| Repository | Current source classification | Durable triage issue |
|---|---|---|
| [`conxius-platform`](https://github.com/Conxian/conxius-platform) | `services/admin-dashboard/src/lib/support/bitvm3.ts`, `bitvm.ts`, and `zkcp.ts` are simulations/scaffolds; length-only or unconditional checks on default paths can produce success-shaped results without cryptographic verification. | [Platform #1187](https://github.com/Conxian/conxius-platform/issues/1187) — open P0 |
| [`lib-conxian-core`](https://github.com/Conxian/lib-conxian-core) | `src/verifier.rs` and verifier architecture docs provide structural/protocol boundaries and fail-closed policy. No current BitVM2 Groth16 verification call is established; Arkworks dependencies alone are not evidence. | [Core #188](https://github.com/Conxian/lib-conxian-core/issues/188) — **closed 2026-07-22**; retain as completed remediation evidence |
| [`conxian-nexus`](https://github.com/Conxian/conxian-nexus) | The current default branch has a real narrow `ark_groth16::Groth16::<Bls12_381>::verify(...)` call with a caller-supplied verification key, but it lacks the Gateway's canonical circuit/key/root-binding semantics, state roots are not bound by that path, negative coverage is incomplete, and trial metadata/ownership/revision drift remains. This path is not interoperable with the Gateway BN254 envelope. | [Nexus #169](https://github.com/Conxian/conxian-nexus/issues/169) — open P1 |
| [`conxius-wallet`](https://github.com/Conxian/conxius-wallet) | TypeScript/Android BitVM paths generate simulation segments and success-shaped results; release guards reduce risk but no actual verifier is present. | [Wallet #427](https://github.com/Conxian/conxius-wallet/issues/427) — **closed 2026-07-22**; retain as completed remediation evidence |
| [`conxius-enclave-sdk`](https://github.com/Conxian/conxius-enclave-sdk) | Production proof routes are explicitly unavailable/fail-closed and bind policy/replay context; generic MuSig2 signing is not SNARK verification, and no BitVM/Groth16 backend is supplied. | [Enclave #202](https://github.com/Conxian/conxius-enclave-sdk/issues/202) — open P0 acceptance gate |
| [`.github`](https://github.com/Conxian/.github) | Organization documentation contains mixed readiness language, including upstream/reference claims that must not be presented as Conxian production evidence. | [`.github` #41](https://github.com/Conxian/.github/issues/41) — **closed 2026-07-22**; retain as completed documentation evidence |

These classifications intentionally separate a real local Arkworks call in Nexus, structural boundaries in Core/Gateway, simulation paths in Platform/Wallet, and fail-closed unsupported paths in Enclave. None is evidence of a production BitVM3 or BitVMX-GC deployment.

### Cross-repository contract mismatch and ownership decision

The following compatibility decisions remain open before any cryptographic
backend is selected. These are Conxian assessments based on the linked source
and issue evidence, not claims that any upstream repository is production-ready.

| Contract surface | Verified current evidence | Compatibility impact / decision required | Tracker |
|---|---|---|---|
| Canonical envelope and errors | Gateway uses a versioned BN254 envelope bound to circuit, VK, public inputs, witness commitment, and block context. PR #278 makes the generic route return typed unavailable/HTTP 501 rather than a false success. | Preserve the BN254 contract and stable fail-closed errors as the only Gateway presentation; do not accept a caller-supplied curve/VK shape as interoperable by implication. | [Gateway #189](https://github.com/Conxian/conxian-gateway/issues/189), [PR #278](https://github.com/Conxian/conxian-gateway/pull/278) |
| Cryptographic backend | Gateway has no production pairing backend. Nexus currently verifies `Bls12_381` with a caller-supplied VK and lacks Gateway root/circuit binding. | Choose an owner and exact backend/curve contract; the Nexus default path cannot be wired directly to the Gateway BN254 envelope. | [Nexus #169](https://github.com/Conxian/conxian-nexus/issues/169), [Gateway #189](https://github.com/Conxian/conxian-gateway/issues/189) |
| VK/circuit registry | Gateway binds circuit and verification-key identifiers but no cross-repository registry or lifecycle owner is established. | Define registry ownership, revision pinning, key distribution, rotation, and circuit-schema compatibility before backend integration. | [Gateway #189](https://github.com/Conxian/conxian-gateway/issues/189), [Nexus #169](https://github.com/Conxian/conxian-nexus/issues/169) |
| Chain observation | The envelope carries block context, but observation, reorg/finality, and inclusion evidence are not owned by the proof backend contract. | Assign chain observation and finality provenance, then define how it is bound to proof acceptance without duplicating node logic across repositories. | [Nexus #169](https://github.com/Conxian/conxian-nexus/issues/169), [Gateway #189](https://github.com/Conxian/conxian-gateway/issues/189) |
| Enclave attestation/capability policy | Enclave production proof routes are unavailable/fail-closed and bind policy/replay context, but do not implement BitVM/Groth16 verification. | Decide whether an enclave attestation is required, which capability is attested, and how policy/replay evidence maps to the Gateway error and acceptance contract. | [Enclave #202](https://github.com/Conxian/conxius-enclave-sdk/issues/202) |
| Client presentation | Platform simulation paths can return success from length-only/unconditional checks; Wallet #427 is closed as a remediation record, but no actual verifier is present in those client paths. | Define the client-facing distinction between verified, unavailable, simulated, and rejected; no success-shaped result may authorize value-bearing behavior. | [Platform #1187](https://github.com/Conxian/conxius-platform/issues/1187), [Wallet #427](https://github.com/Conxian/conxius-wallet/issues/427) |

The ownership decision therefore remains unresolved across the canonical
envelope/errors, cryptographic backend, VK/circuit registry, chain observation,
enclave attestation/capability policy, and client presentation surfaces.

## 8. Explicit claim corrections

| Claim to correct | Correct statement | Evidence |
|---|---|---|
| “BitVM3 is recursive Groth16 migration work.” | BitVM3 is a garbled-circuit bridge/core research family. A Groth16 verifier may appear as a circuit component; that is not recursive Groth16 verification. | [ePrint 2026/933](https://eprint.iacr.org/2026/933), [BitVM3 PDF](https://bitvm.org/bitvm3.pdf) |
| “The BitVM Rust repository proves a mainnet bridge.” | The official repository warns not to use it in production; its public demo graph is BitVM signet/`bitvmnet`. | [BitVM README](https://github.com/BitVM/BitVM), [official demo](https://github.com/BitVM/bitvm.github.io/blob/main/demo/README.md) |
| “No BitVMX mainnet proof exists.” | An upstream BitVMX article links a Bitcoin mainnet transaction for an interactive SNARK-verifier prototype. It must be labeled prototype evidence and not conflated with BitVM3-GC or production bridging. | [BitVMX article](https://bitvmx.org/knowledge/a-new-era-for-bitcoin-successful-snark-proof-verification-with-bitvmx), [transaction](https://mempool.space/tx/75eb2ad4f22263440fc4ceb61c51b0bb77721661dbfbec961358520b04107ec3) |
| “The BitVMX GC article is unavailable.” | The current article is under `/knowledge/implementing-garbled-circuits-for-bitvmx`; the prior `/blog/...` URL is stale. | [Current article](https://bitvmx.org/knowledge/implementing-garbled-circuits-for-bitvmx) |
| “BitVMX-CPU is MIT.” | Repository metadata and `LICENSE` identify Apache-2.0 while README says MIT. Keep the contradiction unresolved until upstream clarifies it. | [Repository](https://github.com/FairgateLabs/BitVMX-CPU), [`LICENSE`](https://github.com/FairgateLabs/BitVMX-CPU/blob/main/LICENSE), [README](https://github.com/FairgateLabs/BitVMX-CPU/blob/main/README.md) |
| “GOAT `bitvm2-gc` is ready to integrate.” | It is public research/reference source with no verified release/license artifact; its approximately 10.4B-gate and 51–374 GB figures are upstream-reported and resource-gating. | [GOAT repository](https://github.com/GOATNetwork/bitvm2-gc), [PR #48](https://github.com/GOATNetwork/bitvm2-gc/pull/48) |
| “Garbled SNARK verifier is a stable dependency.” | It is a GPL-3.0 reference implementation with no verified stable release and unresolved subgroup, commitment, and hash issues. | [Repository](https://github.com/BitVM/garbled-snark-verifier), [PR #80](https://github.com/BitVM/garbled-snark-verifier/pull/80), [issues #76/#82/#87](https://github.com/BitVM/garbled-snark-verifier/issues/76) |
| “Union Bridge is production mainnet evidence.” | Upstream classifies Union Bridge as Rootstock testnet/experimental; V1.5 dispute mechanisms are inactive, no formal audit exists, and mainnet is a 2027 roadmap item. | [Union Bridge article](https://bitvmx.org/knowledge/union-bridge-reaches-testnet-a-milestone-for-bitvmx-powered-bitcoin-bridging) |

## 9. Promotion and readiness gates

### Phase 4 candidate scorecard

This is a bounded triage score, not a probability, security rating, or approval.
Each dimension is scored from 0 (no usable evidence for the Gateway role) to 5
(strong evidence for this specific role): canonical BN254/envelope fit, stable
API/release maturity, security/operational evidence, and license/reproducibility.
The local boundary row is a guardrail baseline, not a cryptographic backend.

| Candidate/evidence track | BN254/envelope fit | API/release maturity | Security/ops evidence | License/reproducibility | Score /20 | Decision |
|---|---:|---:|---:|---:|---:|---|
| Gateway canonical boundary + PR #278 guardrail ([contract](https://github.com/Conxian/conxian-gateway/blob/main/docs/GROTH16_VERIFIER_CONTRACT.md), [PR #278](https://github.com/Conxian/conxian-gateway/pull/278)) | 5 | 4 | 3 | 4 | **16** | Boundary and fail-closed guardrail only; no backend. |
| Nexus `Bls12_381` verifier path ([source](https://github.com/Conxian/conxian-nexus/blob/main/src/executor/bitvm.rs), [#169](https://github.com/Conxian/conxian-nexus/issues/169)) | 0 | 2 | 1 | 3 | **6** | Not interoperable with Gateway BN254; redesign/ownership required. |
| BitVMX-CPU pinned evaluator ([`d390832`](https://github.com/FairgateLabs/BitVMX-CPU/tree/d390832c8e0f2a01453e8ef4bf65dbe715fb9236)) | 0 | 2 | 1 | 1 | **4** | Isolated CPU evaluator only; not GC or Groth16 verification. |
| BitVMX-GC platform/article ([platform](https://bitvmx.org/platform), [article](https://bitvmx.org/knowledge/implementing-garbled-circuits-for-bitvmx)) | 1 | 0 | 0 | 0 | **1** | Roadmap/design evidence; no stable SDK or release. |
| `garbled-snark-verifier` 0.5.0 ([crate](https://crates.io/crates/garbled-snark-verifier/0.5.0), [issues](https://github.com/BitVM/garbled-snark-verifier/issues)) | 2 | 2 | 1 | 0 | **5** | GPL reference implementation with open subgroup/commitment/hash/ARM64 gates. |
| GOAT `bitvm2-gc` ([repository](https://github.com/GOATNetwork/bitvm2-gc), [PR #48](https://github.com/GOATNetwork/bitvm2-gc/pull/48)) | 2 | 1 | 0 | 1 | **4** | Upstream-reported resource burden plus unresolved release/license/reproducibility. |
| Union Bridge ([testnet article](https://bitvmx.org/knowledge/union-bridge-reaches-testnet-a-milestone-for-bitvmx-powered-bitcoin-bridging)) | 1 | 1 | 0 | 1 | **3** | Rootstock Testnet/experimental; inactive dispute mechanisms and no formal audit. |

### Readiness decision

No external candidate satisfies the full promotion gate set. The local
boundary's `16/20` score describes contract clarity and fail-closed behavior,
not cryptographic readiness. PR #278 is therefore a compatibility and safety
hardening change only; its implementation was merged externally, while the
Phase 4 documentation commit is a post-merge branch update. It does not
resolve #189. The next implementation proposal must first settle the six
ownership surfaces above, then provide a pinned backend, registry, vectors,
resource report, protocol/finality evidence, enclave policy, independent
review, and client presentation contract.

No candidate may move from research to production integration until every gate below is satisfied for the exact revision and deployment role:

1. **Scope and terminology:** the candidate is explicitly classified as BitVM2, BitVM3, BitVMX-CPU, BitVMX-GC, Groth16, recursive SNARK/IVC, or another protocol; no category is inferred from branding.
2. **Stable revision/API:** a maintained exact commit or release provides a documented API, compatibility policy, reproducible source archive, and ownership.
3. **License:** repository metadata, checked-in license files, transitive dependencies, and redistribution terms are internally consistent and approved. The BitVMX-CPU Apache/MIT contradiction must be resolved before vendoring.
4. **Reproducible build:** an independent clean builder reproduces the binary/library and verifies source, toolchain, feature, and artifact hashes.
5. **Proof/key contract:** curve, proof encoding, verification-key format, circuit/schema ID, public-input order, transcript/hash domain, state-root binding, and version negotiation are specified and tested.
6. **Independent vectors:** positive, mutation, malformed-envelope, wrong-key, wrong-circuit, wrong-input, subgroup, commitment, and network-context vectors are generated independently and fail closed.
7. **Protocol/economic review:** challenge windows, SPV inclusion, disablement, dispute/counterproof paths, operator/verifier incentives, and recovery behavior are verified on the intended network.
8. **Resource fit:** wall time, CPU, peak RSS, artifact/key/proof sizes, transaction count/weight, and bandwidth fit the deployment with margin. Upstream 51–374 GB reports do not satisfy ordinary CI assumptions.
9. **Isolation and operations:** process descendants, files, network access, timeouts, output sizes, secrets, and rollback behavior are bounded and enforced.
10. **Independent security review:** cryptographic, protocol, economic, supply-chain, and operational review covers the exact implementation and trust assumptions; a paper or demo is not an audit.
11. **Conxian ownership:** Gateway/Core/Nexus/Platform/Wallet/Enclave ownership and issue acceptance are aligned, with no simulation or mock success path reachable from a value-bearing production flow.

## 10. Disposition and follow-up

### Current disposition

- Keep [Gateway #189](https://github.com/Conxian/conxian-gateway/issues/189) **open and research-only**.
- Keep `tools/bitvmx-eval/` isolated and evaluation-only.
- Keep `MockGroth16Verifier` fixture-only and the production Groth16 backend unwired.
- Do not add BitVM3/GC dependencies, production HTTP routes, settlement authorization, or compliance decisions from any upstream paper/demo/transaction.
- [PR #278](https://github.com/Conxian/conxian-gateway/pull/278)'s fail-closed
  implementation was merged externally at
  [`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b).
  The Phase 4 documentation commit is the post-merge branch update
  [`e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005`](https://github.com/Conxian/conxian-gateway/commit/e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005),
  not part of merged `main`; neither resolves [Gateway #189](https://github.com/Conxian/conxian-gateway/issues/189).
- Track the six durable Conxian remediation references with their current
  states: [Platform #1187](https://github.com/Conxian/conxius-platform/issues/1187)
  and [Nexus #169](https://github.com/Conxian/conxian-nexus/issues/169) remain
  open; [Wallet #427](https://github.com/Conxian/conxius-wallet/issues/427),
  [`.github` #41](https://github.com/Conxian/.github/issues/41), and
  [Core #188](https://github.com/Conxian/lib-conxian-core/issues/188) are
  closed; [Enclave #202](https://github.com/Conxian/conxius-enclave-sdk/issues/202)
  remains open. Closed issues remain evidence links, not open readiness gates.

### Next refresh triggers

Re-open implementation scoping only after a public candidate supplies a stable revision/API, reconciled license, reproducible build, independent vectors, resource report, complete dispute/SPV semantics, independent security review, and an explicit Conxian owner. Until then, the correct status is research monitoring rather than an implementation backlog commitment.

## Review metadata

- Phase 1/2 research base verified before PR #278: Gateway `main` at [`d7032ab621ad038f247566f820ac664a6c8c071c`](https://github.com/Conxian/conxian-gateway/commit/d7032ab621ad038f247566f820ac664a6c8c071c).
- The earlier [`6838d872513b681cf88f07fc5431f02b856b6d0e`](https://github.com/Conxian/conxian-gateway/commit/6838d872513b681cf88f07fc5431f02b856b6d0e) and [`4a0433ad92b83bb59d69cb64f86128c1e0212a8e`](https://github.com/Conxian/conxian-gateway/commit/4a0433ad92b83bb59d69cb64f86128c1e0212a8e) bases remain historical PR #268 evidence-chain metadata, not the Phase 4 implementation base.
- PR #278 implementation commit: [`114b857ed9d400beaf474cb68e7ac5f25ef58d78`](https://github.com/Conxian/conxian-gateway/commit/114b857ed9d400beaf474cb68e7ac5f25ef58d78); pre-documentation branch head: [`c893cbb39ea9d680b229a89035ab38f29ed51b8b`](https://github.com/Conxian/conxian-gateway/commit/c893cbb39ea9d680b229a89035ab38f29ed51b8b).
- GitHub reports PR #278 merged externally at 2026-07-22T19:57:47Z as [`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b); Charlie did not merge it. The Phase 4 docs commit [`e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005`](https://github.com/Conxian/conxian-gateway/commit/e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005) was pushed at 2026-07-22T20:07:46Z and is not in merged `main`.
- Gateway evidence merged in PRs [#253](https://github.com/Conxian/conxian-gateway/pull/253), [#255](https://github.com/Conxian/conxian-gateway/pull/255), [#259](https://github.com/Conxian/conxian-gateway/pull/259), and [#267](https://github.com/Conxian/conxian-gateway/pull/267).
- Upstream source and issue metadata were refreshed on 2026-07-22. Upstream-reported claims remain labeled and were not converted into Conxian benchmarks or security conclusions.
