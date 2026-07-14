# Sprint Session Protocol — Organizational Standard

**Version:** 1.0  
**Date:** 2026-07-14  
**Applies To:** All Conxian-Labs Repositories  
**Sprint Cadence:** Bi-weekly (2 weeks)

---

## Overview

This protocol ensures production-grade continuity across all Conxian-Labs repositories by enforcing systematic verification at the start of each session and comprehensive state documentation at the end of each sprint.

### Goals
1. **No work lost** between sessions or sprints
2. **Full traceability** from issue → implementation → verification
3. **Cross-repo alignment** maintained automatically
4. **Audit trail** for all agent activities

---

## Part 1: Per-Session Protocol (Every Session)

### Entry Checklist
Before any work begins, verify:

```bash
# 1. Pull latest state
git pull origin main
git log --oneline -3

# 2. Check for session artifacts from previous sessions
ls -la docs/SESSION_SUMMARY_*.md 2>/dev/null || echo "No prior sessions"

# 3. Verify critical files exist
ls -la docs/GAP_ANALYSIS_*.md 2>/dev/null || echo "No gap analysis"

# 4. Run verification suite
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>/dev/null || true
cargo test --workspace 2>/dev/null || true
python3 scripts/verify_contamination_guard.py 2>/dev/null || true
```

### If Previous Work Is Missing
1. **STOP** — Do not proceed
2. Report what was expected vs. what exists
3. Check git history: `git log --all --oneline --decorate | head -20`
4. Check GitHub issues for recent activity
5. Document gap in new `docs/SESSION_SUMMARY_*.md`
6. Only proceed after verification or explicit user approval

### During Session
- Document every significant decision
- Update `docs/SESSION_SUMMARY_*.md` as you progress
- Post status updates to relevant GitHub issues
- Run verification suite before commits

### Exit Protocol
Before session ends:

```bash
# 1. Update session summary
#    - What was done
#    - What remains
#    - Files created/modified
#    - GitHub issues updated

# 2. Commit all changes (if any)
git add -A && git commit -m "Session: $(date +%Y-%m-%d) - [brief description]"

# 3. Push to remote
git push origin HEAD

# 4. Final verification
ls -la docs/SESSION_SUMMARY_*.md
```

---

## Part 2: Sprint Boundary Protocol (End of Sprint)

### Sprint Close Checklist

#### Repository-Level
- [ ] All completed issues have been closed with verification comments
- [ ] `docs/SPRINT_REVIEW_YYYY-MM-DD.md` created
- [ ] `docs/GAP_ANALYSIS_*.md` updated if new gaps found
- [ ] All PRs merged to main
- [ ] No uncommitted changes

#### Cross-Repository Sync
- [ ] Check dependent repos for state changes
- [ ] Update inter-repository dependency notes
- [ ] Verify version alignment across workspace
- [ ] Sync documentation that spans repos

### Sprint Review Document Template

```markdown
# Sprint Review — YYYY-MM-DD to YYYY-MM-DD

## Sprint Goals
- [Goal 1]
- [Goal 2]

## Completed
| Issue | Description | Verification |
|-------|-------------|--------------|
| #XXX | Task desc | Verified by [who] |

## In Progress
| Issue | Description | Blocker |
|-------|-------------|---------|
| #XXX | Task desc | None/Explain |

## Next Sprint Priorities
1. [Priority 1]
2. [Priority 2]

## Cross-Repo Dependencies
- [Repo A]: [Status]
- [Repo B]: [Status]

## Risks & Blockers
| Risk | Mitigation |
|------|------------|
| [Risk] | [Action] |

## Verification Checklist
- [ ] All tests pass
- [ ] CI/CD green
- [ ] Gap analysis current
- [ ] Session summaries documented
```

---

## Part 3: Organizational Agent Memory

### Per-Repository Memory (in AGENTS.md)

Each repository must maintain:

```markdown
## Repository State (Sprint N, YYYY-MM-DD)

### Last Session Summary
- Date: YYYY-MM-DD
- Completed: [list]
- Next: [list]

### Cross-Repo Dependencies
- repo-A: [version/status]
- repo-B: [version/status]

### Current Sprint Focus
- P0: [issue #]
- P1: [issue #]
- P2: [issue #]
```

### Central Organizational Memory

Maintain a central repo (e.g., `conxian-labs/.github`) with:

```
conxian-labs/.github/
├── AGENTS.md                    # Org-wide instructions
├── SPRINT_PROTOCOL.md          # This document
├── REPOS.md                    # All repo inventory
├── SPRINT_HISTORY/
│   ├── SPRINT-2026-W28.md
│   ├── SPRINT-2026-W27.md
│   └── ...
└── CROSS_REPO_STATUS.md        # Live cross-repo state
```

---

## Part 4: Issue & PR Templates

### Session Progress Issue Label

Use label: `session-progress`

When working an issue across multiple sessions:
- Add label `session-progress`
- Post session updates as comments
- Remove label when issue is closed

### PR Description Template

```markdown
## Session Context
- Previous session: [link/commit]
- Sprint: [Sprint-N]
- Verified: [ ] CI [ ] Tests [ ] Manual

## Changes
- [ ] Files modified
- [ ] Tests added
- [ ] Documentation updated

## Verification
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `python3 scripts/verify_contamination_guard.py`

## Related Issues
- Closes #XXX
- Related to #YYY
```

---

## Part 5: Automation Suggestions

### GitHub Actions for Sprint Hygiene

```yaml
# .github/workflows/sprint-hygiene.yml
name: Sprint Hygiene Check

on:
  schedule:
    - cron: '0 8 * * MON'  # Monday 8 AM
  workflow_dispatch:

jobs:
  verify-previous-sprint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check session summaries
        run: |
          echo "=== Sprint Hygiene Report ==="
          echo "Recent commits:"
          git log --oneline -5
          echo ""
          echo "Session summaries:"
          ls -la docs/SESSION_SUMMARY_*.md 2>/dev/null || echo "None found"
          echo ""
          echo "Open issues needing attention:"
          # Filter issues with session-progress label
          gh issue list --label session-progress --state open --limit 10
```

### Weekly Cross-Repo Sync Automation

```yaml
# Weekly sync check
name: Cross-Repo Sync
on:
  schedule:
    - cron: '0 9 * * FRI'  # Friday 9 AM
jobs:
  sync-report:
    runs-on: ubuntu-latest
    steps:
      - name: Check all Conxian repos
        run: |
          ORG="Conxian"
          REPOS=$(gh repo list $ORG --json name --jq '.[].name')
          echo "=== Cross-Repo Status ==="
          for repo in $REPOS; do
            echo "--- $repo ---"
            LAST_COMMIT=$(gh api repos/$ORG/$repo/commits --jq '.[0].sha' 2>/dev/null | head -c 7)
            OPEN_ISSUES=$(gh issue list --repo $ORG/$repo --state open --limit 1 --json number --jq 'length')
            echo "Last commit: $LAST_COMMIT"
            echo "Open issues: $OPEN_ISSUES"
          done
```

---

## Part 6: Checklist Summary

### Daily Session (Per Repo)
- [ ] Pull latest
- [ ] Check SESSION_SUMMARY_*.md
- [ ] Verify prior work exists
- [ ] Run verification suite
- [ ] Update session summary
- [ ] Push before end

### Sprint Close (Per Repo)
- [ ] All issues closed/verified
- [ ] SPRINT_REVIEW_*.md created
- [ ] GAP_ANALYSIS updated
- [ ] No uncommitted changes
- [ ] Pushed to remote

### Sprint Planning (Cross-Repo)
- [ ] Review previous sprint in all repos
- [ ] Check cross-repo dependencies
- [ ] Update AGENTS.md in each repo
- [ ] Sync central org memory

---

## Appendix: Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-07-14 | Initial protocol |

---

*This protocol is organizational standard for Conxian-Labs. 
All repositories must implement the per-session protocol.
Sprint boundary protocol applies to all production repos.*
