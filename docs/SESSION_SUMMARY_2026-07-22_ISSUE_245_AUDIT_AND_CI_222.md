# Session Summary — Issue #245 Audit and CI #222

**Date:** 2026-07-22
**Repository:** `Conxian/conxian-gateway`
**Trigger:** https://github.com/Conxian/conxian-gateway/issues/245#issuecomment-5046109792
**Selected candidate:** https://github.com/Conxian/conxian-gateway/issues/222

This handoff records the final archive-boundary correction, the current
post-rebase lineage, and the evidence for the release-governance slice. It does
not claim that the remaining #222 administrative or rehearsal prerequisites are
complete.

## Phase 1/2 audit and selection

The current six-issue audit retains the following evidence-backed scores:

| Rank | Issue | Score | Current conclusion |
|---:|---:|---:|---|
| 1 | #222 CI/CD release governance | 88/90 | Selected; implementation is present, but administrative and live-release gates remain |
| 2 | #245 BIP-110 routing/fees | 62/90 | Observability/preflight slice only; no justified runtime fee rewrite |
| 3 | #228 RGB stash resolver | 60/90 | Phase 2 hardening merged; issuer backend and signed regtest evidence remain |
| 4 | #220 DLC CET | 58/90 | Research and conformance gates precede any runtime dependency |
| 5 | #189 BitVM3/BitVMX-GC | 55/90 | Research-only; no stable production integration target verified |
| 6 | #247 ALEX | 42/90 | Signer, contract, escrow, treasury, and governance prerequisites remain |

#222 remains **open/pending**. The outstanding prerequisites are merge and
required-check administration, protected release environment/reviewer rules,
protected-tag rules, a live tagged-release and attestation rehearsal, and
publishable Cargo package/path-dependency metadata. No live administration or
release rehearsal is claimed by this summary.

## Issue #245 factual boundary

The Gateway contains no production BIP-110 integration, BIP-110 fee predictor,
or evidence supporting a fee multiplier or fee-model rewrite. Proposal, policy,
signaling, and active-consensus status remain separate facts. The proposed
follow-on slice is versioned deployment/status observability, Bitcoin Core
preflight or `getblocktemplate` passthrough, fee telemetry, route-confidence
measurements, and acceptance metrics. BIP-110 status alone must not be used to
claim active deployment or reduced fees.

## Final archive and verifier corrections

- `scripts/verify_release_artifacts.py` now parses gzip with a zlib stream and
  rejects missing end-of-stream, unused data, trailing bytes, and any second
  gzip member. The decompressed tar must contain exactly two terminal USTAR zero
  blocks followed immediately by EOF.
- `.github/workflows/release.yml` uses GNU tar `--blocking-factor=1` together
  with `--format=ustar`, sorted entries, normalized metadata, and `gzip -n`, so
  the intended archive satisfies the verifier rather than relying on GNU tar's
  default record padding.
- The full verifier tests cover raw bytes after gzip, an additional empty gzip
  member, a concatenated nonempty gzip member, and additional tar zero blocks.
  The exact workflow archive command is also exercised.
- Remote-tag tests cover HTTP/JSON failure, URL-encoded tag names, cycles,
  exactly-maximum annotated-tag depth, and depth exhaustion.
- CycloneDX verification requires the dependency graph to represent the
  metadata root and every top-level component bom-ref. Fresh
  `cargo-cyclonedx 0.5.9` output was accepted without imposing a requirement
  on nested target descriptors that are not graph nodes.

## Rebase and lineage

The required pull was attempted before rebasing; the branch had diverged, so no
merge commit was created. After fetching again, the branch was rebased onto the
current `origin/main`:

- Branch: `charlie/issue-245-audit-2026-07-22`
- `origin/main`: `764859fd19c6b4305c0b7b9222c71493b3587177`
- Latest substantive parent before this summary was finalized:
  `31b271dfa08b9e7dba41b37124d0c8b623b245cc`
- Merge-base before the summary commit:
  `764859fd19c6b4305c0b7b9222c71493b3587177`
- Ahead/behind before the summary commit: `5/0`
- Branch-only commits, oldest to newest after rebase:
  `37c7bbc` → `1579273` → `f6cb1a2` → `6ae55c7` → `31b271d`
- The rebase used evidence-supported conflict resolution only and introduced
  no merge commit.

This file intentionally does not contain a guessed self-referential commit
hash. After the summary commit is finalized, resolve its containing commit with
`git log -1 --format=%H -- docs/SESSION_SUMMARY_2026-07-22_ISSUE_245_AUDIT_AND_CI_222.md`.
The lineage, merge-base, and ahead/behind values above describe the verified
parent state immediately before that summary commit; re-check them after the
commit and again after pushing.

## Files and artifacts

The audit branch includes the release workflow/tooling and dated continuity
updates, including:

- `.github/workflows/release.yml`, the other CI workflow pin/scope updates,
  `AGENTS.md`, `RELEASE.md`, and `docs/CI_TOOLING_PINS.md`;
- `scripts/verify_release_artifacts.py`, `scripts/verify_remote_tag.py`,
  `scripts/normalize_release_sbom.py`, and the Lightning coverage gate;
- `tests/test_verify_release_artifacts.py`, `tests/test_verify_remote_tag.py`,
  and `tests/test_normalize_release_sbom.py`;
- `docs/GAP_ANALYSIS_2026-07-22.md`, `docs/CROSS_REPO_STATUS.md`, the BIP-110
  and BitVMX research records, and this continuity summary.

This worker did not modify GitHub issue comments. The branch push and focused PR
creation occur after this summary commit and are recorded in the final task
handoff.

## Verification outcomes

- `actionlint .github/workflows/*.yml` — pass.
- `python3 -m unittest discover -s tests -p 'test_*.py'` — **48 passed**;
  normalizer, release-artifact, and remote-tag suites included.
- Python compilation, `git diff --check`, and commit checks — pass.
- Exact workflow archive fixture plus fresh `cargo-cyclonedx 0.5.9` and locked
  Cargo metadata — pass: 3,584 decompressed tar bytes, exactly two terminal
  zero blocks, 326 SBOM components, 327 dependency entries.
- `cargo fmt --all -- --check` — pass.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
  — pass.
- `cargo test --workspace --locked` — pass.
- `cargo test --workspace --features mock-integrations --locked` — pass.
- `pnpm install --frozen-lockfile`, the release baseline typecheck/lint/build,
  Playwright dependency setup, and `pnpm test` — pass.
- Pinned `cargo-audit 0.22.2` and `cargo audit` — pass.
- `./scripts/lightning_coverage_gate.sh 90` — pass at **94.20%**.
- `python3 scripts/verify_contamination_guard.py` — pass; 60 production files
  scanned.
- Local health probe — HTTP 200 with JSON `status=ok`.

## Environment limitations and remaining prerequisites

- The devbox is ARM64 (`aarch64`). The production
  `x86_64-unknown-linux-gnu` build could not complete because
  `x86_64-linux-gnu-gcc` was unavailable/permission-blocked while building
  `aws-lc-sys`. The verifier was exercised with the fresh actual SBOM and
  locked metadata plus a minimal synthetic x86_64 ELF fixture; this is not a
  claim that the production cross-target build passed.
- The devbox has Node `22.23.1`; the workflow pins Node 24. The release command
  set passed locally, but the pinned runner remains authoritative.
- #222 still needs ruleset/required-check administration, protected `release`
  environment and tags, a live tagged-release/attestation rehearsal, and
  crates.io package/path-dependency metadata and token prerequisites.

## Next-session continuity checks

1. Fetch `main` and re-check exact `HEAD`, merge-base, ahead/behind, and clean
   worktree before relying on this handoff.
2. Refresh GitHub issue/PR state and the source commit for dated cross-repo
   status documents; do not treat snapshots as live dashboards.
3. Keep #222 open until administration and a live release rehearsal are
   evidenced. Keep #245 at the observability/preflight boundary.
