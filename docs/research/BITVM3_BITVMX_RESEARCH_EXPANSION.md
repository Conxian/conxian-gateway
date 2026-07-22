# BitVM3 and BitVMX Research Expansion

> **Research / Evaluation Only** — access date: 2026-07-21

This document is the dated evidence record for GitHub issue [#189](https://github.com/Conxian/conxian-gateway/issues/189). It does not authorize production integration, settlement, compliance, custody, or routing decisions. It records upstream claims separately from facts verified in this repository.

## 1. Decision summary

- **BitVM3 remains a paper/protocol research topic.** The reviewed BitVM3 sources describe a garbled-circuit-based off-chain verification construction and bridge design. They do not provide a Conxian dependency or a stable Rust SDK. The BitVM3 paper uses a Groth16 verifier as a circuit in its construction; that is not the same as recursive Groth16 verification.
- **BitVM2 and Groth16 are related but distinct.** BitVM2 is a permissionless challenge/verification protocol. Its design uses a SNARK verifier such as Groth16 as an example computation. A Groth16 verifier backend is a separate cryptographic component.
- **BitVMX-CPU is the only current issue-specific evaluation target.** The merged `tools/bitvmx-eval/` lane invokes an externally built BitVMX-CPU binary at the exact local pin `d390832c8e0f2a01453e8ef4bf65dbe715fb9236`. It is not BitVM3, BitVMX-GC, garbled-circuit verification, or Groth16 verification.
- **BitVMX-GC is not an integration target yet.** The BitVMX platform has public design material and describes GC/DV-SNARK support as coming in 2026, but no stable public API, release, or reproducible integration target was verified on 2026-07-21. The requested BitVMX GC article URL returned HTTP 404 on that date.
- **GOATNetwork/`bitvm2-gc` is a research/reference implementation.** Its README reports large Groth16/DV-SNARK garbled-circuit resource requirements, but the values are upstream-reported and were not reproduced here. It has no GitHub release, no checked-in license file, and is not an ordinary CI dependency target.
- **Recursive SNARK/IVC is a separate research family.** Microsoft Nova is an IVC/recursive-SNARK library. It is not BitVM3, BitVMX, or a current Conxian dependency. It may inform future proof-system comparisons only.

The default decision is therefore **continue research monitoring; do not add a production BitVM3/GC adapter or settlement path**.

## 2. Scope and non-goals

This expansion covers evidence collection, terminology, resource feasibility, reproducibility requirements, and decision gates for #189. It does not:

- add Rust or TypeScript dependencies;
- add a production verifier, bridge, settlement adapter, compliance route, or HTTP endpoint;
- treat evaluator output as a proof, attestation, settlement authorization, or compliance decision;
- claim that a research paper, website statement, benchmark, or mock verifier is a security review;
- replace the existing BitVMX-CPU evaluator contract or the Groth16 boundary contract.

## 3. Distinctions that must remain explicit

| Subject | Evidence-backed description | Boundary for Conxian |
|---|---|---|
| **BitVM3** | The BitVM3 paper/protocol family describes `BitVM3-core`, garbled-circuit-based off-chain computation, and a bridge construction. The ePrint record is a research paper with a revised version dated 2026-06-08. | Research only. Do not describe it as a Rust SDK, a recursive Groth16 system, or a Conxian dependency. |
| **BitVM2** | The BitVM2 design page describes permissionless runtime challenges and uses a SNARK verifier, such as Groth16, as an example of a computation that can be verified. | A protocol design and a possible verification role; not proof that Conxian has a production Groth16 backend. |
| **Groth16 verifier role** | A Groth16 verifier checks a proof against a verification key and public inputs. In BitVM-style designs it can be the computation being verified or compiled into a larger verification construction. | `Groth16Verifier` is a backend-neutral interface. The checked-in mock matches deterministic fixtures and performs no pairings. |
| **BitVMX-CPU** | The BitVMX-CPU repository provides a Rust/RISC-V emulator and related trace, checkpoint, ROM-commitment, and instruction-mapping commands. Its README says the project is under development, unaudited, and not production-ready. | Isolated evaluator only. It does not implement BitVM3, garbled circuits, Groth16, or recursive proof verification. |
| **BitVMX-GC** | The BitVMX platform describes a GC/DV-SNARK verification plug-in and says support is coming in 2026. The design material is not a stable public integration API. | Monitor only until a public revision/release, license position, build instructions, vectors, and resource evidence are available. |
| **GOAT `bitvm2-gc`** | The repository README describes Groth16 and DV-SNARK verifier circuits and publishes benchmark figures. The inspected repository has no release and no checked-in license file. | Research/reference material. Do not vendor, wire into CI, or infer production suitability from the README. |
| **Recursive SNARK / IVC** | Nova is a distinct folding-based recursive SNARK/IVC library with its own curve cycles, commitment schemes, and setup requirements. | Separate comparison track. Do not substitute Nova for BitVM3, BitVMX-GC, or a Groth16 backend without a separate design and security review. |

## 4. Evidence matrix

Evidence status means: **verified** = directly observed in the linked source or exact local revision; **upstream-reported** = a source states the value or capability but it was not independently reproduced; **inferred** = a bounded interpretation of verified evidence; **unknown** = unavailable or unresolved.

| Source | Access / exact ref | Evidence | Status / confidence | Release or API maturity | Licensing signal | Material resource information |
|---|---|---|---|---|---|---|
| [BitVM3 paper](https://bitvm.org/bitvm3.pdf) | Accessed 2026-07-21; retrieved PDF SHA-256 `1f446acde8fdacd5c622ee079b85cc6dd0b7eb98743a4c0593a841a288247cf8`; 29 pages; no release/tag | Title and paper content identify BitVM3 as “Efficient Bitcoin Bridges via Garbled Circuits”; the paper separates `BitVM3-core` from the bridge construction. | **verified**, high for document identity; **upstream-reported**, medium for performance/security claims | Paper only; no SDK, crate, or API release identified | PDF does not establish a software license; use the ePrint record for the paper license | Paper includes prototype and transaction-cost evaluation; values are not Conxian benchmarks. |
| [Cryptology ePrint 2026/933](https://eprint.iacr.org/2026/933) | Accessed 2026-07-21; received 2026-05-11; revised 2026-06-08; ePrint page lists CC BY | Abstract describes `BitVM3-bridge`, `BitVM3-core`, garbled circuits, a Bitcoin light client, and claimed cost reductions. | **verified**, high for metadata; **upstream-reported**, medium for claims | Research paper; no software API or release | CC BY for the paper record; this does not license an implementation | Reported figures such as on-chain cost and prototype circuit measurements require independent reproduction before engineering use. |
| [BitVM2 design](https://bitvm.org/bitvm2) | Accessed 2026-07-21; web page, no release/tag | Describes permissionless runtime challenges, a one-time 1-of-n setup, and a SNARK verifier such as Groth16 as an example computation. | **verified**, high for page content; **upstream-reported**, medium for feasibility estimates | Protocol design page; no SDK or versioned API | No software license stated on the page | The page discusses script and trace-size trade-offs; these are design notes, not current Conxian capacity measurements. |
| [SNARK verifier in Bitcoin Script](https://bitvm.org/snark.html) | Accessed 2026-07-21; web page, no release/tag | Provides a design plan for Groth16/FFlonk-style verification in Bitcoin Script and lists circuit/field-operation constraints. | **verified**, high for page content; **upstream-reported**, medium for estimates | Design note; no implementation release identified | No software license stated on the page | Includes upstream estimates for proof size, script sizing, and large computation chunking; none were reproduced here. |
| [BitVMX platform](https://bitvmx.org/platform) | Accessed 2026-07-21; current site page | Describes BitVMX-CPU as an available verification protocol and BitVMX-GC as a GC/DV-SNARK plug-in “coming in 2026.” | **verified**, high for page text; **upstream-reported**, low-to-medium for roadmap/resource claims | Public platform description; no stable GC API/release verified | Site page does not establish a GC implementation license | The page reports garbling tables on the order of hundreds of megabytes; treat this as an upstream claim, not a sizing commitment. |
| [BitVMX GC article URL](https://bitvmx.org/blog/implementing-garbled-circuits-for-bitvmx) | Accessed 2026-07-21; HTTP 404 response | The requested article was not available at the supplied URL on the access date. | **verified**, high for availability; substantive evidence **unknown** | No usable public article/API at this URL | Unknown | No resource claim taken from the unavailable page. |
| [BitVMX whitepaper](https://bitvmx.org/files/bitvmx-whitepaper.pdf) | Accessed 2026-07-21; retrieved PDF SHA-256 `ea111dc785c5df1cd1019252a73f9a84ba9dc83e8c04857fac2780593b89d958`; 13 pages | “BitVMX: A CPU for Universal Computation on Bitcoin” describes a virtual CPU, trace hash chains, and challenge-response verification games. | **verified**, high for document identity; **upstream-reported**, medium for protocol properties | Whitepaper; no GC release/API | PDF does not establish a software license | Describes trade-offs among transaction cost, round complexity, prover cost, verifier cost, and precomputation; it is not a benchmark for the current evaluator. |
| [FairgateLabs/BitVMX-CPU at `d390832`](https://github.com/FairgateLabs/BitVMX-CPU/tree/d390832c8e0f2a01453e8ef4bf65dbe715fb9236) and [README](https://github.com/FairgateLabs/BitVMX-CPU/blob/d390832c8e0f2a01453e8ef4bf65dbe715fb9236/README.md) | Default `main` commit `d390832c8e0f2a01453e8ef4bf65dbe715fb9236`; tag `v0.7.0` resolves to the same commit; tag `v0.8.0` resolves to `e23fbfccb0b50b52c882e6ba4f57eba3b7c3887f`; GitHub latest-release endpoint returned `v0.5.11`; accessed 2026-07-21 | README documents a Rust emulator, RISC-V execution, traces, checkpoints, ROM commitments, and failure-injection commands. It also says the project is under development, unaudited, and not production-ready. | **verified**, high for revision/content; **upstream-reported**, high for README maturity disclaimer | Runnable CLI/repository with tags; release metadata is inconsistent with the newer tags; local evaluator intentionally pins `d390832` rather than claiming `v0.8.0` | [Repository metadata reports Apache-2.0](https://github.com/FairgateLabs/BitVMX-CPU); checked-in [`LICENSE`](https://github.com/FairgateLabs/BitVMX-CPU/blob/d390832c8e0f2a01453e8ef4bf65dbe715fb9236) is Apache-2.0, while the pinned README says MIT. Contradiction unresolved. | No independent resource benchmark was run in this research expansion. |
| [GOATNetwork/bitvm2-gc at `da8c5bd`](https://github.com/GOATNetwork/bitvm2-gc/tree/da8c5bdcfdc5f61ddd5e4aa62d64183ee8dcb7f1) and [README](https://github.com/GOATNetwork/bitvm2-gc/blob/da8c5bdcfdc5f61ddd5e4aa62d64183ee8dcb7f1/README.md) | Default `main` commit `da8c5bdcfdc5f61ddd5e4aa62d64183ee8dcb7f1`; accessed 2026-07-21; no tag or GitHub release | README describes Groth16 and DV-SNARK verifier circuits and reports a `10,398,026,901`-gate Groth16 circuit, `51G` peak memory for one program, and `374G` peak memory for the DV-SNARK row. | **verified**, high for README text; resource values **upstream-reported**, not reproduced | Research/reference repository; no stable release or narrow integration API verified | GitHub repository metadata has no SPDX license and no checked-in license file was found; unresolved | The reported memory figures exceed ordinary CI-class capacity. Do not claim ordinary CI suitability or use the figures as a Conxian benchmark. |
| [Microsoft/Nova at `666e3b2`](https://github.com/microsoft/Nova/tree/666e3b25bfb9f8b2106f8b4d8057010f28b1ee79) and [README](https://github.com/microsoft/Nova/blob/666e3b25bfb9f8b2106f8b4d8057010f28b1ee79/README.md) | Default `main` commit `666e3b25bfb9f8b2106f8b4d8057010f28b1ee79`; accessed 2026-07-21; no GitHub release endpoint result | README describes Nova as a recursive SNARK/IVC library with folding schemes, multiple curve cycles, and optional commitment/compression features. | **verified**, high for repository/document content; **inferred**, medium for its relevance as a comparison track | Active library repository; no release used by Conxian | [MIT license](https://github.com/microsoft/Nova/blob/666e3b25bfb9f8b2106f8b4d8057010f28b1ee79/LICENSE) | README documents setup and file-size considerations for optional KZG/IVC paths; these do not describe BitVM3 or current Conxian resources. |
| [arkworks `groth16` at `3214457`](https://github.com/arkworks-rs/groth16/tree/321445775f3025b924b7aeadfea9e5fe096efdb5) and [README](https://github.com/arkworks-rs/groth16/blob/321445775f3025b924b7aeadfea9e5fe096efdb5/README.md) | Default `master` commit `321445775f3025b924b7aeadfea9e5fe096efdb5`; tag `v0.6.0` resolves to `0bb3e604c534bd118ed477eaf1231f591d6fc40f`; accessed 2026-07-21 | README calls the library an academic proof-of-concept prototype and explicitly says it is not ready for production use. | **verified**, high for repository/document content | Versioned Rust library with tags; no production role inferred | Dual MIT/Apache-2.0 licensing is stated in the README and license files | No resource figure is adopted here; production use would still require independent vectors, review, and deployment-specific benchmarking. |

The matrix records what the sources say. It is not an endorsement, compatibility statement, or security assessment.

## 5. Current Conxian artifacts and boundaries

| Artifact | Verified current role | Explicit non-claim |
|---|---|---|
| [`tools/bitvmx-eval/`](../../tools/bitvmx-eval/) and [`BITVMX_EVAL.md`](./BITVMX_EVAL.md) | Feature-gated, standalone subprocess evaluation of the pinned BitVMX-CPU CLI with bounded reports, exact revision sidecars, artifact limits, and fail-closed parsing. | Synthetic tests are harness-contract tests. The lane does not execute a production BitVMX protocol and never turns an evaluator result into `verified: true`. |
| [`bitvm_adapter.rs`](../../internal/engine/src/bitcoin/bitvm_adapter.rs) | Parses and validates a BitVM Groth16 envelope, checks network and statement constraints, validates circuit/key association through the injected trait, and delegates to an injected backend. | The legacy `ChainAdapter::verify_state_proof` path is explicitly metadata-only. The adapter does not contain a pairing-based Groth16 backend. |
| [`groth16_verifier.rs`](../../internal/engine/src/bitcoin/groth16_verifier.rs) | Defines the BN254 statement/hash contract, public-input/witness-commitment binding, circuit/key association, proof-envelope validation, and backend-neutral `Groth16Verifier`. | `MockGroth16Verifier` records deterministic fixture digests and performs no Groth16 pairings. The boundary is not cryptographic verification. |
| [`UniversalVerifier`](../../internal/compliance/src/verifier.rs) | Delegates generic `verify_state_proof` calls to a chain-keyed `ChainAdapter` map and separately delegates compliance attestations to `CoreVerifier`. | It is not specially wired to a production Groth16 backend, BitVM3, BitVMX-GC, or a recursive SNARK implementation. |
| Current lockfiles and manifests | `Cargo.lock` contains `ark-groth16` `0.5.0`, `risc0-groth16` `5.0.0-rc.1`, and `risc0-zkvm` `5.0.0-rc.1`; `internal/compliance/Cargo.toml` requests `risc0-zkvm = 5.0.0-rc.1`. | These are current repository dependency facts, not claims about upstream latest versions and not a recommendation to upgrade or wire them into #189. |

## 6. Resource and reproducibility assessment

1. **BitVM3 and BitVMX-GC claims are not local measurements.** Paper/site estimates, including circuit size, garbling cost, transaction cost, or bandwidth, remain upstream-reported until the exact source, compiler, workload, hardware, and measurement procedure can be reproduced.
2. **GOAT `bitvm2-gc` is resource-gated.** The inspected README reports approximately 10.4 billion gates and peak memory values of 51G and 374G for listed workloads. Those values are not independently reproduced here and are not evidence that ordinary CI or the current Gateway environment can run the project.
3. **BitVMX-CPU is reproducible only as an isolated external build.** The evaluator records an exact source pin and executable hash, but its synthetic fixtures validate the wrapper contract rather than upstream protocol correctness. Network denial and aggregate process isolation remain caller/environment responsibilities documented by the evaluator.
4. **Proof size is not interchangeable with trace or circuit size.** The CPU evaluator deliberately reports `proof_size_bytes: null`; a trace, checkpoint, garbling table, SNARK proof, verification key, and Bitcoin transaction have different semantics and must not be compared as one metric.
5. **A reproducible future experiment must retain:** exact source revision, license files, compiler/toolchain, feature flags, build command, binary hash, fixture/input hashes, proof/key formats, hardware/OS, wall/CPU/RSS/output metrics, network policy, process limits, and positive/negative results.

## 7. Contradictions and unknowns

- BitVMX-CPU repository metadata and checked-in `LICENSE` identify Apache-2.0, while the pinned README says MIT. This report records the contradiction and does not resolve it speculatively.
- BitVMX has public platform prose for GC and a 2026 target statement, but no stable public GC revision, release, or narrow API was verified. The supplied GC article URL returned 404 on 2026-07-21.
- GOAT `bitvm2-gc` publishes benchmark values without an inspected license file, release artifact, or independent reproduction. Its README is evidence of an upstream report, not a deployment recommendation.
- The BitVM3 PDF and ePrint record were reviewed as separate sources. Their URLs, metadata, and retrieved content were not assumed to be byte-identical.
- Nova, arkworks `groth16`, RISC Zero Groth16, BitVM2, BitVM3, BitVMX-CPU, and BitVMX-GC are not interchangeable merely because they mention SNARKs, Groth16, recursive proofs, or Bitcoin verification.

## 8. Trust boundary

Upstream papers, websites, repositories, benchmarks, and release metadata are untrusted inputs to Conxian engineering decisions until they are pinned, license-reviewed, reproducibly built, independently tested, and security-reviewed. The current evaluator is an isolated observation tool. Its output must not cross into the Gateway's cryptographic verification, settlement, or compliance decision paths. The injected Groth16 interface is a contract boundary; the checked-in mock is fixture-only; `UniversalVerifier` has no special production Groth16 wiring.

## 9. Decision gates before any production proposal

All gates are required; passing one gate does not waive another:

1. **License:** project and dependency licenses are explicit, internally consistent, and approved for the intended distribution model.
2. **Stable revision/release:** an exact public commit or release has a maintained API, reproducible source archive, and documented compatibility policy.
3. **Reproducible build:** an independent builder can reproduce the binary/library and verify hashes from a clean environment.
4. **Independent vectors:** at least two independently generated positive vectors and negative/mutation vectors cover proof, public input, verification key, circuit association, malformed envelope, and boundary failures.
5. **Resource fit:** wall time, CPU, peak RSS, artifact sizes, key sizes, proof sizes, and transaction/bandwidth costs fit the intended deployment with a documented margin. GOAT's upstream 51G/374G figures do not satisfy an ordinary CI assumption.
6. **Process and network isolation:** subprocesses, descendants, files, and network access are bounded and enforced rather than represented by caller-provided markers alone.
7. **Proof and key formats:** proof encoding, verification-key format, curve, circuit identifier, public-input ordering, transcript/hash domains, and version negotiation are specified and independently testable.
8. **Security review:** cryptographic, protocol, economic, operational, and trust-boundary review is complete for the exact implementation and deployment role.

Until all gates pass, keep #189 research-only and keep settlement/compliance decisions out of scope.

## 10. Phased next actions

### Phase 0 — Maintain the evidence record

- Re-check the pinned upstream refs, release metadata, license files, and availability of the BitVMX-GC material on each research refresh.
- Preserve upstream-reported values with their source, access date, exact ref, and confidence; do not convert them into Conxian performance claims.

### Phase 1 — Reproducibility spike, if a public target appears

- Use a fresh isolated workspace for one exact BitVMX-GC or GOAT revision.
- Resolve licensing before vendoring or redistribution.
- Build outside the production workspace, record hashes and toolchains, deny network access, measure process descendants, and abort on resource ceilings.
- Produce independent positive/negative vectors and a resource report before discussing an adapter.

### Phase 2 — Interface-only review

- Compare the candidate proof/key formats with the existing [`Groth16Verifier`](../../internal/engine/src/bitcoin/groth16_verifier.rs) boundary without wiring it to settlement or compliance.
- Decide separately whether a candidate is a Groth16 pairing backend, a BitVM challenge engine, a GC evaluator, a recursive SNARK/IVC system, or only an external reference.

### Phase 3 — Security and promotion decision

- Obtain independent cryptographic and protocol review for the exact candidate and trust assumptions.
- Re-run every decision gate with deployment-specific limits.
- Promote only through a separately approved, feature-gated change with production tests and explicit settlement/compliance ownership. Otherwise retain research monitoring.
