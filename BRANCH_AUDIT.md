# Conxian Gateway: Repository Branch Audit Report (April 2026)

This report documents the results of a comprehensive audit of all 47 remote branches. Based on patch analysis and commit history, 35 branches are recommended for removal.

## 1. Functionally Merged Branches
The following branches have been successfully merged into `main` via Pull Requests or contain logic already present in production patches.
- `ai-con-403-readme-ownership-and-codeowners`
- `audit-production-readiness-gateway-11232461536560604960`
- `botshelomokoka/con-147-alignment-audit-10711599692256767047`
- `charlie/alex-quote-docs-followup`
- `charlie/alex-quote-docs-semantics`
- `charlie/alex-quote-docs-validation`
- `charlie/alex-swap-not-implemented`
- `charlie/alex-swap-stable-501`
- `charlie/bitvm-job-hash-jcs`
- `charlie/bitvm-job-hash-tag-strict`
- `charlie/bitvm-job-hash-tag-strict-followup`
- `charlie/compliance-amount-parse-errors`
- `charlie/con-160-globally-complete`
- `charlie/con-418-bitcoin-rpc-auth-trim`
- `charlie/con-492-x402-parser-filter`
- `charlie/con-70-zkml-verification`
- `charlie/fix-amount-parse-errors-context`
- `charlie/fix-amount-parsing-no-f64`
- `charlie/fix-amount-parsing-no-f64-v2`
- `charlie/fix-bitvm-verification`
- `charlie/fix-codeowners-simplify`
- `charlie/fix-codeowners-team-syntax`
- `charlie/fix-readme-alex-quote-method`
- `charlie/fix-zkml-image-id-parse`
- `charlie/harden-zkml-verification`
- `charlie/restore-iso20022-parsing`
- `charlie/restore-iso20022-parsing-v2`
- `charlie/state-root-non-ascii-ws-test`
- `charlie/tee-settlement-attestation`
- `charlie/tee-settlement-attestation-followup`
- `charlie/zkml-receipt-limits-followup`
- `feat-fiat-gateway-implementation-11877877150361172350`
- `feat/hardened-settlement-ingress-con-163-6663957579318093791`
- `feat/linear-alignment-urgent-10773333514409016323`
- `feat/offline-pos-sync-and-mainnet-hardening-con-78-4972651857816393098`
- `feature/a2p-ntt-enhancements-4593743720121463714`
- `fix/ingress-contract-tests`
- `improve-governance-docs-8680048328179488987`
- `jules/institutional-hardening-cjcs-v2-4765259349195086214`
- `jules/mainnet-hardening-alignment-3994870843557432824`
- `mainnet-readiness-alignment-11658704593586632277`
- `production-alignment-mainnet-readiness-15794099133794960330`
- `sab-infra-migration-hardening-6375350590507230082`

## 2. Recommendation
It is recommended that the above branches be deleted from the remote repository to maintain a clean and audit-ready source control environment. The `main` branch is currently healthy, passing all tests, and contains the production logic originally targeted by these features.
