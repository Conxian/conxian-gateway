# Session Summary — BitVM Evidence and Cross-Repository Triage

**Date:** 2026-07-22
**Repository:** `Conxian/conxian-gateway`
**Branch:** `charlie/issue-189-bitvm-evidence-triage`

## Continuity verification

- Pulled `origin/main`; it was already current at `4a0433ad92b83bb59d69cb64f86128c1e0212a8e` (merged PR #267).
- Reviewed prior `docs/SESSION_SUMMARY_*.md`, `docs/GAP_ANALYSIS_*.md`, `docs/SPRINT_REVIEW_*.md`, and `docs/CROSS_REPO_STATUS.md` before making changes.
- Confirmed Gateway issue #189 remains open and research-oriented.
- Created the working branch from the verified `origin/main` head; no product-code changes were made.

## Evidence outcome

- IACR ePrint 2026/933 is the BitVM3 authority for paper/protocol evidence (received 2026-05-11; revised 2026-06-08), not a shipped SDK or production deployment.
- The official BitVM Rust repository is a developer preview with a signet/`bitvmnet` demo graph; those transactions are not Bitcoin mainnet evidence.
- The official BitVMX SNARK article links Bitcoin transaction `75eb2ad4f22263440fc4ceb61c51b0bb77721661dbfbec961358520b04107ec3`; it is classified as an upstream prototype transaction, not BitVM3-GC, a stable SDK, a production bridge, a Conxian verifier, or audit evidence.
- The current BitVMX GC article is the `/knowledge/` URL; the prior `/blog/` path is stale. No stable versioned GC SDK/API or verified BitVM3/BitVMX-GC production deployment was found.
- BitVM3, recursive Groth16, Nova/IVC/folding, garbled circuits, BitVM2, BitVMX-CPU, and BitVMX-GC remain separate evidence tracks.
- The concrete disposition is to keep #189 open and research-only, with production integration gated on the canonical report's verifier, protocol, security, operational, and provenance requirements.

## Durable triage issues

- Platform P0: https://github.com/Conxian/conxius-platform/issues/1187
- Nexus P1: https://github.com/Conxian/conxian-nexus/issues/169
- Wallet P1: https://github.com/Conxian/conxius-wallet/issues/427
- Organization documentation P2: https://github.com/Conxian/.github/issues/41
- Existing core tracker: https://github.com/Conxian/lib-conxian-core/issues/188
- Existing enclave acceptance tracker: https://github.com/Conxian/conxius-enclave-sdk/issues/202

## Files changed

- `AGENTS.md` — corrected stale #216/#219 and BitVMX research status bullets.
- `docs/research/BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md` — added the canonical dated evidence taxonomy, maturity, paper, network-proof, dependency, cross-repo, claim-correction, and readiness-gate report.
- `docs/research/BITVM3_BITVMX_RESEARCH_EXPANSION.md` — added the canonical refresh link and corrected the stale GC article and mainnet-prototype wording while retaining historical context.
- `docs/research/KNOWLEDGE_MAP.md` — linked the canonical refresh and corrected research status.
- `docs/CROSS_REPO_STATUS.md` — updated #189, merged #216/#219 status, and the six durable triage links with a historical supersession note.
- `docs/SESSION_SUMMARY_2026-07-22_BITVM_TRIAGE.md` — recorded this continuity and verification handoff.

## Verification

- `git diff --check` — pass.
- Repository-relative Markdown target check — pass; all checked relative targets exist. No repository-specific Markdown/link checker configuration was present.
- `cargo fmt --all -- --check` — pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass.
- `cargo test --workspace` — pass.
- `cargo test --workspace --features mock-integrations` — pass, including the 11 Groth16-boundary tests.
- `pnpm install && pnpm build && pnpm test` — pass. Next.js emitted its existing middleware deprecation and missing Auth.js secret test-server warnings; the build and all tests completed successfully.
- `python3 scripts/verify_contamination_guard.py` — pass; 60 files scanned and production paths clean.
- Health check — pass using the repository's Rust 1.96 toolchain and a simulated-network local invocation: `GET /api/v1/health` returned HTTP 200 with status `ok` and version `0.1.4`.
- An initial health attempt from `/tmp` selected the system Rust 1.89 toolchain and was blocked by the repository's `ruint` MSRV; rerunning from the repository with `rust-toolchain.toml` selected Rust 1.96 and passed. No repository changes were made for this environment issue.

## Follow-up gates

1. Keep #189 open/research-only; do not add BitVM3 or BitVMX-GC production dispatch based on paper, demo, signet, testnet, or prototype evidence.
2. Resolve the linked Platform, Nexus, Wallet, `.github`, Core, and Enclave SDK triage issues before making any readiness claim.
3. Require a real cryptographic verifier, canonical circuit/key/proof binding, negative and malformed-input coverage, reproducible vectors, security review, license resolution, network-specific operational evidence, and independent audit before promotion.
4. Reassess upstream BitVM/garbled-verifier and FairgateLabs dependency issues, including SPV, malformed-point, subgroup, hash, dispute, disablement, and CI security findings.
