# Session Summary — 2026-07-22 BitVM Fail-Closed Phase 4

- **Repository:** `Conxian/conxian-gateway`
- **Branch:** `charlie/issue-189-bitvm-fail-closed`
- **Pull request:** [#278](https://github.com/Conxian/conxian-gateway/pull/278)
- **Issue:** [#189](https://github.com/Conxian/conxian-gateway/issues/189)
- **Trigger:** [issue comment](https://github.com/Conxian/conxian-gateway/issues/189#issuecomment-5050390033)
- **Approval:** [issue comment](https://github.com/Conxian/conxian-gateway/issues/189#issuecomment-5050635950)

> **Status:** Research / evaluation only. PR #278 was pending at the Phase 4
> continuity checkpoint and was subsequently merged externally; the Phase 4
> documentation commit was pushed afterward and is not in merged `main`. The
> implementation and docs must not be treated as resolving issue #189.

## Continuity verification

- The session began with a clean worktree on
  `charlie/issue-189-bitvm-fail-closed` at implementation commit
  [`114b857ed9d400beaf474cb68e7ac5f25ef58d78`](https://github.com/Conxian/conxian-gateway/commit/114b857ed9d400beaf474cb68e7ac5f25ef58d78).
- Fetching exposed an existing remote merge on the PR branch rather than an
  unexpected local edit: remote head was
  [`c893cbb39ea9d680b229a89035ab38f29ed51b8b`](https://github.com/Conxian/conxian-gateway/commit/c893cbb39ea9d680b229a89035ab38f29ed51b8b),
  with `main` merged at
  [`81d175540922b25192b683e95c9b48230c009454`](https://github.com/Conxian/conxian-gateway/commit/81d175540922b25192b683e95c9b48230c009454).
- The branch was fast-forwarded with `git pull --ff-only`; no remote work was
  overwritten. `git pull --ff-only origin main` was already up to date, and
  implementation commit `114b857...` remained an ancestor of the branch head.
- Continuity artifacts were present: prior session summaries, dated gap
  analyses, sprint review, and `docs/CROSS_REPO_STATUS.md`.
- The verified research base before PR #278 was
  [`d7032ab621ad038f247566f820ac664a6c8c071c`](https://github.com/Conxian/conxian-gateway/commit/d7032ab621ad038f247566f820ac664a6c8c071c).
- Current linked issue states were re-checked with GitHub CLI: Platform #1187,
  Nexus #169, and Enclave #202 are open; Wallet #427, `.github` #41, and Core
  #188 are closed remediation records.

## External merge boundary

- GitHub reports PR #278 was merged externally on 2026-07-22T19:57:47Z by
  `botshelomokoka` as merge commit
  [`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b).
- Charlie did not merge the PR. The Phase 4 documentation commit
  [`e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005`](https://github.com/Conxian/conxian-gateway/commit/e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005)
  was created at 2026-07-22T20:07:46Z and pushed after the merge, so it is on
  the branch but not in merged `main`.
- Issue #189 remains open and research-only. If these docs must land in `main`,
  they need a separate reviewed follow-up path; no new PR or merge was created
  in this session.

## Phase 1 — cross-repository contract findings

The canonical report now records the following compatibility boundary:

- Gateway's canonical envelope is versioned BN254 and binds circuit,
  verification key, public inputs, witness commitment, and block context. The
  Gateway production path has no cryptographic backend.
- Nexus's current default branch uses
  `Groth16::<Bls12_381>` with a caller-supplied verification key. It lacks the
  Gateway circuit/VK/root-binding semantics and is not interoperable with the
  Gateway BN254 envelope. See [Nexus #169](https://github.com/Conxian/conxian-nexus/issues/169).
- Platform BitVM/ZKCP defaults remain simulations/scaffolds; length-only or
  unconditional checks can produce success-shaped results without cryptographic
  verification. See [Platform #1187](https://github.com/Conxian/conxius-platform/issues/1187).
- Enclave production proof routes are explicitly unavailable/fail-closed and
  bind policy/replay context, but do not provide a BitVM/Groth16 backend. See
  [Enclave #202](https://github.com/Conxian/conxius-enclave-sdk/issues/202).
- Before selecting a backend, ownership is still required for canonical
  envelope/errors, cryptographic backend, VK/circuit registry, chain
  observation, enclave attestation/capability policy, and client presentation.

## Phase 2 — upstream evidence refresh

The canonical report distinguishes source-verified facts, upstream-reported
claims, on-chain artifacts, local implementation evidence, and unresolved items.
The durable findings are:

- [IACR ePrint 2026/933](https://eprint.iacr.org/2026/933) is the BitVM3 paper
  record, received **2026-05-11** and revised **2026-06-08**. It is research
  evidence, not a shipped SDK, release, or deployment.
- Official BitVMX platform language describes GC/DV-SNARK support as “coming in
  2026”; no stable public BitVMX-GC SDK/API, release, or verified GC deployment
  was found.
- BitVMX-CPU has GitHub Release `v0.5.11` and newer tag `v0.8.0`; the pinned
  default-branch/tag revision is `d390832c8e0f2a01453e8ef4bf65dbe715fb9236`,
  while `v0.8.0` resolves to
  `e23fbfccb0b50b52c882e6ba4f57eba3b7c3887f`. README says MIT while repository
  metadata and `LICENSE` say Apache-2.0; the contradiction is unresolved.
- `BitVM/garbled-snark-verifier` has Cargo package `0.5.0`, tag
  `v0.5.0-alpha.6` (and older `v0.3.0`), GPL-3.0-only metadata/`LICENCE`, no
  GitHub Release, and open subgroup, commitment/hash, and ARM64 issues.
- `GOATNetwork/bitvm2-gc` has no release or root license artifact/GitHub-detected
  license; its Cargo workspace declares `MIT OR Apache-2.0`. The approximately
  10.4B-gate and 51–374 GB figures are upstream-reported, not Conxian benchmarks.
- Transaction
  `75eb2ad4f22263440fc4ceb61c51b0bb77721661dbfbec961358520b04107ec3` is
  historical BitVMX-linked prototype evidence confirmed at block `853871` with
  block time **2024-07-25**. It is not BitVM3-GC or production bridge evidence.
- Union Bridge remains upstream-described Rootstock Testnet/experimental; V1.5
  dispute mechanisms are inactive, no formal audit is cited, and mainnet is a
  2027 roadmap item.
- The canonical report preserves BitVM #285/#376/#415, garbled-verifier PR #80
  and issues #76/#82/#87/#43, Fairgate client issues #316/#317/#321/#322/#325/
  #331/#336/#338/#339, and GOAT PR #48.

## Phase 3 — implementation handoff

PR #278 changes the generic `POST /api/v1/chains/bitvm/verify` behavior to fail
closed. It propagates typed `VerifierUnavailable` and returns HTTP `501` with
`status: "unsupported"`, `code: "verifier_unavailable"`, and
`authoritative: false` rather than treating a present `root_hash` as proof.
The implementation does not add a cryptographic backend, pairing implementation,
recursive Groth16, BitVM3/BitVMX-GC adapter, settlement path, persistence,
compliance authorization, or unconfigured-backend wiring.

## Phase 4 — documentation consolidation

Updated artifacts:

- `docs/research/BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md` — canonical
  current report, refresh metadata, exact upstream facts, cross-repo contract
  table, blocker links, candidate scorecard, and readiness decision.
- `docs/research/BITVM3_BITVMX_RESEARCH_EXPANSION.md` — explicitly historical;
  current-source claims point to the canonical report.
- `docs/research/BITVMX_EVAL.md` — evaluator pin and source-of-truth boundary
  clarified; no claim that newer tags/releases are compatible.
- `docs/GAP_ANALYSIS_2026-07-22.md` and `docs/CROSS_REPO_STATUS.md` — explicit
  post-snapshot PR #278 note and current open cross-repo acceptance issues,
  without rewriting the 14:42:43Z historical snapshot.
- `docs/research/OPPORTUNITY_MAP_AND_EXPANSION.md` — canonical report is now the
  current BitVM evidence link; the expansion remains historical.
- This session summary records continuity, evidence, implementation scope,
  scorecard, verification, and remaining gates.

## Candidate scorecard and decision

The canonical report uses a 0–5 triage score for BN254/envelope fit, API/release
maturity, security/operational evidence, and license/reproducibility. It is not a
security rating or approval:

| Candidate/evidence track | Score /20 | Decision |
|---|---:|---|
| Gateway canonical boundary + PR #278 guardrail | **16** | Contract/fail-closed guardrail only; no backend |
| Nexus `Bls12_381` verifier path | **6** | Not interoperable with Gateway BN254 |
| BitVMX-CPU pinned evaluator | **4** | Isolated CPU evaluator only |
| BitVMX-GC platform/article | **1** | Roadmap/design evidence only |
| `garbled-snark-verifier` 0.5.0 | **5** | GPL reference implementation; open security/format/ARM64 gates |
| GOAT `bitvm2-gc` | **4** | Resource, release, license, and reproducibility blockers |
| Union Bridge | **3** | Rootstock Testnet/experimental, not production evidence |

**Decision:** keep #189 open and research-only. No external candidate satisfies
the promotion gates. PR #278's implementation was merged externally, but the
post-merge documentation commit is not in merged `main`; neither resolves #189.

## Verification

### Continuity and documentation checks observed in this session

- `git status --short --branch` — clean before edits on the expected branch.
- `git fetch origin --prune` and explicit PR-branch fetch — remote divergence
  identified without overwriting work.
- `git pull --ff-only origin charlie/issue-189-bitvm-fail-closed` — fast-forwarded
  local checkout to `c893cbb...`.
- `git pull --ff-only origin main` — already up to date.
- `git merge-base --is-ancestor 114b857ed9d400beaf474cb68e7ac5f25ef58d78 HEAD` —
  exit `0`.
- `git diff --check` — pass.
- `cargo fmt --all -- --check` — pass.
- `python3 scripts/verify_contamination_guard.py` — pass; 62 files scanned and
  production paths clean.
- Repository-relative Markdown target check — pass; 22 targets checked and all
  targets exist.
- `gh issue view` checks — Platform #1187, Nexus #169, Enclave #202 `OPEN`;
  Wallet #427, `.github` #41, and Core #188 `CLOSED`.
- `gh pr view 278 --json ...` before documentation edits — PR head
  `c893cbb...`; all reported existing checks were `COMPLETED`/`SUCCESS` at that
  pre-documentation head. New checks after the docs push must be re-queried.

### Full implementation baseline re-verified

These exact commands were run after the documentation edits and completed
successfully:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass.
- `cargo test --workspace` — pass; all workspace suites reported zero failures.
- `cargo test --workspace --features mock-integrations` — pass, including 12
  Groth16-boundary tests.
- `pnpm install && pnpm build && pnpm test` — pass; existing Next.js middleware
  deprecation, `NO_COLOR`/`FORCE_COLOR`, and Auth.js missing-secret test-server
  warnings were non-blocking.
- Simulated Gateway startup with `CONXIAN_NETWORK=simulated` and
  `GET /api/v1/health` — HTTP 200 with `{"status":"ok","version":"0.1.4"}`.

## Remaining gates and next session steps

1. Do not merge further in this session. Because the Phase 4 documentation was
   pushed after PR #278's external merge, land it through a separate reviewed
   follow-up path if it must be included in `main`.
2. Resolve ownership for the six cross-repository contract surfaces before
   selecting a backend or registry.
3. Require a pinned, reproducible backend with reconciled license, independent
   positive/negative vectors, resource measurements, protocol/SPV/dispute review,
   and independent cryptographic/security review.
4. Close or otherwise satisfy Platform #1187, Nexus #169, and Enclave #202;
   retain Wallet #427, `.github` #41, and Core #188 as completed evidence.
5. Keep client/API presentation fail-closed: unavailable, simulated, and
   rejected results must never be presented as verified settlement evidence.
6. Re-run and record terminal CI checks for the final documentation commit; do
   not claim green if any check is pending or non-success.
