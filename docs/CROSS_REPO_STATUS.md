# Cross-Repository Status Dashboard

**Last Updated:** 2026-07-14  
**Sprint:** W28 → W29 Transition (2026-07-14)  
**Maintained By:** Agent sessions

---

## Repository Inventory

| Repository | Layer | Production Path | Current Version | Last Session |
|------------|-------|-----------------|-----------------|--------------|
| conxian-gateway | L1 | main (mainnet) | 0.1.4 | 2026-07-14 ✅ |
| conxian-nexus | L1 | main (mainnet) | - | ⏳ Not reviewed |
| conxius-wallet | L2 | main (prod) | - | ⏳ Not reviewed |
| Conxian_UI | L2 | main (prod) | - | ⏳ Not reviewed |
| conxian-labs-site | L2 | main (public) | - | ⏳ Not reviewed |
| lib-conxian-core | L3 | main (shared) | - | ⏳ Not reviewed |
| lib-conclave-sdk | L3 | main (public) | - | ⏳ Not reviewed |
| conxius-platform | L3 | main (internal) | - | ⏳ Not reviewed |
| stacksorbit | L3 | main (internal) | - | ⏳ Not reviewed |
| conxian-business | L4 | main (strategic) | - | ⏳ Not reviewed |

---

## Cross-Repository Dependencies

### conxian-gateway Dependencies
```
lib-conxian-core  ←  required
conxius-wallet    →  depends on gateway API
lib-conclave-sdk  ←  shares types with SDK
```

### Dependency Status
| Dependency | Version | Status | Last Verified |
|-----------|---------|--------|---------------|
| lib-conxian-core | shared | ✅ Aligned | 2026-07-14 |
| conxius-wallet | API v1 | ✅ Compatible | 2026-07-14 |

---

## W28 Sprint Summary

### conxian-gateway (W28 Complete ✅)

**Completed:**
- ✅ Session Continuity Protocol implemented (PR #243)
- ✅ Full gap analysis of 11 open issues
- ✅ Gap analysis posted to all GitHub issues
- ✅ AGENTS.md updated with verification checklist
- ✅ Sprint Session Protocol documented
- ✅ W29 Planning created (PR #244)

---

## W29 Sprint Planning (2026-07-21 to 2026-08-01)

### 🚨 ALL P0 ITEMS APPROVED FOR IMPLEMENTATION

| # | Issue | Priority | Action |
|---|-------|----------|--------|
| 1 | #236 SDK | P0 | Fix version drift + README claim |
| 2 | #220 DLC CET | P0 | Add dlc-manager, implement CET |
| 3 | #219 Groth16 | P0 | Define verifier boundary |
| 4 | #216 Babylon | P0 | Implement BTC header SPV |

---

## Session History

| Date | Repository | Session Summary |
|------|------------|-----------------|
| 2026-07-14 | conxian-gateway | W28 sprint close. Gap analysis of 11 issues. Session Continuity Protocol implemented. W29 planning with P0 approvals. PR #244 created. |
| 2026-07-14 | conxian-gateway | Initial gap analysis. AGENTS.md updated. 11 GitHub issues commented. |

---

## Verification Checklist

Before starting work on any repo, verify:
- [ ] `git pull origin main` executed
- [ ] `docs/SESSION_SUMMARY_*.md` exists
- [ ] `docs/GAP_ANALYSIS_*.md` is current
- [ ] All previous session artifacts present
- [ ] PRs from previous session are merged

---

## Notes for Next Session (W29)

### conxian-gateway Next Steps
1. ✅ PR #244 should be merged (W29 Planning)
2. Start #236 SDK Fix (version drift + README)
3. Start #220 DLC CET Construction
4. Start #219 Groth16 Verifier Boundary
5. Start #216 Babylon BTC Header-Chain

### Cross-Repo Actions Needed
- [ ] Review conxian-nexus repository
- [ ] Review conxius-wallet repository  
- [ ] Update lib-conxian-core if types changed
- [ ] Apply Session Continuity Protocol to other repos

---

*This file is auto-maintained by agent sessions.*
*Last Major Update: 2026-07-14 (W28 Sprint Close)*
