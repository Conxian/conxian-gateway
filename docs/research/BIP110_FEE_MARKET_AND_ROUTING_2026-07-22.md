# BIP-110, Fee Markets, and Gateway Routing — Evidence Review

**Date:** 2026-07-22
**Gateway issue:** [#245 — Evaluate impact on routing and fee markets](https://github.com/Conxian/conxian-gateway/issues/245)
**Gateway snapshot:** [`6838d872513b681cf88f07fc5431f02b856b6d0e`](https://github.com/Conxian/conxian-gateway/commit/6838d872513b681cf88f07fc5431f02b856b6d0e)
**Status:** Research and observability design only; no BIP-110 integration or fee-model change is authorized by this document.

> **Executive finding:** BIP-110 is a completed temporary soft-fork proposal in
> the BIP registry, not evidence that its rules are active Bitcoin consensus on
> 2026-07-22. The referenced Bitcoin Core implementation pull requests are
> closed without a merge, current Core release documentation does not establish
> deployment, and the proponent monitor is non-authoritative. Any effect on
> fee levels, inscription demand, or bridge predictability remains an unverified
> hypothesis. Gateway routing must therefore remain based on observed node,
> mempool, block, and confirmation data rather than BIP-110 status alone.

## 1. Scope and decision record

This report answers a narrow question: what can the Gateway safely observe or
pass through if an upstream component reports BIP-110-related status or size
metadata?

It does **not**:

- claim BIP-110 activation, lock-in, enforcement, or chain-wide adoption;
- claim that limiting data fields reduces fees or inscription demand;
- treat relay policy as consensus validation;
- turn `lib-conxian-core` metadata validation into a transaction parser,
  interpreter, deployment detector, or Bitcoin consensus implementation;
- treat the `conxius-enclave-sdk` package version `2.0.12` as a production
  release; or
- authorize a fee multiplier, fee-model rewrite, automatic route change, or
  production settlement decision.

The recommended future candidate is a **deployment/status observability and
preflight passthrough slice**, followed by measured fee telemetry. The candidate
must preserve explicit unknown/unsupported states and a route-confidence value;
it must never silently convert proposal status into a fee instruction.

## 2. Evidence matrix

| Source / artifact | Verified state at the audit snapshot | What it establishes | What it does not establish |
|---|---|---|---|
| [BIP-110 specification](https://github.com/bitcoin/bips/blob/b0a1a276021cf371a93865315b274a55616e3b6c/bip-0110.mediawiki) | File inspected at commit `b0a1a276021cf371a93865315b274a55616e3b6c`; `Status: Complete` | Proposed temporary consensus rules and deployment parameters; specific script/witness limits | Activation, miner adoption, current network state, or fee outcomes |
| [BIP registry](https://github.com/bitcoin/bips/blob/f078d4f5ff70fe2921fd005d799a3d4a8ff7d55c/README.mediawiki) | File inspected at commit `f078d4f5ff70fe2921fd005d799a3d4a8ff7d55c`; BIP 110 is listed `Complete` | Registry terminology separates a completed proposal from a deployed BIP | A completed registry entry is not a deployment observation |
| [Bitcoin Core PR #34929](https://github.com/bitcoin/bitcoin/pull/34929) | Closed 2026-03-26; no merge | Proposed versionbits extensions were not merged into Bitcoin Core | No statement about other implementations or future work |
| [Bitcoin Core PR #34930](https://github.com/bitcoin/bitcoin/pull/34930) | Closed 2026-03-26; no merge | Proposed ReducedData Temporary Softfork implementation was not merged | No activation or deployment evidence |
| [Bitcoin-Dev discussion](https://groups.google.com/g/bitcoindev/c/nOZim6FbuF8) | Proposal discussion thread is reachable; discussion is not a deployment oracle | Shows the public technical/governance discussion context | Community discussion, signaling, or a proponent claim cannot establish consensus state |
| [Bitcoin Core 30.0 release notes](https://bitcoincore.org/en/releases/30.0/) | Current release notes say `-datacarriersize` defaults to `100000` and describe relay/mining policy behavior | Core’s node-local data-carrier policy defaults and their operational scope | A policy default is not a BIP-110 consensus rule |
| [`estimatesmartfee` RPC docs](https://bitcoincore.org/en/doc/26.0.0/rpc/util/estimatesmartfee/) | Versioned Core RPC documentation inspected | Fee estimates are node-produced, target-based estimates using available historical/mempool data | A universal fee truth, an activation signal, or a guarantee of confirmation |
| [Bitcoin Optech Newsletter #260](https://bitcoinops.org/en/newsletters/2023/07/19/) | Published 2023-07-19; mempool/relay/mining-policy material inspected | Relay policy, mempool inclusion, and mining selection are distinct operational layers | Consensus activation or a direct BIP-110 forecast |
| [ArXiv 2604.17183](https://arxiv.org/abs/2604.17183) | Submitted 2026-04-19; research model and abstract inspected | Fee formation can be studied from congestion, delay, RBF/CPFP, and block conditions | A validated Gateway predictor or a production threshold for fee decisions |
| [BIP-110 proponent monitor](https://bip110.org/monitor) | Audit snapshot retained at approximately `0.92%` signaling; the dynamic page later displayed `0.98%` on 2026-07-22 | A non-authoritative, time-varying view of signaling claimed by the proposal proponents | Consensus activation, lock-in, deployment, or a trustworthy route/fee input |
| [`lib-conxian-core` BIP-110 alignment](https://github.com/Conxian/lib-conxian-core/blob/main/docs/BIP110_ALIGNMENT.md) | `main` at `35432776a05cba6cd11bae9d6258ec7618a3138c`; alignment doc blob `6e28f250c8b639880126f98b87e2b2b3be4b3d6b` | Versioned fail-closed byte-size/preflight metadata contract and ownership boundary | Parsing, script interpretation, deployment detection, or network enforcement |
| [`lib-conxian-core` PR #184](https://github.com/Conxian/lib-conxian-core/pull/184) and [PR #224](https://github.com/Conxian/lib-conxian-core/pull/224) | Both merged; merge commits `1699cf3b04ee0755756f5e8c38ec37388c89efbd` and `fc8a9be5574c65fef560057eb4b75ccc3b43398a` | Core size contract and downstream handoff documentation exist | A Gateway integration or BIP-110 activation |
| [`conxius-enclave-sdk` release `v2.0.11`](https://github.com/Conxian/conxius-enclave-sdk/releases/tag/v2.0.11) | Latest visible release/tag verified as `v2.0.11`, tag commit `d3e9a6a26da1bd4c15e612ce7051a0bfdf640a83` | A pinned release target exists | Current `Cargo.toml` version `2.0.12` is not production-release evidence; issue #202 remains open |
| [Enclave issue #179](https://github.com/Conxian/conxius-enclave-sdk/issues/179) and [PR #203](https://github.com/Conxian/conxius-enclave-sdk/pull/203) | Issue #179 closed; PR #203 merged at `86b06486a722c1e84c55d4737fbd9035eb2507c7` | A scoped BIP-110/BIP-322 signing slice exists | Independent security/release acceptance or full transaction validation |

All web links in this matrix returned an HTTP-success response during the
2026-07-22 audit. Dynamic pages are recorded as snapshots, not immutable state
feeds.

## 3. Consensus, policy, and adoption are different states

The Gateway must represent these as separate fields or evidence classes. A
single `bip110_enabled: bool` would be ambiguous and unsafe.

| Layer | Meaning | Example evidence | Gateway treatment |
|---|---|---|---|
| **Consensus proposal** | A specification describes rules that would be enforced if deployed | BIP-110 text, registry status `Complete` | Store as proposal metadata and source revision; do not treat as active |
| **Consensus deployment state** | A particular network/node reports a real deployment state at a particular height | A validated node-specific deployment query or independently verified chain state | Preserve network, height, source, freshness, and `unknown` when unavailable |
| **Relay/mining policy** | A node or miner chooses what to relay or include before consensus validity is tested | Core `-datacarriersize`, `-minrelaytxfee`, mempool policy, package rules | Treat as local policy telemetry; never call it consensus validation |
| **Adoption/signaling** | Miners or operators signal or announce support for a proposal | The proponent monitor or observed versionbits signals | Treat as non-authoritative observation; do not infer activation |
| **Application preflight** | An adapter checks caller-supplied size/context metadata against a configured contract | `lib-conxian-core` BIP-110 preflight API | Preserve phase, context, provenance, findings, and unsupported states |

The distinction matters because Bitcoin Core 30.0’s `-datacarriersize=100000`
default is a policy setting described in release notes, while BIP-110 proposes
different consensus rules. Neither value alone says what a particular block or
network will accept under consensus.

## 4. Factual corrections to the issue context

### 4.1 BIP-110 status

BIP-110 is marked **Complete** in the BIP registry. The proposal is a temporary
soft fork with a one-year deployment model, not an active consensus rule as of
2026-07-22. The BIP file describes proposed thresholds, heights, grandfathering,
and expiry; those fields are proposal parameters, not a live deployment report.

### 4.2 Bitcoin Core implementation status

Bitcoin Core PRs #34929 and #34930 are closed and have no merge commit. The
current Core release notes and RPC documentation reviewed here do not establish
that BIP-110 is deployed by Bitcoin Core. The Gateway must not report “active”
based on the existence of either pull request.

### 4.3 Scope of the proposed limits

BIP-110 does not generically ban every form of arbitrary data or every large
transaction. Its rules distinguish, among other things, new output ScriptPubKeys,
OP_RETURN, OP_PUSHDATA payloads, script-argument witness items, undefined
witness/Tapleaf versions, annexes, Taproot control blocks, and Tapscript opcode
contexts. Exceptions and grandfathering are part of the proposal. A generic
“transaction has data” check would be incorrect.

### 4.4 Fee and inscription projections

Reduced inscription demand, lower fees, and more predictable bridge operations
are **hypotheses**, not verified results in this repository or in the cited
proposal sources. Fee formation depends on congestion, transaction arrivals,
block selection, mempool policy, RBF/CPFP behavior, package rules, and user
intent. The Gateway should measure these relationships before changing routing
or pricing behavior.

### 4.5 Core and enclave alignment

`lib-conxian-core` currently defines a versioned, fail-closed size/preflight
metadata contract. Upstream `main` is package version `0.3.0`; the latest
observed tag is `v0.2.11`. The issue’s `v0.2.12` assertion is not a stable
integration identifier and must not be used as one.

`conxius-enclave-sdk` `main` declares package version `2.0.12`, but the latest
verified release is `v2.0.11`. Its README describes the 2.x line as beta/
conditional and records P0 evidence gaps. Enclave issue #202 remains open.

## 5. Gateway current-state evidence

### 5.1 Existing fee and mempool behavior

The Gateway already has a bounded fee-bump surface:

- `internal/engine/src/bitcoin/fee_bump_policy.rs` classifies stuck
  transactions, applies attempt and maximum-feerate guardrails, and chooses
  RBF before CPFP when available.
- `internal/engine/src/bitcoin/mempool_orchestrator.rs` persists mempool
  tracking, records RBF/CPFP outcomes, and does not synthesize an RGB contract
  ID from a Bitcoin transaction ID.
- `internal/engine/src/bitcoin/rpc.rs` delegates RBF replacement to Core’s
  `bumpfee` RPC and deliberately does not construct/sign CPFP children without
  wallet UTXO and key context.

There is no current Gateway BIP-110 preflight route, `estimatesmartfee`
passthrough, block-quantile fee estimator, deployment detector, or fee predictor
in production source. The existing RBF/CPFP path must not be retrofitted with a
BIP-110 fee multiplier without an observed-data validation phase.

### 5.2 Core contract boundary

The current Core alignment is useful as a downstream contract only:

- `src/control_model/bip110.rs` owns canonical size limits;
- `src/control_model/bip110_preflight.rs` owns the versioned request/result
  envelope and fail-closed findings;
- `docs/BIP110_ALIGNMENT.md` explicitly states that Core does not parse or
  interpret transactions and does not infer deployment state.

The Gateway should consume this contract as metadata, preserve its provenance,
and keep script/context classification with the owning parser or adapter. It
must not add a second script interpreter merely to duplicate Core’s boundary.

### 5.3 Deployment status and route confidence

If a future implementation observes deployment state, the result should carry
at least:

```text
network
observed_at
source_kind            # node_rpc, chain_observation, proposal_registry, monitor
source_revision        # endpoint/version/commit where applicable
reported_state         # unknown, not_deployed, defined, started, locked_in, active, expired
observed_height
activation_height      # optional; only when independently established
expiry_height          # optional; only when independently established
freshness_seconds
confidence              # calibrated route-confidence, not a consensus verdict
```

`unknown` and `not_deployed` are different: the former means the Gateway cannot
establish the state; the latter means the selected authoritative source reports
that no deployment is active. A proponent monitor should be recorded as
`source_kind=monitor` and must not override a node or independently verified
chain observation.

## 6. Candidate design for a future #245 implementation slice

### Slice A — deployment/status observability

Add a read-only, provenance-carrying observation model. It should be suitable
for dashboards, audit logs, and route confidence but should not reject or
reprioritize transactions by itself.

Acceptance boundary:

- explicit `unknown`, `not_deployed`, `defined`, `started`, `locked_in`,
  `active`, and `expired` states;
- network and observed height are mandatory when a state is not `unknown`;
- stale or conflicting sources produce a visible conflict/unknown result;
- the proponent monitor is contextual and non-authoritative;
- no fee or route decision is made solely from proposal or signaling status.

### Slice B — Core preflight passthrough

Expose the Core request/result contract at the Gateway boundary without
reimplementing script parsing. Preserve:

- API version;
- `pre_construction` versus `post_serialization` phase;
- source provenance (`caller_classified` versus `serialized_transaction`);
- supported/unsupported/unknown context;
- ordered findings and exact byte measurements; and
- deployment state as separate metadata, never as an implicit validator flag.

Required boundary vectors are:

| Measurement | Boundary behavior |
|---|---|
| Non-OP_RETURN ScriptPubKey | `34` passes; `35` fails when the adapter classifies it as applicable |
| OP_RETURN ScriptPubKey | `83` passes; `84` fails when the adapter classifies it as applicable |
| Applicable pushdata or script-argument witness item | `256` passes; `257` fails |
| Explicit Taproot control block | `257` passes; `258` fails |

The exact rule/context classification remains adapter-owned. Add explicit tests
for unsupported contexts, unknown contexts, missing measurements,
phase/source mismatch, unsupported API versions, and deployment-unknown states.
Do not add a duplicate script interpreter in the Gateway.

### Slice C — fee and route telemetry

Collect evidence before selecting a model:

- Core `estimatesmartfee` result, target, mode, errors, and node identity;
- current mempool size, minimum rolling fee, backlog/queue quantiles, and
  package/ancestor signals where available;
- recent block fee-rate quantiles and selected transaction weight;
- transaction intent, route class, and deadline without storing unnecessary
  personal or wallet-identifying data;
- RBF attempts, target feerate, success/failure, replacement confirmation, and
  CPFP fallback outcomes; and
- route confidence with source freshness, coverage, and calibration metadata.

The initial output should be telemetry and a recommendation with an explicit
confidence/coverage field. It should not silently mutate a caller’s fee,
change route, or broadcast policy.

## 7. Fee-model acceptance metrics

Any future fee predictor must be evaluated against observed outcomes over a
rolling **30-day and 2016-block** window. The minimum report should include:

| Metric | Definition / acceptance question |
|---|---|
| Predicted vs. actual confirmation | How closely did the predicted target compare with the block in which the transaction confirmed? |
| Delay distribution | Report p50, p90, and p95 confirmation delay by route class and target. |
| Absolute feerate error | `abs(predicted_sat_vB - effective_confirmed_sat_vB)` and distribution by congestion regime. |
| Overpayment | Fee above the minimum observed feerate that met the stated target, with a conservative counterfactual definition. |
| Under-target rate | Share of transactions missing the stated confirmation target or deadline. |
| Coverage | Fraction of requests for which the model returns a recommendation rather than `unknown`/insufficient data. |
| Calibration | Whether a stated confidence interval contains the observed delay at the advertised rate. |
| RBF/CPFP outcome quality | Replacement success, confirmation delay after bump, extra fee paid, and fallback failure rate. |
| Stability | Drift across the 30-day and 2016-block windows, including policy or node changes. |

No production threshold should be invented from a single snapshot. Thresholds
must be approved after a baseline period, segmented by network, node policy,
transaction class, and deadline. A model that cannot report coverage or
calibration must fail closed to a documented fallback rather than fabricate a
precise fee.

## 8. Privacy, chain-split, and evasion risks

### Privacy and observability

- Mempool timing, route class, transaction size, and fee-bump history can form a
  wallet or business fingerprint. Store aggregate features and short-lived
  correlation identifiers rather than raw transaction histories unless an
  explicit audit purpose requires them.
- Do not send wallet identifiers, raw scripts, or unnecessary transaction
  payloads to a third-party fee service. Record node/source identity and
  retention policy.
- Route confidence must describe evidence quality, not user identity or a
  compliance conclusion.

### Chain split and false consensus

- Treating BIP-110 as active when one node, miner, or monitor reports signaling
  can cause incompatible preflight, broadcast, or routing decisions.
- A deployment observation must be network-scoped and height-scoped. Conflicting
  observations must produce `unknown`/`conflict`, not an averaged boolean.
- Policy differences can make a transaction relay successfully through one
  topology and fail through another without implying a consensus split.

### Evasion and classification

- A generic data detector can misclassify legitimate script/witness forms,
  grandfathered UTXOs, Taproot annex/control-block structure, or future witness
  versions.
- A user can choose a different transaction shape or relay path; BIP-110 status
  does not make a fee estimate or route confidence invariant.
- Signaling, monitor data, and observed mempool behavior can be strategically
  manipulated or become stale. Keep source provenance and freshness visible.
- A BIP-110-aware preflight must not become a second, divergent script
  interpreter. The owning parser must provide classified measurements and the
  Core contract must validate those measurements fail closed.

## 9. Recommended disposition

Keep issue #245 open as a research/observability item. The next safe milestone
is a design and telemetry contract, not a fee multiplier. The order should be:

1. pin the exact Core contract commit/release and expose version/phase/context
   metadata;
2. implement deployment/status observation with explicit unknown and source
   provenance;
3. collect fee/mempool/block/RBF/CPFP outcomes;
4. evaluate the acceptance metrics over rolling 30-day and 2016-block windows;
5. only then consider a model change, with a reversible feature flag and
   independent review; and
6. never alter fees based solely on BIP-110 proposal, signaling, or monitor
   status.

This preserves Gateway’s role as a routing and verification membrane: observe,
measure, and route with evidence, without claiming consensus authority or taking
custody.

## 10. Source ledger

| Source | Access date | Version / commit / state | Use in this report |
|---|---|---|---|
| https://github.com/bitcoin/bips/blob/master/bip-0110.mediawiki | 2026-07-22 | Audited file commit `b0a1a276021cf371a93865315b274a55616e3b6c` | Canonical BIP-110 status, rules, exceptions, grandfathering, and proposed deployment |
| https://github.com/bitcoin/bips/blob/master/README.mediawiki | 2026-07-22 | Audited file commit `f078d4f5ff70fe2921fd005d799a3d4a8ff7d55c` | BIP registry status terminology and BIP-110 `Complete` entry |
| https://github.com/bitcoin/bitcoin/pull/34929 | 2026-07-22 | Closed 2026-03-26; no merge | Core versionbits proposal state |
| https://github.com/bitcoin/bitcoin/pull/34930 | 2026-07-22 | Closed 2026-03-26; no merge | Core ReducedData implementation proposal state |
| https://groups.google.com/g/bitcoindev/c/nOZim6FbuF8 | 2026-07-22 | Public discussion thread; no deployment authority | Proposal discussion context |
| https://bitcoincore.org/en/releases/30.0/ | 2026-07-22 | Bitcoin Core 30.0 release notes | `-datacarriersize=100000` default and policy distinction |
| https://bitcoincore.org/en/doc/26.0.0/rpc/util/estimatesmartfee/ | 2026-07-22 | Bitcoin Core 26.0 RPC documentation | Node fee-estimate semantics and target range |
| https://bitcoinops.org/en/newsletters/2023/07/19/ | 2026-07-22 | Bitcoin Optech Newsletter #260, 2023-07-19 | Relay/mempool/mining-policy distinction |
| https://arxiv.org/abs/2604.17183 | 2026-07-22 | Submitted 2026-04-19; arXiv:2604.17183 | Context for congestion/delay/RBF/CPFP fee research; not a Gateway model |
| https://bip110.org/monitor | 2026-07-22 | Dynamic proponent monitor; audit snapshot ~0.92%, later same-day display ~0.98% | Contextual signaling observation only; explicitly non-authoritative |
| https://github.com/Conxian/lib-conxian-core/blob/main/docs/BIP110_ALIGNMENT.md | 2026-07-22 | `main` commit `35432776a05cba6cd11bae9d6258ec7618a3138c`; doc blob `6e28f250c8b639880126f98b87e2b2b3be4b3d6b` | Core ownership and fail-closed preflight boundary |
| https://github.com/Conxian/lib-conxian-core/pull/184 | 2026-07-22 | Merged; `1699cf3b04ee0755756f5e8c38ec37388c89efbd` | Core size contract implementation |
| https://github.com/Conxian/lib-conxian-core/pull/224 | 2026-07-22 | Merged; `fc8a9be5574c65fef560057eb4b75ccc3b43398a` | Downstream handoff/status refresh |
| https://github.com/Conxian/conxius-enclave-sdk/issues/179 | 2026-07-22 | Closed 2026-07-20 | Enclave BIP-110 feature issue state |
| https://github.com/Conxian/conxius-enclave-sdk/pull/203 | 2026-07-22 | Merged; `86b06486a722c1e84c55d4737fbd9035eb2507c7` | BIP-322/BIP-110 signing slice |
| https://github.com/Conxian/conxius-enclave-sdk/issues/202 | 2026-07-22 | Open | Independent security/release acceptance blocker |
| https://github.com/Conxian/conxius-enclave-sdk/releases/tag/v2.0.11 | 2026-07-22 | Release `v2.0.11`; tag commit `d3e9a6a26da1bd4c15e612ce7051a0bfdf640a83` | Latest verified release target |
| https://github.com/Conxian/conxian-gateway/issues/245 | 2026-07-22 | Open; current issue request | Gateway scope and downstream ownership |
