# Conxian Gateway Gap Analysis — 2026-07-22

**Scope:** Current Gateway issue inventory, implementation evidence, research
boundaries, and next acceptance slices for the six issues that were open at the
2026-07-22 Phase 2 audit checkpoint.

**Historical audit snapshot:** `Conxian/conxian-gateway` `origin/main` at
[`764859fd19c6b4305c0b7b9222c71493b3587177`](https://github.com/Conxian/conxian-gateway/commit/764859fd19c6b4305c0b7b9222c71493b3587177).

**Historical Phase 4 implementation context before PR #278:** local continuity verification on
2026-07-22 pulled `origin/main` at
[`d7032ab621ad038f247566f820ac664a6c8c071c`](https://github.com/Conxian/conxian-gateway/commit/d7032ab621ad038f247566f820ac664a6c8c071c)
and created `charlie/issue-245-tracked-mempool-telemetry` directly from that
base. The branch is a reviewable implementation context, not a claim that the
slice is merged into `main`.

**Current merged-main verification:** `origin/main` is at
[`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b),
the external merge commit for PR #278.

**Audit branch:** `charlie/issue-245-audit-2026-07-22`.

**GitHub observation:** 2026-07-22T14:42:43Z; PR #274 merged at
2026-07-22T14:25:01Z. The preceding pre-merge observation is retained in the
status history rather than treated as live.

**Important boundary:** This is a timestamped current-status audit and
acceptance plan. The audit follow-up changes CI/workflow and release-verifier
controls only; it does not change BIP-110 enforcement, fee-model behavior,
cryptographic backends, or production settlement integrations. Earlier dated
gap analyses and sprint reviews remain historical records; they are not silently
rewritten by this document.

**Post-snapshot BitVM Phase 4 note — 2026-07-22:** After the historical audit
snapshot, [PR #278](https://github.com/Conxian/conxian-gateway/pull/278) continued
on `charlie/issue-189-bitvm-fail-closed`. Its fail-closed implementation is
[`114b857ed9d400beaf474cb68e7ac5f25ef58d78`](https://github.com/Conxian/conxian-gateway/commit/114b857ed9d400beaf474cb68e7ac5f25ef58d78);
the branch was at
[`c893cbb39ea9d680b229a89035ab38f29ed51b8b`](https://github.com/Conxian/conxian-gateway/commit/c893cbb39ea9d680b229a89035ab38f29ed51b8b)
before this documentation consolidation. The continuity checkpoint predates
GitHub's subsequent external merge at
2026-07-22T19:57:47Z as
[`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b).
Charlie did not merge it. The Phase 4 documentation commit
[`e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005`](https://github.com/Conxian/conxian-gateway/commit/e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005)
was pushed after that merge and is not part of merged `main`. [Gateway #189](https://github.com/Conxian/conxian-gateway/issues/189)
remains open and research-only. The current open cross-repository acceptance
issues are [Platform #1187](https://github.com/Conxian/conxius-platform/issues/1187),
[Nexus #169](https://github.com/Conxian/conxian-nexus/issues/169), and
[Enclave #202](https://github.com/Conxian/conxius-enclave-sdk/issues/202);
[Wallet #427](https://github.com/Conxian/conxius-wallet/issues/427),
[`.github` #41](https://github.com/Conxian/.github/issues/41), and
[Core #188](https://github.com/Conxian/lib-conxian-core/issues/188) are retained
as closed remediation evidence.
This documentation recovery is carried by a separate follow-up PR; that PR is
pending review/merge and is not part of `main` until it lands.

## Executive outcome

The 2026-07-22T14:42:43Z GitHub snapshot contained exactly six open Gateway
issues:
[#189](https://github.com/Conxian/conxian-gateway/issues/189),
[#220](https://github.com/Conxian/conxian-gateway/issues/220),
[#222](https://github.com/Conxian/conxian-gateway/issues/222),
[#228](https://github.com/Conxian/conxian-gateway/issues/228),
[#245](https://github.com/Conxian/conxian-gateway/issues/245), and
[#247](https://github.com/Conxian/conxian-gateway/issues/247). No Gateway pull
request was open at the checkpoint; recent work through PRs #268, #269, #270,
#271, #272, #273, and [#274](https://github.com/Conxian/conxian-gateway/pull/274)
is merged.

The highest overall triage score is **#222**, because its remaining work is
well-evidenced, mostly self-contained, and directly reduces release and proof
claims risk. The narrow candidate for the current #245 workstream is **status
observability plus Core preflight passthrough**, not a fee multiplier or a model
rewrite. BIP-110 deployment status must never be used as the sole reason to
change a fee or route.

## Current inventory and ranking

| Rank | Issue | Current classification | State at checkpoint | Score |
|---:|---|---|---|---:|
| 1 | [#222](https://github.com/Conxian/conxian-gateway/issues/222) | Implementation-ready; highest overall score | Open; Phase 3 release-governance implementation is prepared on the audit branch, while merge/admin/live-release evidence remains | **88 / 90** |
| 2 | [#245](https://github.com/Conxian/conxian-gateway/issues/245) | Research/observability; narrow preflight-integration candidate | Open; no Gateway BIP-110 integration or fee predictor | **62 / 90** |
| 3 | [#228](https://github.com/Conxian/conxian-gateway/issues/228) | Bounded implementation slices remain | Open; RGB Phase 1 and Phase 2 hardening are merged | **60 / 90** |
| 4 | [#220](https://github.com/Conxian/conxian-gateway/issues/220) | Research milestone with production blockers | Open; isolated DLC evidence is merged, runtime integration is not | **58 / 90** |
| 5 | [#189](https://github.com/Conxian/conxian-gateway/issues/189) | Research milestone with production blockers | Open; BitVM3/GC remains research-only | **55 / 90** |
| 6 | [#247](https://github.com/Conxian/conxian-gateway/issues/247) | Blocked/high-risk pending signer, contract, and governance details | Open; ALEX quote/prepared-payload surfaces exist, execution is not production-ready | **42 / 90** |

### Reproducible scoring rubric

The score is a **triage comparison**, not a probability, delivery estimate, or
security rating. Each factor is assigned an integer from 0 to 5 using the
definitions below; no decimal precision is implied.

| Factor | Weight | `0` | `5` |
|---|---:|---|---|
| Impact | +25 | No meaningful effect on Gateway reliability, safety, or delivery | Directly affects a critical production, release, settlement, or proof boundary |
| Risk reduction | +25 | Does not reduce a material known risk | Removes or contains a high-consequence false-readiness, security, or release risk |
| Readiness | +20 | No bounded owner, evidence, or acceptance slice | Current code/docs and a concrete, reviewable acceptance slice already exist |
| Evidence confidence | +20 | Primarily assertion or unverified projection | Local source plus authoritative external evidence agree at a pinned snapshot |
| External-dependency penalty | −10 | Mostly Gateway-owned and self-contained | Requires unresolved upstream protocol, signer, governance, or cross-repository decisions |

The calculation is:

```text
score = 25 * impact/5
      + 25 * risk_reduction/5
      + 20 * readiness/5
      + 20 * evidence_confidence/5
      - 10 * external_dependency_penalty/5
```

The maximum possible score is 90 because the dependency term is a penalty.
The factor values used for this snapshot are:

| Issue | Impact | Risk reduction | Readiness | Evidence confidence | Dependency penalty | Calculation |
|---|---:|---:|---:|---:|---:|---:|
| #222 | 5 | 5 | 5 | 5 | 1 | 25 + 25 + 20 + 20 − 2 = **88** |
| #245 | 4 | 4 | 3 | 4 | 3 | 20 + 20 + 12 + 16 − 6 = **62** |
| #228 | 4 | 4 | 3 | 4 | 4 | 20 + 20 + 12 + 16 − 8 = **60** |
| #220 | 4 | 4 | 2 | 5 | 5 | 20 + 20 + 8 + 20 − 10 = **58** |
| #189 | 4 | 5 | 1 | 4 | 5 | 20 + 25 + 4 + 16 − 10 = **55** |
| #247 | 4 | 4 | 1 | 2 | 5 | 20 + 20 + 4 + 8 − 10 = **42** |

The ordering is intentionally conservative: high impact does not overcome a
missing cryptographic, signer, governance, or upstream protocol acceptance
gate. The values should be re-scored when a dependency, release, fixture, or
production-owner decision changes.

## Already-satisfied milestones outside the open inventory

These issues are closed and must not be represented as current missing work:

| Issue | Verified milestone | Evidence |
|---|---|---|
| [#216](https://github.com/Conxian/conxian-gateway/issues/216) | Babylon Bitcoin header-chain retrieval and bounded SPV-style verification | [PR #253](https://github.com/Conxian/conxian-gateway/pull/253) merged; current boundary in `internal/engine/src/bitcoin/babylon_adapter.rs` |
| [#219](https://github.com/Conxian/conxian-gateway/issues/219) | Backend-neutral Groth16/BN254 contract, witness-commitment binding, fixture, rejection tests, and BitVM handoff | [PR #255](https://github.com/Conxian/conxian-gateway/pull/255) merged; `internal/engine/src/bitcoin/groth16_verifier.rs`, `internal/engine/src/bitcoin/bitvm_adapter.rs` |
| [#236](https://github.com/Conxian/conxian-gateway/issues/236) | SDK version/documentation correction | Issue closed; `packages/client-sdk/package.json` is `0.1.4`, and the SDK README uses “Developer Preview” language |

These milestones do not imply that Babylon EOTS/finality, a production
Groth16 pairing backend, or SDK production publication is complete.

## Issue-by-issue evidence and next acceptance slice

### #222 — strict CI/CD baseline

**Classification:** Implementation-ready and highest overall score. Phase 3 now
contains the narrow release-governance implementation; the issue should remain
open until the change is merged, the external controls are configured, and a
controlled tag release demonstrates the resulting assets and attestation. A
broad workflow rewrite is not justified.

**Current evidence**

- Existing workflow surfaces are present in `.github/workflows/rust-ci.yml`,
  `node-ci.yml`, `cargo-audit.yml`, `secret-scan.yml`,
  `dependency-review.yml`, `lightning-coverage.yml`, and `release.yml`.
- The Rust and Node workflows contain SHA-pinned actions and the release
  workflow has SBOM/provenance jobs. The scoped Lightning gate is in
  `.github/workflows/lightning-coverage.yml`.
- The current Gateway release surface includes `v0.1.4`; the SDK package is
  `0.1.4`. The old `v0.1.0`/`0.1.4` drift is historical.
- The release workflow now validates that the exact tag commit is reachable
  from the fetched `origin/main` history, then runs direct exact-commit baseline
  jobs for Rust/contamination/verifier tests, the complete Node CI command set,
  `cargo-audit 0.22.2`, Gitleaks `8.30.1` with a pinned official checksum
  manifest, and `cargo-llvm-cov 0.8.7` Lightning coverage. Packaging cannot
  start unless every one of those jobs succeeds.
- The package job builds the production `gateway` binary, creates a clean
  deterministic archive directory, records the full release commit in
  `RELEASE-METADATA.txt`, normalizes the CycloneDX 1.5 SBOM, verifies the exact
  archive/checksum/SBOM set, and uploads it immutably. Attestation and GitHub
  Release publication depend on that package plus the same verifier rerun.
- `RELEASE.md` now documents preflight, artifact verification, the protected
  release environment, the optional crates.io gate, rollback/yank guidance, and
  partial/draft-release recovery.
- `scripts/normalize_release_sbom.py` and
  `scripts/verify_release_artifacts.py` provide deterministic local checks for
  SBOM identity, archive metadata/ELF target, full-commit binding, duplicate or
  unsafe tar members, checksums, and exact release file shape. Focused stdlib
  regression tests cover valid, malformed, duplicate, unsafe, symlink, extra,
  missing, and checksum-mismatch fixtures.
- The shared security-tool versions and refresh procedure are recorded in
  [`docs/CI_TOOLING_PINS.md`](CI_TOOLING_PINS.md); the scheduled workflows and
  release baseline use the same pins.
- GitHub currently exposes one active “Code Quality Copilot review for default
  branch” repository ruleset. That is not evidence that every required CI,
  security, release, and artifact check is merge-required.

**Dependencies:** merging the implementation, repository settings/admin
ownership for required checks and environment protection, exact release
artifact publication semantics, a controlled live tag release, and the existing
external checks. The optional crates.io path also depends on publishable Cargo
package metadata/path-dependency version requirements. No new protocol
dependency is required.

**Next acceptance slice:** review and merge the Phase 3 patch; configure and
verify the `main` required-check ruleset and protected `release` environment;
run one controlled `vX.Y.Z` release and verify its archive, checksum, SBOM, and
attestation; then resolve the separate Cargo package publication prerequisites
before enabling crates.io publication. Branch protection/ruleset state and the
live release rehearsal remain external/admin evidence, not claims made by this
repository patch.

### #245 — BIP-110 routing and fee-market impact

**Classification:** Research/observability with a bounded Phase 4 tracked-state
telemetry slice on the working branch. The issue remains open; no fee
multiplier or fee-model rewrite is justified by the current evidence.

**Current evidence**

- A production-source search found no Gateway BIP-110 integration,
  `estimatesmartfee` passthrough, or fee predictor. Existing fee behavior is
  bounded RBF/CPFP orchestration in
  `internal/engine/src/bitcoin/fee_bump_policy.rs`,
  `internal/engine/src/bitcoin/mempool_orchestrator.rs`, and
  `internal/engine/src/bitcoin/rpc.rs`.
- The current `lib-conxian-core` `main` package is `0.3.0` at commit
  `35432776a05cba6cd11bae9d6258ec7618a3138c`; the latest observed tag is
  `v0.2.11` at `46ae83c739d56df8f4bf52c976b27de8ec5bb91a`. The issue’s
  `v0.2.12` statement is therefore ambiguous/stale and must be replaced by an
  exact commit or release before integration.
- Core’s `src/control_model/bip110.rs`,
  `src/control_model/bip110_preflight.rs`, and
  `docs/BIP110_ALIGNMENT.md` define a versioned, fail-closed byte-size and
  preflight metadata contract. Core does not parse transactions, interpret
  scripts, detect deployment, or prove network consensus activation.
- The latest visible `conxius-enclave-sdk` release is `v2.0.11`; its README
  labels the 2.x line “Beta / conditional” and records P0 evidence gaps. The
  current `Cargo.toml` package version `2.0.12` is not release or production
  support evidence. Enclave issue [#202](https://github.com/Conxian/conxius-enclave-sdk/issues/202)
  remains open; issue #179 and PR #203 are closed/merged BIP-110 signing
  slices.
- The canonical BIP-110 text is marked `Complete`, not `Deployed`; Bitcoin Core
  implementation PRs [#34929](https://github.com/bitcoin/bitcoin/pull/34929)
  and [#34930](https://github.com/bitcoin/bitcoin/pull/34930) are closed without
  a merge. An inspection of stock Bitcoin Core 31.0 at source commit
  `a2e074d66ac17ca7907909bbbb563e77185a45e5` found no `REDUCED_DATA`
  deployment. Current Core documentation does not establish deployment.

**Phase 4 bounded implementation slice (working branch):**

- `GET /api/v1/bitcoin/mempool/telemetry` aggregates the existing persisted
  `PersistentState.mempool_pending_txs` records through the existing Bearer
  authentication boundary.
- The response is schema-versioned and explicitly scoped to
  `gateway_tracked_transactions`; `network_mempool_observation` is
  `not_configured`, and empty means no Gateway-tracked records, not a network
  mempool zero.
- The aggregation reports every current `MempoolTxStatus`, replaceable and
  CPFP-capable totals, the sum of current persisted `bump_attempts`, current
  `last_bump_strategy` observations, and a nullable timestamp derived from
  persisted evaluation/bump fields.
- `/metrics` exposes the same useful aggregates with bounded names and only
  closed status/strategy labels. It emits no txids, addresses, node IDs, route
  IDs, or free-form errors.
- `FilePersistence` now serializes same-process reads and writes through the
  shared backend, uses durable temporary-file plus atomic-rename replacement,
  and cleans up failed temporary writes. The async telemetry handlers offload
  blocking loads with `spawn_blocking`; route tests cover stable 503 failures
  and unavailable metrics without fabricated aggregate zeros.
- Pure aggregation tests cover status/empty semantics, attempt and strategy
  honesty, capability totals, last-updated derivation, deterministic serde, no
  mutation, bounded metric rendering, and authenticated route behavior.

This persistence boundary is deliberately limited to same-process calls on a
shared `FilePersistence` instance (with atomic replacement on the supported
Unix deployment). It is not a multi-process transaction layer and does not
make separate load-modify-save sequences transactional.

This slice does **not** provide a Bitcoin Core or network mempool view, BIP-110
validation or deployment detection, `estimatesmartfee` passthrough, block or
backlog quantiles, route-confidence calibration, historical RBF/CPFP outcome
storage, or a fee predictor. It also does not change fee-bump decisions,
transaction construction, signing, broadcast, Core validation, Wallet fee
recommendation, or Nexus observation.

**Dependencies:** exact Core contract revision, Bitcoin node RPC telemetry,
deployment-state provenance, enclave release acceptance, and privacy-safe
metrics. The proposal/adoption state is external and must remain explicit.

**Remaining acceptance slices:** add deployment/status observability with
explicit `unknown`/`not_deployed`/`active`/`expired` states; pass Core preflight
requests/results through the Gateway without duplicating a script interpreter;
capture node-backed `estimatesmartfee`-style estimates, network/node mempool
and backlog data, block quantiles, and durable RBF/CPFP outcomes; and expose
route confidence with provenance and calibration. Never alter fees solely
because a BIP-110 status value changes.

The detailed evidence matrix, limits, risks, and acceptance metrics are in
[`docs/research/BIP110_FEE_MARKET_AND_ROUTING_2026-07-22.md`](research/BIP110_FEE_MARKET_AND_ROUTING_2026-07-22.md).

### #228 — RGB stash resolver integration

**Classification:** Bounded implementation slices remain. The issue is open;
its recent hardening is not a production-complete issuer-verification claim.

**Current evidence**

- [PR #256](https://github.com/Conxian/conxian-gateway/pull/256) merged the
  native RGB Phase 1.5 hardening. [PR #261](https://github.com/Conxian/conxian-gateway/pull/261)
  added the pinned `rgb-persist-fs::StockpileDir`, wallet-owned auth-token/seal
  registry, consignment boundary, and fail-closed signature policy.
- [PR #262](https://github.com/Conxian/conxian-gateway/pull/262) added staged
  import rollback, filesystem durability/permission hardening, and regression
  coverage for failed imports.
- Current paths are `internal/engine/src/bitcoin/rgb_stash.rs`,
  `rgb_native.rs`, `rgb_adapter.rs`, and `docs/RFC_RGB_ADAPTER.md`. The JSON
  metadata cache is descriptive; the stockpile is the consensus boundary.
- Remaining blockers are a concrete issuer-signature backend, a complete
  signed Bitcoin/RGB regtest fixture and end-to-end harness, and a transactional
  existing-contract update path. Existing-contract imports are deliberately
  rejected until copy-on-write/update semantics exist.

**Dependencies:** pinned RGB ecosystem APIs, issuer cryptography, Bitcoin/RGB
regtest fixtures, and transactional persistence semantics.

**Next acceptance slice:** select and review the issuer signature backend;
produce an independently reproducible signed regtest fixture; then implement
and test copy-on-write updates for an existing contract without weakening the
current rollback boundary.

### #220 — DLC CET construction

**Classification:** Research milestone with production blockers. Isolated
conformance and fixture evidence is useful, but no Gateway runtime or custody
path is complete.

**Current evidence**

- Research alignment and SDK comparison are recorded in
  `docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md` and
  `docs/research/DLC_STAGE0_SDK_COMPARISON_2026-07-22.md`.
- [PR #269](https://github.com/Conxian/conxian-gateway/pull/269),
  [#270](https://github.com/Conxian/conxian-gateway/pull/270),
  [#271](https://github.com/Conxian/conxian-gateway/pull/271), and
  [#272](https://github.com/Conxian/conxian-gateway/pull/272) merged isolated
  research/conformance/fixture slices.
- `experiments/dlc-stage0/` contains the probes and deterministic fixture. The
  Gateway still has no production DLC dependency, manager/provider state,
  wallet signing boundary, persistence/restart flow, or mainnet CET path.

**Dependencies:** exact upstream API/vector compatibility, cryptographic
announcement and attestation verification, funding/CET/refund semantics,
wallet ownership, and independent interoperability evidence.

**Next acceptance slice:** finish the manager/provider API decision and
independent offer/accept/sign/funding/CET/refund vectors before adding a
Gateway dependency or runtime endpoint.

### #189 — BitVM3 / BitVMX-GC evidence

**Classification:** Research milestone with production blockers. No stable
BitVM3/GC SDK, production deployment, or production pairing backend is
verified.

**Current evidence**

- [PR #259](https://github.com/Conxian/conxian-gateway/pull/259) merged the
  isolated BitVMX-CPU evaluation harness. [PR #267](https://github.com/Conxian/conxian-gateway/pull/267)
  and [PR #268](https://github.com/Conxian/conxian-gateway/pull/268) merged the
  research expansion and canonical SDK/paper/network-proof/cross-repository
  triage.
- [PR #278](https://github.com/Conxian/conxian-gateway/pull/278) carries the
  fail-closed generic BitVM verification change; its implementation was merged
  externally, while the Phase 4 documentation commit is a post-merge branch
  update. It does not resolve #189 and does not add a cryptographic backend.
- Current Gateway boundaries are `tools/bitvmx-eval/`,
  `internal/engine/src/bitcoin/groth16_verifier.rs`,
  `internal/engine/src/bitcoin/bitvm_adapter.rs`, and
  `internal/compliance/src/verifier.rs`. They are evaluation/interface
  boundaries, not a BitVM3/GC production verifier.
- Cross-repository triage is current only when issue states are refreshed:
  Platform #1187 and Nexus #169 remain open; Wallet #427, `.github` #41, and
  Core #188 are closed; Enclave #202 remains open.

**Dependencies:** stable upstream revision/API, license resolution,
reproducible builds, independent proof vectors, resource fit, protocol dispute
semantics, and cross-repository security/release acceptance.

**Next acceptance slice:** maintain research-only posture and refresh the
  evidence ledger only when a candidate supplies a stable API, reproducible
  artifacts, independent vectors, and an exact deployment/security review.

### #247 — ALEX settlement rail integration

**Classification:** Blocked/high-risk pending signer, contract, and governance
details. Existing ALEX code is a scaffold and prepared-payload boundary, not a
primary production settlement rail.

**Current evidence**

- `internal/engine/src/stacks/alex.rs` provides quote, prepared-payload, and
  trait surfaces. `AlexRpcClient::execute_swap` explicitly fails closed until
  secure signer-enclave integration exists.
- `internal/api/src/handlers.rs` exposes `/alex/quote` and `/alex/swap`; the
  swap handler currently returns a prepared payload rather than signing or
  broadcasting. `scripts/alex_rehearsal.sh` still describes a 501 rehearsal
  expectation, so the rehearsal contract itself needs alignment before any
  readiness claim.
- `conxian_market/docs/research/FUNDING_AND_ECONOMICS.md` requires an escrow
  contract, 3-of-5 controls, treasury exposure limits, and rapid exit behavior.
  `WALLET_TREASURY_FEASIBILITY.md` makes contract principals and signer
  governance prerequisites explicit.

**Dependencies:** exact ALEX contract IDs/ABIs and network behavior, secure
signer/enclave acceptance, escrow/custody semantics, treasury limits, emergency
exit controls, governance ownership, and independent security review.

**Next acceptance slice:** write and approve the exact contract/signer/
governance design, prove it on the intended Stacks network with deterministic
negative tests and multisig controls, and reconcile the rehearsal/API contract.
Do not label ALEX the primary settlement rail before that slice passes.

## Dependency map

```text
#222 release governance and artifact proof
  └── provides the delivery/observability controls needed by every production slice

#245 BIP-110 status + fee observability
  ├── lib-conxian-core versioned preflight contract
  ├── Bitcoin Core RPC/mempool/fee telemetry
  └── conxius-enclave-sdk release acceptance (#202 remains open)

#228 RGB stockpile/consignment
  └── rgb-std/rgb-persist-fs + issuer signatures + Bitcoin/RGB regtest fixture

#220 DLC CET research
  └── rust-dlc/DDK/API and vector decision + wallet/signing boundary

#189 BitVM3/GC research
  └── stable upstream artifacts + Platform/Nexus/Enclave security gates

#247 ALEX settlement
  └── ALEX contracts + secure signer + escrow/governance/treasury controls
```

## Selection rationale for Phase 2 (historical checkpoint)

The score selects **#222** as the highest overall backlog candidate, but this
phase is intentionally not changing workflows. The safe, useful #245 slice is
documentation and observability design:

1. Record the distinction between a completed proposal, node policy, signaling,
   and active consensus.
2. Define a versioned Core preflight passthrough with explicit unsupported and
   deployment-unknown states.
3. Define fee telemetry and route-confidence metrics that can be evaluated
   against observed outcomes.
4. Keep fee selection based on measured confirmation/mempool evidence; do not
   infer cheaper fees or reduced demand from BIP-110 status.

This preserved the current fail-closed posture, created a reviewable
acceptance slice, and avoided duplicating Bitcoin script interpretation in the
Gateway. Phase 3 subsequently took the highest-ranked #222 slice into the
release workflow and runbook without changing Gateway runtime behavior.

## Phase 3 implementation checkpoint — #222

The Phase 3 implementation is scoped to the release/security/coverage
workflows, `RELEASE.md`, `docs/CI_TOOLING_PINS.md`, deterministic artifact
verification, and its stdlib regression tests. It resolves the Gateway-owned
workflow gaps identified in the audit:

1. Release jobs require a valid tag whose exact commit is reachable from the
   fetched `origin/main` history. Direct exact-commit baseline jobs then run
   Rust formatting/Clippy/tests, mock-integrations tests, contamination and
   verifier tests, the Node typecheck/lint/build/test flow, pinned Cargo audit,
   pinned/checksummed Gitleaks, and the Lightning coverage gate. Packaging and
   every later release job depend on all baseline jobs.
2. The shipped artifact is the built `gateway` binary packaged for
   `x86_64-unknown-linux-gnu`; the workflow does not rebuild a separate binary
   during release publication.
3. The release asset set contains the archive, SHA-256 manifest, normalized
   CycloneDX 1.5 SBOM, and signed provenance bundle. `actions/attest` attests
   the archive, checksum manifest, and SBOM as the exact subjects.
4. GitHub Release publication waits for every baseline, packaging, and
   attestation job and runs in the `release` environment. The optional crates.io job remains separately
   environment-gated and fails before reading its token when Cargo packaging is
   not publishable.

This checkpoint does **not** claim that GitHub branch protection/rulesets,
environment reviewers, tag restrictions, required-check administration, Cargo
registry secrets, external CodeQL/GitGuardian/dependency-review results, or a
live release rehearsal are configured. Those controls must be verified by
repository administrators or a controlled release owner after merge.
