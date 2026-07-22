# Session Summary — Issue #245 Audit and CI #222

**Date:** 2026-07-22
**Repository:** `Conxian/conxian-gateway`
**Trigger:** Close the remaining pre-push P1/P2 release-audit findings while
preserving the factual #245 BIP-110 boundary and the Session Continuity
Protocol.

## Phased approach

1. **Continuity and lineage:** attempted the required `git pull origin main`,
   then inspected the graph, merge-base, and reflog because the branch had
   diverged from `origin/main`. The active lineage is preserved; no rebase or
   force update was performed.
2. **Release identity:** added last-moment remote-tag rechecks immediately
   before GitHub Release creation and crates.io publication. The check uses the
   GitHub REST API, peels annotated tags, and compares the result with the
   immutable commit emitted by `release-identity`.
3. **Artifact proof:** hardened the raw USTAR scanner and added semantic
   CycloneDX 1.5 validation against locked Cargo workspace metadata. Added
   deterministic and adversarial regression tests without extracting archives.
4. **Snapshot alignment:** corrected the root instructions, Node CI scope, and
   cross-repository wording so dated status documents are not presented as
   live dashboards. #222 and #245 remain accurately open/research-bound.
5. **Verification and handoff:** ran the repository-required checks, a fresh
   cargo-cyclonedx/locked-metadata release verification, and the Lightning gate.
   The final changes and this summary are intended for one signed-off commit;
   the post-commit SHA must be recorded from `git rev-parse HEAD`, not guessed
   in this file.

## Verified branch state before the final commit

- Branch: `charlie/issue-245-audit-2026-07-22`
- Current pre-commit `HEAD`: `570db329c7144c456c074f1226b8d1022490496f`
- Original local base: `6838d872513b681cf88f07fc5431f02b856b6d0e`
- `origin/main`: `764859fd19c6b4305c0b7b9222c71493b3587177`
- Merge-base with `origin/main`: `0dc6390ddbfbb4d74c472da3a86e90aa2397524f`
- Ahead/behind before the final commit: `3/1`
- Active branch-only lineage: `8945ec4957f63bfe74fd8120889d57cc3154aeec` →
  `f2d6a6f4ff907a04321d5073604215166d1bbb57` →
  `570db329c7144c456c074f1226b8d1022490496f`
- Previously reported `f2ef5d2eeb762f2c255d2dec3ef62dc18afd2512` and
  `c622ea3f9f8ff75edf10125b89a03814fb8959b6` are not ancestors; the active
  equivalent commits above are the lineage to preserve.

The branch is not demonstrably based on the latest `origin/main`; the entry
agent must decide the later rebase/merge and push.

## Six-issue scoring and selection

The dated gap analysis ranks the open Gateway inventory as follows:

| Rank | Issue | Score | Selection rationale |
|---:|---:|---:|---|
| 1 | #222 CI/CD release governance | 88/90 | Implementation-ready, highest risk reduction, and directly controls release/proof claims |
| 2 | #245 BIP-110 routing/fees | 62/90 | Narrow research/observability slice; no justified runtime rewrite |
| 3 | #228 RGB stash resolver | 60/90 | Hardening is merged, but issuer backend and signed regtest evidence remain |
| 4 | #220 DLC CET | 58/90 | Research and conformance gates still precede a runtime dependency |
| 5 | #189 BitVM3/BitVMX-GC | 55/90 | Evidence remains research-only; no stable production target |
| 6 | #247 ALEX | 42/90 | Blocked by signer, contract, escrow, treasury, and governance prerequisites |

This session selected #222 because its remaining slice is concrete and
release-critical. It remains **open/pending** merge, required-check and
protected-environment administration, a live tagged-release rehearsal, and
publishable Cargo metadata. No live administrative control or release rehearsal
is claimed here.

## Issue #245 factual conclusion

The Gateway tree contains no production BIP-110 integration, no BIP-110-driven
fee predictor, and no evidence supporting a fee multiplier or fee-model rewrite.
BIP-110 proposal, policy, signaling, and active-consensus status must remain
distinct; a status change cannot be used to claim active deployment or reduced
fees. The next slice should be operational and versioned:

- deployment/status observability;
- Bitcoin Core preflight or `getblocktemplate` passthrough with an explicit
  versioned contract;
- fee telemetry and route-confidence measurements; and
- acceptance metrics for routing decisions.

Do not replace the existing fee model or infer fee reductions from BIP-110
status alone.

## Files and artifacts produced

- `.github/workflows/release.yml` — locked metadata propagation, complete test
  discovery, and immediate remote-tag checks before both publication paths.
- `scripts/verify_remote_tag.py` — token-safe GitHub REST tag resolver with
  annotated-tag peeling and immutable-commit comparison.
- `scripts/verify_release_artifacts.py` — exact USTAR validation and locked
  Cargo/CycloneDX semantic checks.
- `scripts/normalize_release_sbom.py` — fail-closed input and property-shape
  validation.
- `tests/test_verify_remote_tag.py`, `tests/test_verify_release_artifacts.py`,
  and `tests/test_normalize_release_sbom.py` — deterministic tag, archive,
  SBOM, metadata, and adversarial regression coverage.
- `AGENTS.md`, `RELEASE.md`, and `docs/CROSS_REPO_STATUS.md` — dated snapshot
  language, all-workspace Node scope, and accurate #222/#245 wording.
- This session summary; no GitHub issues/comments were modified and nothing was
  pushed.

## Verification outcomes

- `actionlint .github/workflows/*.yml` — pass.
- `python3 -m unittest discover -s tests -p 'test_*.py'` — **36 passed**.
- `python3 -m py_compile ...` and `git diff --check` — pass.
- Fresh `cargo cyclonedx 0.5.9` output plus `cargo metadata --locked
  --format-version 1 --no-deps` — normalized and verified successfully: 326
  CycloneDX components, exact workspace inventory, and resolved dependency
  references.
- `cargo fmt --all -- --check` — pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass.
- `cargo test --workspace` — pass.
- `cargo test --workspace --features mock-integrations` — pass.
- `pnpm install --frozen-lockfile && pnpm build && pnpm test` — pass; the
  release baseline's workspace typecheck/lint/build/test command set also
  passed.
- Pinned `cargo-audit 0.22.2` and `cargo audit` — pass.
- `./scripts/lightning_coverage_gate.sh 90` — pass at **94.20%**.
- `python3 scripts/verify_contamination_guard.py` — pass; 60 production files
  scanned.
- Local health probe — HTTP 200 with `status=ok`.

## Environment limitations and remaining prerequisites

- The devbox could not complete the production cross-target build because the
  `x86_64-linux-gnu-gcc` cross compiler was unavailable/permission-blocked.
  Fresh SBOM verification and archive checks therefore used the actual fresh
  SBOM/locked metadata plus a minimal synthetic x86_64 ELF fixture; the
  production build still requires the release runner/toolchain.
- The devbox Node runtime is 22.23.1 while the workflow pins Node 24; the
  release command set passed locally, but the pinned runner remains authoritative.
- Required external work remains: rebase/merge onto current `origin/main`,
  push, configure branch/ruleset required checks, configure protected `release`
  reviewers/environment, enforce tag force-update restrictions as defense in
  depth, run one live tagged-release/attestation rehearsal, and resolve
  crates.io path-dependency/package metadata and token prerequisites.

## Next-session start instructions

1. Fetch/pull current `main` and re-check the branch graph, exact `HEAD`,
   merge-base, and ahead/behind counts before rebasing or pushing.
2. Refresh GitHub open issue and pull-request states and the source commit for
   `docs/CROSS_REPO_STATUS.md`; treat all snapshot and gap-analysis documents
   as dated evidence, not live state.
3. Read this summary and the latest `docs/GAP_ANALYSIS_*.md`, then verify prior
   artifacts and any release/admin changes before selecting new scope.
4. Keep #222 open until merge, administrative controls, and a live release
   rehearsal are evidenced. Keep #245 at the observability/preflight boundary;
   do not claim active BIP-110 deployment or fee reductions.
