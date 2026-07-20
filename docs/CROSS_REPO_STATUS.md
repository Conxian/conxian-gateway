# Cross-Repository Status Dashboard

**Last Updated:** 2026-07-20
**Sprint:** W29 (2026-07-15 to 2026-07-25)  
**Maintained By:** Agent sessions

---

## Repository Inventory (Canonical — from PORTFOLIO_MAP.md)

### Layer 1: Decentralization-Critical
| Repository | Production Path | Last Session | W29 Status |
|------------|-----------------|--------------|------------|
| **conxian-nexus** | main (Mainnet) | ⏳ Not reviewed | - |
| **conxian-gateway** | main (Mainnet) | 2026-07-20 ⚠️ | ⚠️ P0 status corrected; #219 branch pending review/merge |

### Layer 2: User & Application Surface
| Repository | Production Path | Last Session | W29 Status |
|------------|-----------------|--------------|------------|
| **conxius-wallet** | main (Production) | ⏳ Not reviewed | - |
| **Conxian_UI** | main (Production) | ⏳ Not reviewed | - |
| **conxian-labs-site** | main (Public) | ⏳ Not reviewed | - |

### Layer 3: Shared Runtime & Developer Infrastructure
| Repository | Production Path | Last Session | W29 Status |
|------------|-----------------|--------------|------------|
| **lib-conxian-core** | main (Shared) | ⏳ Not reviewed | - |
| **lib-conclave-sdk** | main (Public) | ⏳ Not reviewed | - |
| **conxius-platform** | main (Internal) | ⏳ Not reviewed | - |
| **stacksorbit** | main (Internal) | ⏳ Not reviewed | - |

### Layer 4: Governance & Operating System
| Repository | Production Path | Last Session | W29 Status |
|------------|-----------------|--------------|------------|
| **conxian-business** | main (Strategic) | ⏳ Not reviewed | - |
| **.github** | main (Governance) | ⏳ Not reviewed | - |

---

## Cross-Repository Dependencies

### conxian-gateway Dependencies
```
lib-conxian-core  ←  required (L3 foundation)
conxius-wallet    →  depends on gateway API (L2)
lib-conclave-sdk  ←  shares types with SDK (L3)
```

### Dependency Status
| Dependency | Version | Status | Last Verified |
|-----------|---------|--------|---------------|
| lib-conxian-core | shared | ✅ Aligned | 2026-07-15 |
| conxius-wallet | API v1 | ✅ Compatible | 2026-07-15 |

---

## W29 Sprint Status (2026-07-15)

### conxian-gateway (W29 status corrected — 2026-07-20)

**Sprint Start Verification (2026-07-15):**
- ✅ Full repository verification complete
- ✅ Cargo update: 31 packages updated
- ✅ Clippy: 0 warnings
- ✅ Format: Check passed
- ✅ Tests: 158 tests passed
- ✅ Contamination guard: Clean
- ✅ Release hygiene: Verified
- ✅ Knowledge retention: 19 docs verified
- ✅ GitHub issues reviewed (37 total)
- ✅ Security advisories: None found

**P0 Implementation Status — continuity correction (2026-07-20):**
| # | Issue | Status |
|---|-------|--------|
| #236 SDK | Version drift + README | ✅ Fixed (0.1.4, Developer Preview) |
| #220 DLC CET | dlc-manager integration | ⚠️ Attempted in `453a15a`/`8ef9d05`, then reverted in `cb8b680` after API incompatibility/CI failures; remains open |
| #219 Groth16 | Verifier boundary | 🟡 Canonical contract, commitment binding, fixture, and BitVM handoff defined on `charlie/issue-219-groth16-boundary`; not merged and not cryptographic |
| #216 Babylon | BTC header SPV | 🟡 Implemented on PR #253 (`feat/216-babylon-header-chain`); pending review/merge |

**Historical DLC commits:**
- `453a15a` attempted the W29 P0 implementation, including `dlc_cet.rs` and `dlc-manager`.
- `8ef9d05` adjusted the attempted `dlc-manager` version.
- `cb8b680` removed `dlc_cet`, `dlc-manager`, and related wiring after API incompatibility/CI failures.
- `cc10886` recorded the superseded completion claim; see `docs/SESSION_SUMMARY_2026-07-20.md` for the correction.

**Babylon #216 status (2026-07-20):** PR #253 carries the BTC header-chain
query and verification implementation. Until that PR is merged, the
main-branch `babylon_adapter.rs` remains the stub that returned `0` without a
Bitcoin RPC and did not verify raw header hashes or parent links. Issue #216 is
not claimed merged or closed by this status entry.

**Liquid #218/#193 status (2026-07-20):**

- ✅ PR #257 merged the host-daemon Elements/Bitcoin peg-in/peg-out harness in
  `tests/liquid/`, including pinned daemon archives, checksum verification,
  real `claimpegin`/confidential-transfer/`sendtomainchain` coverage, and
  artifact upload.
- 🟡 PR #258 is the narrow follow-up for workflow/path hardening, configurable
  peg-in depth, verifier delegation coverage, and gateway API coverage.  It
  removes the duplicate Compose harness and keeps the merged host-daemon
  workflow as the CI entry point.
- ⚠️ The merged harness is not a production proof backend.  The
  `LiquidAdapter::verify_state_proof` boundary remains unwired and
  fail-closed; caller-supplied metadata is rejected rather than treated as a
  trusted Liquid state proof.

---

## Session History

| Date | Repository | Session Summary |
|------|------------|-----------------|
| 2026-07-20 | conxian-gateway | #216 continuity correction and Babylon header-chain implementation delivered in PR #253; pending merge. |
| 2026-07-20 | conxian-gateway | #219 boundary milestone: canonical contract, commitment binding, circuit/key association, BitVM handoff, deterministic fixture, and rejection tests completed locally; production backend remains open. |
| 2026-07-15 | conxian-gateway | W29 P0 implementation attempt recorded; later verification found the DLC CET attempt reverted in `cb8b680`, so #220 remains open. |
| 2026-07-15 | conxian-gateway | W29 sprint start. Full verification complete. |
| 2026-07-14 | conxian-gateway | W28 sprint close. Gap analysis of 11 issues. Session Continuity Protocol implemented. |
| 2026-07-14 | conxian-gateway | Initial gap analysis. AGENTS.md updated. 11 GitHub issues commented. |

---

## Verification Checklist

Before starting work on any repo, verify:
- [x] `git pull origin main` executed
- [x] `docs/SESSION_SUMMARY_*.md` exists
- [x] `docs/GAP_ANALYSIS_*.md` is current
- [x] All previous session artifacts present
- [x] PRs from previous session are merged

---

## W29 Remaining Goals

### Cross-Repo Actions (Pending)
- Review conxian-nexus (L1)
- Apply Session Continuity Protocol to other repos
- Verify lib-conxian-core alignment

### P1-P3 Issues (Future Sprints)
- #222 CI/CD coverage threshold
- #218/#193 Liquid harness ✅ host-daemon harness merged; PR #258 hardening follow-up open; production proof backend unwired
- #189 BitVM3 research monitoring

---

*This file is auto-maintained by agent sessions.*
*Last Major Update: 2026-07-20 (W29 P0 status corrected; #216 PR #253 pending merge)*
