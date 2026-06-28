# Gap Analysis & Scoring (2026-06-28)

This document tracks identified gaps across the Conxian Gateway portfolio, scored by Risk, Impact, and Effort.

## 1. Scoring Logic
- **Risk**: Potential for security breach, data loss, or regulatory non-compliance (1-5).
- **Impact**: Benefit to the platform, user experience, or developer adoption (1-5).
- **Effort**: Estimated engineering or research time required (1-5).

## 2. Gap Matrix

| ID | Gap Description | Domain | Risk | Impact | Effort | Priority Score | Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **G-01** | Missing CI Validation Scripts (CON-1322) | Security | 4 | 5 | 3 | **12** | Done |
| **G-02** | Production Lightning Backend Skeleton | Technical | 2 | 5 | 5 | **10** | Backlog |
| **G-03** | Missing Flagship Technical Whitepaper | Legal/Public | 1 | 5 | 4 | **9** | Backlog |
| **G-04** | Missing Developer Quickstart & Guide | Legal/Public | 1 | 5 | 2 | **6** | Backlog |
| **G-05** | No automated release/tagging workflow | Process | 3 | 5 | 3 | **11** | In Progress |
| **G-06** | Dependency Review fail-on-error disabled | Security | 5 | 3 | 1 | **15** | Verified |
| **G-07** | Actions/Checkout version drift | Hygiene | 1 | 1 | 1 | **2** | Done |
| **G-08** | Tier 2 Adapters (Liquid/Babylon) Shadow-Mode | Technical | 2 | 4 | 4 | **8** | Active |
| **G-09** | BitVM3 / Recursive Proof Research | Research | 1 | 5 | 5 | **5** | Research |
| **G-10** | Missing `docs/governance/CHANGELOG.md` | Hygiene | 1 | 2 | 1 | **4** | Missing |

## 3. High-Priority Remediation (Priority Score > 10)

### [G-06] Dependency Review Security Gate
- **Risk**: 5 (High)
- **Remediation**: Update `dependency-review.yml` to remove `continue-on-error: true`.
- **Target**: Immediate.

### [G-01] Unified CI Validation Scripts
- **Risk**: 4 (High)
- **Remediation**: Implement the 6 missing Python validation scripts in `scripts/`.
- **Target**: Next PR.

### [G-05] Release Workflow Implementation
- **Impact**: 5 (High)
- **Remediation**: Create `release-monorepo.yml` using tag-based triggers and changelog generation.
- **Target**: Current Cycle.

## 4. Opportunity Mapping
- **Recursive Proofs**: BitVM3 research shows 40% prover time reduction. High long-term impact for sharding.
- **Wasm Verification**: Opportunity to move UCV-1 verification to the client-side (SDK) for faster UX.
- **ISO 20022 Expansion**: Transition from pacs.008 to full camt.* reporting for treasury.
