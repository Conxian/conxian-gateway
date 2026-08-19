# Cross-Repository Status Snapshot

**Historical status snapshot:** 2026-07-22T14:42:43Z (observed via GitHub CLI; not live)
**Historical source commit:** `764859fd19c6b4305c0b7b9222c71493b3587177` (`origin/main`)
**Refresh rule:** Re-query GitHub before treating issue or PR counts as current;
this timestamped snapshot and its dated history are not live data.
**Current RGB correction — 2026-07-26:** Gateway `main` includes transactional
existing-contract updates and process-lifetime stash ownership. The current
#228 branch adds an opt-in BIP340 issuer public-key allowlist backend; controlled
runtime/import wiring and a state-changing signed Bitcoin/RGB regtest fixture
remain open. Historical table text below is retained as a dated snapshot.
**Historical Phase 4 implementation context before the PR #278 merge (local verification, 2026-07-22):**
`origin/main` is now at
[`d7032ab621ad038f247566f820ac664a6c8c071c`](https://github.com/Conxian/conxian-gateway/commit/d7032ab621ad038f247566f820ac664a6c8c071c),
and the bounded #245 slice is being prepared on
`charlie/issue-245-tracked-mempool-telemetry`. This branch context is not a
claim that the slice is merged into `main`.
**Current merged-main verification:** `origin/main` is at
[`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b),
the external merge commit for PR #278.
**Superseded observation:** The preceding pre-merge snapshot recorded PR #274
as open; it merged into `main` at 2026-07-22T14:25:01Z as commit
`764859fd19c6b4305c0b7b9222c71493b3587177`.
**Post-snapshot BitVM Phase 4 note — 2026-07-22:** The continuity checkpoint
predates the external merge of Gateway [PR #278](https://github.com/Conxian/conxian-gateway/pull/278)
on `charlie/issue-189-bitvm-fail-closed`. Its implementation commit is
[`114b857ed9d400beaf474cb68e7ac5f25ef58d78`](https://github.com/Conxian/conxian-gateway/commit/114b857ed9d400beaf474cb68e7ac5f25ef58d78);
the pre-documentation branch head was
[`c893cbb39ea9d680b229a89035ab38f29ed51b8b`](https://github.com/Conxian/conxian-gateway/commit/c893cbb39ea9d680b229a89035ab38f29ed51b8b).
GitHub subsequently reports an external merge at 2026-07-22T19:57:47Z as
[`96de9c0e976caf1dd3592593073d1f53e58bc91b`](https://github.com/Conxian/conxian-gateway/commit/96de9c0e976caf1dd3592593073d1f53e58bc91b);
Charlie did not merge PR #278. The Phase 4 documentation commit
[`e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005`](https://github.com/Conxian/conxian-gateway/commit/e761d3edfa7c7cbe6a4d9aa67e4e34229a7e3005)
was pushed after that merge and is not in merged `main`. PR #278 does not
resolve [Gateway #189](https://github.com/Conxian/conxian-gateway/issues/189),
which remains research-only. The current open cross-repository acceptance
issues are [Platform #1187](https://github.com/Conxian/conxius-platform/issues/1187),
[Nexus #169](https://github.com/Conxian/conxian-nexus/issues/169), and
[Enclave #202](https://github.com/Conxian/conxius-enclave-sdk/issues/202);
[Wallet #427](https://github.com/Conxian/conxius-wallet/issues/427),
[`.github` #41](https://github.com/Conxian/.github/issues/41), and
[Core #188](https://github.com/Conxian/lib-conxian-core/issues/188) remain
closed remediation evidence.
This documentation recovery is carried by a separate follow-up PR; that PR is
pending review/merge and is not part of `main` until it lands.
**Sprint:** W29 (2026-07-15 to 2026-07-25)  
**Maintained By:** Agent sessions

---

## Repository Inventory (Canonical — from PORTFOLIO_MAP.md)

### Layer 1: Decentralization-Critical
| Repository | Production Path | Last Session | W29 Status |
|------------|-----------------|--------------|------------|
| **conxian-nexus** | main (Mainnet) | ⏳ Not reviewed | - |
| **conxian-gateway** | main (Mainnet) | 2026-07-22 | Six Gateway issues remain open (#189, #220, #222, #228, #245, #247); PR #274 is merged in the timestamped snapshot; #216/#219/#236 are closed milestones and #189 remains research-only |

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

## Gateway status snapshot — 2026-07-22T14:42:43Z (not live)

**Verified base:** `origin/main` at [`764859fd19c6b4305c0b7b9222c71493b3587177`](https://github.com/Conxian/conxian-gateway/commit/764859fd19c6b4305c0b7b9222c71493b3587177).

- `gh issue list --state open` returns exactly six open Gateway issues:
  [#189](https://github.com/Conxian/conxian-gateway/issues/189),
  [#220](https://github.com/Conxian/conxian-gateway/issues/220),
  [#222](https://github.com/Conxian/conxian-gateway/issues/222),
  [#228](https://github.com/Conxian/conxian-gateway/issues/228),
  [#245](https://github.com/Conxian/conxian-gateway/issues/245), and
  [#247](https://github.com/Conxian/conxian-gateway/issues/247).
- The snapshot query found no open Gateway pull requests. PRs #268, #269,
  #270, #271, #272, #273, and [#274](https://github.com/Conxian/conxian-gateway/pull/274)
  are merged; PR #274 is titled “docs: correct ALEX evidence gate status and
  auth details” and merged at 2026-07-22T14:25:01Z. The source commit above
  includes the merged base through PR #274.

### Open Gateway issues

| Issue | Status at snapshot | Next evidence/acceptance slice |
|---|---|---|
| [#189](https://github.com/Conxian/conxian-gateway/issues/189) | Research-only; no stable BitVM3/BitVMX-GC SDK, production deployment, or production pairing backend verified | Keep the canonical evidence/triage report current; require stable revision, vectors, resource, protocol, security, and cross-repo gates |
| [#220](https://github.com/Conxian/conxian-gateway/issues/220) | Isolated DLC research/conformance/fixture slices merged; no Gateway runtime dependency or production CET path | Select manager/provider API only after independent offer/accept/sign/funding/CET/refund vectors and wallet/signing boundaries pass |
| [#222](https://github.com/Conxian/conxian-gateway/issues/222) | The audit follow-up adds an exact-tag-commit baseline for Rust, Node, Cargo audit, Gitleaks, Lightning coverage, deterministic artifact verification, and SLSA subjects; issue remains open pending merge, admin controls, and a live release rehearsal | Review/merge the narrow release-governance slice; configure required checks and the protected `release` environment; verify one tagged release |
| [#228](https://github.com/Conxian/conxian-gateway/issues/228) | Historical 2026-07-22 snapshot: RGB Phase 1 plus Phase 2 stockpile/import hardening merged in PRs #256/#261/#262; current correction above records later transactional/ownership work and the opt-in BIP340 branch | Add controlled issuer-policy wiring and a complete state-changing signed Bitcoin/RGB regtest fixture |
| [#245](https://github.com/Conxian/conxian-gateway/issues/245) | Research/observability; the Phase 4 working branch adds read-only Gateway-tracked mempool/fee-bump telemetry, but no BIP-110 integration or fee predictor; no fee multiplier/model rewrite justified | Add node/Core deployment and preflight provenance, network/node mempool and fee telemetry, durable RBF/CPFP outcome history, route-confidence calibration, and fee-model acceptance evidence |
| [#247](https://github.com/Conxian/conxian-gateway/issues/247) | Blocked/high-risk; ALEX quote/prepared-payload surfaces exist, while secure signer, exact contract/escrow semantics, and governance controls remain unresolved | Approve exact signer/contract/governance design, prove testnet controls, and reconcile the rehearsal/API contract |

### Closed/merged milestones that must not be listed as open

| Item | Verified state at snapshot |
|---|---|
| [Gateway #216](https://github.com/Conxian/conxian-gateway/issues/216) / [PR #253](https://github.com/Conxian/conxian-gateway/pull/253) | Issue closed; Babylon header-chain retrieval and bounded verification merged; EOTS/finality remain separate |
| [Gateway #219](https://github.com/Conxian/conxian-gateway/issues/219) / [PR #255](https://github.com/Conxian/conxian-gateway/pull/255) | Issue closed; backend-neutral Groth16 boundary and BitVM handoff merged; production pairing backend remains separate |
| [Gateway #236](https://github.com/Conxian/conxian-gateway/issues/236) | Issue closed; SDK package is `0.1.4` and README status is Developer Preview |
| [Gateway PR #258](https://github.com/Conxian/conxian-gateway/pull/258) | Merged 2026-07-20; Liquid harness hardening follow-up is not open |
| [Gateway PR #268](https://github.com/Conxian/conxian-gateway/pull/268) | Merged 2026-07-22; the timestamped BitVM evidence/triage report remains research-only |

The detailed score and evidence inventory are in
[`GAP_ANALYSIS_2026-07-22.md`](GAP_ANALYSIS_2026-07-22.md), and the #245 source
ledger is in
[`BIP110_FEE_MARKET_AND_ROUTING_2026-07-22.md`](research/BIP110_FEE_MARKET_AND_ROUTING_2026-07-22.md).

**Post-snapshot #189 handoff:** The canonical current report is
[`BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md`](research/BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md).
It records the Gateway BN254 envelope/error contract, the Nexus `Bls12_381`
compatibility mismatch, simulation success-path risk in Platform, fail-closed
Enclave proof routes, upstream release/tag/license/network evidence, the
candidate scorecard, and the remaining ownership decisions. These additions
do not rewrite the historical 14:42:43Z snapshot above.

### Gateway ownership boundary for #245

| Surface | Owner | Current responsibility |
|---|---|---|
| Core preflight | `lib-conxian-core` | Versioned, fail-closed BIP-110 size/preflight contract; no script interpretation or deployment verdict |
| Fee recommendation | `conxius-wallet` | Wallet-owned fee recommendation and transaction-construction/signing context; not inferred by Gateway telemetry |
| Network observation | `conxian-nexus` | Node/network observation and upstream chain/mempool evidence; not synthesized by Gateway tracked-state aggregates |
| Tracked operational telemetry | `conxian-gateway` | Read-only aggregation of persisted `TrackedMempoolTx` records, authenticated `/api/v1/bitcoin/mempool/telemetry`, and bounded `/metrics` gauges |

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
| #236 SDK | Version drift + README | ✅ Closed; fixed (0.1.4, Developer Preview) |
| #220 DLC CET | Research/API gate before CET implementation | ⚠️ HTTP oracle/event/key/outcome scaffold only; isolated Stage 1 vector normalization and rejection evidence is recorded, but there is still no cryptographic Gateway verification, DLC dependency, funding/CET/refund/adaptor-signature construction, or real bond construction. UUID/mock bond IDs only. See [`DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`](research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md) and [`DLC_STAGE1_CONFORMANCE_2026-07-22.md`](research/DLC_STAGE1_CONFORMANCE_2026-07-22.md). |
| #219 Groth16 | Verifier boundary | ✅ Canonical contract, commitment binding, fixture, and BitVM handoff merged in PR #255; no production cryptographic backend |
| #216 Babylon | BTC header SPV | ✅ Header-chain retrieval and bounded verification merged in PR #253; EOTS/finality extensions remain separate |
| #245 BIP-110 | Routing and fee-market impact | 🔬 Research/observability only; use the dated BIP-110 evidence ledger; no fee multiplier, model rewrite, or active-consensus claim |
| #247 ALEX | Settlement rail integration | 🔴 Blocked/high-risk pending secure signer, exact contract/escrow semantics, treasury controls, and governance/security acceptance |

**#189 / #216 / #219 status at the 2026-07-22 snapshot:**

- ✅ PR #259 merged the isolated, feature-gated BitVMX-CPU evaluator and its research-only contract tests.
- The canonical evidence and cross-repository triage record is [`docs/research/BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md`](research/BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md); the prior expansion remains the historical evidence record.
- 🔬 BitVM3, BitVMX-GC, and GOATNetwork/`bitvm2-gc` remain research/reference topics; no production BitVM3 or garbled-circuit adapter is present.
- 🟡 The Groth16 boundary is backend-neutral. The injected verifier/mock is not cryptographic Groth16 verification, and no production cryptographic verifier or settlement adapter is wired.
- PR #253 closed the #216 implementation gap and PR #255 closed the #219 boundary milestone; neither is a BitVM3/GC backend or production pairing implementation.

**#189 cross-repository triage (2026-07-22):**

- [conxius-platform #1187](https://github.com/Conxian/conxius-platform/issues/1187) — open P0: simulation defaults must be replaced or quarantined.
- [conxian-nexus #169](https://github.com/Conxian/conxian-nexus/issues/169) — open P1: bind real Arkworks verification to canonical BitVM state-transition semantics.
- [conxius-wallet #427](https://github.com/Conxian/conxius-wallet/issues/427) — **closed 2026-07-22**; retain the merged issue as historical remediation evidence.
- [Conxian/.github #41](https://github.com/Conxian/.github/issues/41) — **closed 2026-07-22**; retain the merged issue as historical documentation evidence.
- [lib-conxian-core #188](https://github.com/Conxian/lib-conxian-core/issues/188) — **closed 2026-07-22**; fail-closed boundary work is recorded in the Core tree observed for this snapshot.
- [conxius-enclave-sdk #202](https://github.com/Conxian/conxius-enclave-sdk/issues/202) — open P0: complete independent security/release acceptance evidence.

The Platform #1187 and Nexus #169 issues remain open. The Wallet #427,
`.github` #41, and Core #188 rows above replace the older open-state snapshot;
the BitVM research conclusions themselves are unchanged.

**Historical DLC commits:**
- `453a15a` attempted the W29 P0 implementation, including `dlc_cet.rs` and `dlc-manager`.
- `8ef9d05` adjusted the attempted `dlc-manager` version.
- `cb8b680` removed `dlc_cet`, `dlc-manager`, and related wiring after API incompatibility/CI failures.
- `cc10886` recorded the superseded completion claim; see `docs/SESSION_SUMMARY_2026-07-20.md` for the correction.

**DLC research alignment (2026-07-22):** The canonical source ledger,
mainnet-evidence policy, SDK comparison, readiness gates, and the isolated
Stage 1 conformance checkpoint are recorded in
[`docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`](research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md)
and [`docs/research/DLC_STAGE1_CONFORMANCE_2026-07-22.md`](research/DLC_STAGE1_CONFORMANCE_2026-07-22.md).
The checkpoint does not authorize a dependency addition, custody, CET/runtime
integration, or a mainnet claim.

**Babylon #216 status (2026-07-22; supersedes the pending note below):** PR #253
merged the BTC header-chain query and bounded verification implementation. The
`babylon_adapter.rs` tree state observed in this snapshot has Bitcoin RPC-backed tip/mainchain queries,
header parsing, parent-link and cumulative-work checks, and bounded traversal.
EOTS, full Babylon finality, and other non-goals remain separate.

**Liquid #218/#193 status (2026-07-20):**

- ✅ PR #257 merged the host-daemon Elements/Bitcoin peg-in/peg-out harness in
  `tests/liquid/`, including pinned daemon archives, checksum verification,
  real `claimpegin`/confidential-transfer/`sendtomainchain` coverage, and
  artifact upload.
- ✅ PR #258 merged the narrow follow-up for workflow/path hardening,
  configurable peg-in depth, verifier delegation coverage, and gateway API
  coverage. It removed the duplicate Compose harness and kept the merged
  host-daemon workflow as the CI entry point.
- ⚠️ The merged harness is not a production proof backend.  The
  `LiquidAdapter::verify_state_proof` boundary remains unwired and
  fail-closed; caller-supplied metadata is rejected rather than treated as a
  trusted Liquid state proof.

---

## Session History

> **Supersession note (2026-07-22):** The historical rows below preserve the
> state recorded before PRs #253, #255, #258, #267, and #268 merged. The
> timestamped status sections above are authoritative for #216, #219, #189,
> #245, #247, and the cross-repository issue states as observed on 2026-07-22.

| Date | Repository | Session Summary |
|------|------------|-----------------|
| 2026-07-21 | conxian-gateway | #189 research expansion: evaluator PR #259 verified as merged; BitVM3/GC remain research-only; Groth16 boundary remains non-cryptographic. |
| 2026-07-20 | conxian-gateway | #216 continuity correction and Babylon header-chain implementation delivered in PR #253; pending merge. |
| 2026-07-20 | conxian-gateway | #219 boundary milestone: canonical contract, commitment binding, circuit/key association, BitVM handoff, deterministic fixture, and rejection tests completed locally; production backend remains open. |
| 2026-07-15 | conxian-gateway | W29 P0 implementation attempt recorded; later verification found the DLC CET attempt reverted in `cb8b680`, so #220 remains open. |
| 2026-07-22 | conxian-gateway | DLC ecosystem, SDK, paper, and mainnet-evidence research aligned; CET implementation and dependency selection remain gated by the canonical readiness document. |
| 2026-07-15 | conxian-gateway | W29 sprint start. Full verification complete. |
| 2026-07-14 | conxian-gateway | W28 sprint close. Gap analysis of 11 issues. Session Continuity Protocol implemented. |
| 2026-07-14 | conxian-gateway | Initial gap analysis. AGENTS.md updated. 11 GitHub issues commented. |

---

## Verification Checklist

Before starting work on any repo, verify:
- [x] `git pull origin main` executed
- [x] `docs/SESSION_SUMMARY_*.md` exists
- [x] `docs/GAP_ANALYSIS_*.md` timestamped snapshots are present
- [x] All previous session artifacts present
- [x] PRs from previous session are merged

---

## W29 Remaining Goals

### Cross-Repo Actions (Pending)
- Review conxian-nexus (L1)
- Apply Session Continuity Protocol to other repos
- Verify lib-conxian-core alignment

### P1-P3 Issues (Future Sprints)
- #222 release-governance implementation prepared; merge, admin ruleset/environment configuration, live release rehearsal, and Cargo publication prerequisites remain
- #218/#193 Liquid harness ✅ host-daemon harness and PR #258 hardening merged; production proof backend unwired
- #189 BitVM3/BitVMX research monitoring; canonical evidence/triage refresh in `docs/research/BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md`; no production GC or cryptographic verifier

---

*This file is maintained by agent sessions as a timestamped snapshot.*
*Last Major Update: 2026-07-22T14:42:43Z (timestamped #245/#222 audit snapshot; PR #274 merged at observation time; #216/#219 milestones, #258, #268, #272, #273, and #274 merged; six Gateway issues open)*

---

## Session Update — 2026-08-19 (Session 52)
- **G-FI2 Closed**: Shipped ISO 20022 `pacs.008.001.08` customer credit transfer XML builder, validator, compliance normalization, and API endpoint (`/api/v1/fiat/pacs008/generate`).
- **Research Expansion**: Updated candidate matrices, gap analysis, and cross-repo status logs for end-to-end audit continuity across Conxian repositories.
