# Cross-Repository Status Dashboard

**Last Updated:** 2026-07-14  
**Sprint:** W28 (2026-07-07 to 2026-07-18)  
**Maintained By:** Agent sessions

---

## Repository Inventory

| Repository | Layer | Production Path | Current Version | Last Session |
|------------|-------|-----------------|-----------------|--------------|
| conxian-gateway | L1 | main (mainnet) | 0.1.4 | 2026-07-14 |
| conxian-nexus | L1 | main (mainnet) | - | - |
| conxius-wallet | L2 | main (prod) | - | - |
| Conxian_UI | L2 | main (prod) | - | - |
| conxian-labs-site | L2 | main (public) | - | - |
| lib-conxian-core | L3 | main (shared) | - | - |
| lib-conclave-sdk | L3 | main (public) | - | - |
| conxius-platform | L3 | main (internal) | - | - |
| stacksorbit | L3 | main (internal) | - | - |
| conxian-business | L4 | main (strategic) | - | - |

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

## Current Sprint Focus (W28)

### conxian-gateway
**Priority Issues:**
- #236: TypeScript SDK npm publish (P0)
- #220: DLC CET construction (P1)
- #219: Groth16 verifier boundary (P1)
- #216: Babylon BTC header-chain (P1)

**Blockers:**
- #219: Requires ark_groth16 dependency evaluation
- #220: Requires dlc-manager crate evaluation

### conxian-nexus
*(To be updated when repo is accessed)*

---

## Session History

| Date | Repository | Session Summary |
|------|------------|-----------------|
| 2026-07-14 | conxian-gateway | Full gap analysis of 11 open issues. Created `docs/GAP_ANALYSIS_2026-07-14.md`. Posted gap analysis to all 11 GitHub issues. Updated AGENTS.md with Session Continuity Protocol. |
| - | - | - |

---

## Verification Checklist

Before starting work on any repo, verify:
- [ ] `git pull origin main` executed
- [ ] `docs/SESSION_SUMMARY_*.md` exists
- [ ] `docs/GAP_ANALYSIS_*.md` is current
- [ ] All previous session artifacts present

---

## Notes for Next Session

### conxian-gateway Next Steps
1. Fix SDK version drift (#236) — `package.json: 0.1.0` → `0.1.4`
2. Fix SDK README claim (#236) — Remove "Production Ready"
3. Add dlc-manager dependency (#220)
4. Define Groth16 verifier boundary (#219)
5. Implement Babylon BTC header-chain (#216)

### Cross-Repo Actions Needed
- [ ] Create SPRINT_REVIEW for W27 (if not done)
- [ ] Verify conxian-nexus session state
- [ ] Update lib-conxian-core if types changed

---

*This file is auto-maintained by agent sessions.
Do not manually edit — let sessions update this file.*
