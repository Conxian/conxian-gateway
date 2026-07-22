# Conxian Gateway: Agent Instructions

You are working on the **Conxian Gateway**, an institutional-grade Rust middleware designed for high-performance Bitcoin/Stacks state logic and enterprise compliance.

---

## 🚨 CRITICAL: Session Continuity Protocol

**This is a production-grade repository. Every session MUST verify previous work before proceeding.**

### Session Start Checklist
Before beginning any work, you MUST:

1. **Pull latest code**: `git pull origin main`
2. **Check session artifacts**: Look for `docs/SESSION_SUMMARY_*.md` files from previous sessions
3. **Verify prior deliverables**: If previous session created files or posted comments, verify they exist and are correct
4. **Check open issues**: Review any issues that were being worked on in the previous session
5. **Read gap analysis**: Check `docs/GAP_ANALYSIS_*.md` for alignment status
6. **Review sprint status**: Check `docs/SPRINT_REVIEW_*.md` and `docs/CROSS_REPO_STATUS.md`

### Verification Commands
```bash
# Pull and verify state
git pull origin main
git log --oneline -5  # Check recent commits
ls -la docs/*.md       # Check for session summaries

# Run verification suite
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
python3 scripts/verify_contamination_guard.py
```

### Sprint & Org-Wide Protocol
For complete organizational protocol, see:
- `docs/SPRINT_SESSION_PROTOCOL.md` — Org-wide sprint & session standards
- `docs/CROSS_REPO_STATUS.md` — Live dashboard of all Conxian-Labs repos
- `docs/SPRINT_REVIEW_*.md` — Sprint boundary documentation

### If Previous Session Work is Missing or Broken
- **STOP** and report what was expected vs. what exists
- Do NOT continue with new work until verification is complete
- Document the gap in a new `docs/SESSION_SUMMARY_*.md`

---

## Current State (2026-07-22, updated)
- **Status Audit**: Holistic review of Nexus/Gateway alignment complete (CON-1353).
- **Protocol Drift**: Resolved — Fedimint, Citrea, and Strata adapters implemented and in production paths.
- **RGB G-1385 (Phase 1)**: StashResolver delivered (commit `124d17e`) with `rgb-std` v0.12.0-rc.3 + `bp-esplora` v0.12.0-rc.3 behind `rgb-native` feature. Phase 2 (ContractVerify, consignment) blocked on rgb-std ecosystem stabilization.
- **PR #233 (G-1389)**: Tech debt reduction merged (`5e6613e`). Includes Fedimint/Citrea/Strata adapters, Redis coordination module, auth middleware timing stubs, reqwest 0.13 upgrade, dead_code cleanup. Citrea adapter moved from `bitcoin/` to `ntt/`.
- **Gap Analysis (2026-07-14, DLC correction 2026-07-22)**: Full review of 11 open issues vs. codebase complete. Report at `docs/GAP_ANALYSIS_2026-07-14.md`. Key findings and current corrections:
  - ✅ #236 SDK: Version/documentation alignment is fixed in the tree — `packages/client-sdk/package.json` is `0.1.4` and the README says "Developer Preview"; the dated gap-analysis entry is retained as historical context
  - ⚠️ #220 DLC: HTTP oracle scaffold exists, but cryptographic attestation verification, CET/funding/refund construction, and dependency selection remain open; see `docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`
  - ⚠️ #219 Groth16: a partial trait skeleton exists on `main`; the canonical contract, BitVM handoff, fixture, and rejection tests are implemented on the focused `charlie/issue-219-groth16-boundary` branch and are not merged yet
  - ❌ #216 Babylon: BTC header-chain returns `0`, no SPV implementation
- **Sprint Protocol (2026-07-14)**: Session Continuity Protocol implemented. All agent sessions now verify prior work before proceeding. See:
  - `docs/SPRINT_SESSION_PROTOCOL.md` — Org-wide standards
  - `docs/CROSS_REPO_STATUS.md` — Live cross-repo dashboard
  - `docs/SPRINT_REVIEW_2026-W28.md` — Sprint W28 documentation
- **CI status**: All workflows green on main. `cargo-audit.yml` augmented with `.cargo/audit.toml` ignore list for transitive `rustls-webpki` CVEs.
- **P3 Sprint Review (commit `07c9508`)**: All review findings resolved — G-C6 verdict logic, signature verification for all machine providers, SystemTime→now_unix, inline test backend. 29 canton_m2m_tests pass; 158 workspace tests pass.
- **Strategic Research (2026-07-06)**: Canton Network & Machine Economy deep-dive complete. Key finding: "route without touch" — Conxian as sovereign routing layer between Canton's $6T+ institutional capital and Bitcoin's permissionless settlement. Machine Economy: $1.1B/month Lightning M2M volume, peaq 60+ DePINs, DIMO vehicle identity. See `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` and `docs/research/KNOWLEDGE_MAP.md`.
- **Issue #219 boundary update (2026-07-20)**: `internal/engine/src/bitcoin/groth16_verifier.rs` now defines the BN254 canonical statement/hash contract, witness-commitment public-input binding, circuit/key association, witness-privacy boundary, and deterministic test verifier. `bitvm_adapter.rs` parses and validates the envelope before delegating to an injected verifier. This remains a boundary milestone, not cryptographic Groth16 verification.

### Protocol Implementations (2026-07-05)
| Protocol | Status | File |
|---|---|---|
| NWC NIP-47 | тЬЕ Integrated | `internal/api/src/nwc_backend.rs` |
| Rootstock | тЬЕ Integrated | `internal/engine/src/ntt/rootstock_adapter.rs` |
| Babylon | Pending — header-chain/SPV remains unimplemented while PR #253 is open | `internal/engine/src/bitcoin/babylon_adapter.rs` |
| BitVM2 | Partial — metadata adapter plus explicit Groth16 handoff on focused branch | `internal/engine/src/bitcoin/bitvm_adapter.rs` |
| RGB | тЬЕ v0.12 + Stash (P1) | `internal/engine/src/bitcoin/rgb_adapter.rs` + `rgb_native.rs` + `rgb_stash.rs` |
| Liquid | тЬЕ Integrated | `internal/engine/src/bitcoin/liquid_adapter.rs` |
| Citrea | тЬЕ Integrated | `internal/engine/src/ntt/citrea_adapter.rs` |
| RISC Zero | ЁЯЯб Unwired | `internal/engine/src/bitcoin/risc0_verifier.rs` |
| Fedimint | тЬЕ Integrated | `internal/engine/src/bitcoin/fedimint_adapter.rs` |
| Strata | тЬЕ Testnet | `internal/engine/src/bitcoin/strata_adapter.rs` |
| BitVMX GC | ЁЯЯб Pending 2026 | N/A |
| BRICS Pay | ЁЯЯб Research only | N/A |
| mBridge | ЁЯЯб Research only | N/A |
| Canton Network | 🟡 Research — "route without touch" (G-C7 P3) | `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` |
| Machine Economy (peaq/DIMO/DePIN) | 🟡 Research — BTC/LN gateway opportunity (G-C8 P3) | `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` |
| UCV-2 | 🟡 Architecture — Cross-ledger settlement protocol | `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` |

## Core Philosophy
- **Sovereignty**: All code must prioritize non-custodial logic and user sovereignty.
- **Institutional Grade**: Maintain SLA-grade interfaces, high-performance async Rust, and robust error handling.
- **Compliance Pipe**: The gateway is a pass-through for compliance data (ZKC), not a storage for PII.

## Technical Standards
- **Rust Edition**: 2021
- **MSRV**: 1.85 (toolchain: 1.96.0)
- **Framework**: Axum (HTTP), Tokio (Runtime)
- **Security**: Mandatory Bearer token auth for sensitive endpoints.
- **Observability**: Prometheus metrics and structured tracing are required for all new modules.
- **Persistence**: Any stateful component must use the atomic persistence layer.

## Verification Protocol
Before submitting changes, you MUST:
1. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
2. Run `cargo fmt --all -- --check`
3. Run all tests: `cargo test --workspace` AND `cargo test --workspace --features mock-integrations`
4. Run `pnpm install && pnpm build && pnpm test`
5. Verify health check: `GET /api/v1/health` returns HTTP 200 with `{"status":"ok"}`.
6. Run `python3 scripts/verify_contamination_guard.py`

## Module Map
- `/cmd/gateway`: Entry point, configuration, and wiring.
- `/internal/engine`: Blockchain listeners and RPC clients.
- `/internal/api`: REST interface, handlers, and auth middleware.
- `/internal/compliance`: ZKC (Zero-Knowledge Compliance) and MVCR logic.
- `/pkg/conxian-core`: Shared models, error types, and persistence logic.

## CI/CD Pipelines
- **rust-ci.yml**: Format, clippy, test (incl. mock-integrations), release build.
- **lightning-coverage.yml**: Lightning scoped coverage gate (тЙе90%).
- **cargo-audit.yml**: Weekly dependency audit.
- **secret-scan.yml**: Gitleaks secret scanning.
- **node-ci.yml**: TypeScript build + vitest (client-sdk only).
- **release.yml**: Tag-triggered GitHub Release with SBOM (CycloneDX) and SLSA L3 provenance.

## Known Gaps (2026-07-14 snapshot; current corrections noted)
- [x] #228: RGB stash resolver (G-1385 P1) — merged `124d17e`
- [x] #233 (G-1389): Tech debt reduction — merged `5e6613e`
- [x] G-1276: Redis AUTH + token expiry — merged `2ef6df1`
- [x] G-1380: SBOM and Provenance to release workflow — merged `19181c5`
- [x] #236: SDK version drift + README overclaim — fixed in tree (`packages/client-sdk/package.json` is `0.1.4`; README says "Developer Preview"); issue state is tracked separately
- [ ] #220: DLC CET construction — research/API spike required before selecting `rust-dlc` or DDK; see `docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`
- [ ] #219: Groth16 cryptographic backend — boundary contract and deterministic fixture handoff implemented on `charlie/issue-219-groth16-boundary`; not merged or cryptographic
- [ ] #216: Babylon BTC header-chain SPV — returns 0, needs implementation
- [ ] #189: BitVMX GC adapter — pending 2026 garbled circuits release (see research below)
- [x] #231: BRICS Pay — DCMS settlement rail (closed — research complete, no adapter needed)
- [x] #232: mBridge — BIS multi-CBDC DLT (closed — research complete, observation only)

Full gap analysis: `docs/GAP_ANALYSIS_2026-07-14.md`

### Critical P0 Actions (W29 — historical approval list)
The #236 version and README corrections listed below are complete in the current
tree. This historical list is retained for continuity and does not imply that
those two fixes remain open.
1. **#236 SDK version** — ✅ Applied: `packages/client-sdk/package.json` is `0.1.4`
2. **#236 SDK README** — ✅ Applied: the status is "Developer Preview", not "Production Ready"
3. **Align DLC research and API gate** — Compare pinned `rust-dlc` v0.8.0 and DDK v1.1.2 in an isolated spike before any workspace dependency or CET implementation for #220
4. **Define Groth16 boundary** — Canonical contract and BitVM handoff implemented on the focused #219 branch; merge and add a real backend separately
5. **Implement Babylon SPV** — BTC header-chain for #216

**Status:** P0 items remain approved; #219's boundary milestone is implemented locally on 2026-07-20, while a production cryptographic backend remains out of scope. DLC remains research/status alignment only until the gates in `docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md` pass.

### New Strategic Gaps (2026-07-06)
- [x] G-C1: CBTC non-custodial verification — conxian-core types + `POST /api/v1/canton/cbtc/verify` handler with 6-point attestation check (commit pending)
- [x] G-C2: Machine identity DID extension — `MachineIdentity`, `MachineType`, `MachineIdentityResolutionRequest/Response` in conxian-core + `POST /api/v1/identity/resolve/machine` handler (commit pending)
- [x] G-C3: Lightning M2M settlement primitives — `SettlementSource::MachineToMachine`, `M2MSettlementRail`, `M2MSettlementRequest/Response` + `POST /api/v1/m2m/settle` handler routing through Lightning adapter (commit pending)
- [x] G-C4: Canton state translation adapter — `UniversalContractRef`, `CantonDomainRef`, `CantonStateTranslationRequest/Response` in conxian-core + `POST /api/v1/canton/state/translate` with Daml template-aware mapping (commit pending)
- [x] G-C5: Chainlink CCIP Canton connector — `CcipMessageRoute`, `CcipRouteRequest/Response`, sanctions-risk classification with escalation logic + `POST /api/v1/ccip/route` (commit pending)
- [x] G-C6: Machine RWA revenue verification — `MachineRwaRevenue`, `RevenueSource`, `MachineRwaVerificationRequest/Response` + `POST /api/v1/rwa/machine/verify-revenue` with 5-point verification check (commit pending)
- [ ] G-C7: Canton↔Bitcoin atomic swap engine — P3, Q1 2027
- [ ] G-C8: DePIN compliance ZKC — P3, Q1 2027

## Gap Research (2026-07-05 Refresh)

### #189: BitVMX GC (Garbled Circuits)
- **BitVMX-CPU**: Open source (Rust, MIT, FairgateLabs) — RISC-V emulation + Bitcoin script
- **BitVMX-GC**: Targeting 2026 release, currently closed source (Liam Eagen, Feb 2026)
- **GOATNetwork/bitvm2-gc**: Open source reference — Groth16 + DV-SNARK via GC, 10B gates
- **BitVM3 paper** (Robin Linus, Jul 2025): Theoretical foundation
- **Conxian posture**: Evaluate BitVMX-CPU now; monitor GOATNetwork/bitvm2-gc for POC; wait for BitVMX-GC public SDK
- **Citrea Groth16 adapter already shipped** (#192, `8d82062`) — same recursive proof pattern expected

### #231: BRICS Pay (DCMS Settlement)
- **DCMS spec v1.0**: 20K msgs/sec, distributed consensus, open-source planned
- **Timeline**: Pilots in Russia/India/South Africa (H1 2025), BRICS+ connectivity Q4 2026, CBDC integration Q3 2027
- **Classification**: Messaging standard, NOT a blockchain protocol — no adapter needed
- **Conxian posture**: Settlement rail identifier only; compliance pipeline (#203, #204) handles jurisdictional routing
- **UNIT ecosystem**: Gold-pegged + BRICS currency basket may need asset classification

### #232: mBridge (BIS Multi-CBDC DLT)
- **Scale**: $55.49B across 4,047 transactions; 95% in e-CNY
- **Architecture**: Permissioned DLT (HotStuff+), EVM-compatible, ISO 20022 payloads
- **Governance**: BIS exited Oct 2024; now PBOC/HKMA/BOT/CBUAE/SAMA consortium
- **Classification**: Permissioned-governance platform requiring central bank membership
- **Conxian posture**: Observer/compliance pass-through only; no adapter needed
- **Re-evaluate** if BIS/mBridge publishes public observer API

### Canton Network (Institutional Privacy DLT — updated 2026-07-06)
- **What**: Digital Asset's privacy-preserving DLT; Daml smart contracts; eUTXO model isomorphic to Bitcoin
- **Architecture**: ~780 validators, ~600 nodes (Dec 2025), Canton 3.5.6 (June 2026); participant nodes + synchronizers + 2PC
- **$344.83B** represented asset value (RWA.xyz, May 2026); DTCC, Franklin Templeton, J.P. Morgan Kinexys, HSBC Orion active
- **CBTC**: Wrapped Bitcoin via BitSafe — FROST threshold attestation (Kiln + Figment), validator-scoped privacy
- **LayerZero**: Live on Canton (March 2026) — connects to 165+ blockchains for institutional asset routing
- **Zenith**: Atomic swap engine (Canton↔Ethereum), emerged March 2026, Tier-1 Super Validator
- **Chainlink CCIP**: Data Streams integration guide published (requires Canton Party ID + DAR upload)
- **Conxian posture**: Observe-only routing layer; never run a Canton validator; translate Daml ACS → Bitcoin UTXO state
- **Opportunity**: Non-custodial capital routing between institutional Canton and sovereign Bitcoin/DePIN
- **Key constraint**: Canton is permissioned at application layer; Conxian routes without touching — verify, attest, never hold
- **Full research**: `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` + `docs/research/KNOWLEDGE_MAP.md`

### Machine Economy (DePIN, M2M, peaq — updated 2026-07-06)
- **What**: Machines owning wallets, paying machines, earning autonomously; DePIN = token-incentivized physical infrastructure
- **peaq**: 60+ DePINs across 22 industries, $180M TVL, 12K+ daily active devices; Machine RWA Framework (Registration→Issuance→Revenue→Compliance); Mastercard, Bosch, Tether QVAC integrations; x402 via thirdweb
- **Settlement rail**: Lightning Network — $1.1B/month volume, USDT via Taproot Assets, sub-cent fees, instant finality
- **Machine identity**: peaq DID, DIMO Vehicle ID, device pubkeys; Conxian extends existing BNS/ENS/World ID stack
- **Conxian posture**: Non-custodial M2M settlement routing via Lightning; machine identity verification; machine RWA revenue attestation
- **Monetization**: M2M routing fees (1-5 bps), identity attestation fees, Lightning channel liquidity leasing
- **Key principle**: Machines hold their own keys; Conxian routes and verifies, never custodies
- **Full research**: `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` + `docs/research/KNOWLEDGE_MAP.md`

## OpenHands SDK & Automation Capabilities

### SDK (Python)
- **Install**: `pip install openhands-sdk openhands-tools`
- **Core**: `Agent`, `Conversation`, `LLM`, `Tool`, `Skill`, `Workspace`
- **Key features**: File-based sub-agents, MCP integration, goal completion loops, agent delegation, persistence, iteration with critic, browser use, OpenTelemetry tracing
- **Remote**: Agent server (local, Docker, API sandbox, Apptainer, Cloud workspace)
- **GitHub integration**: PR review, TODO management, assign reviews

### Automations (Cloud/CLI)
- **Triggers**: Cron schedules or webhook events (GitHub, Linear, Slack, Stripe, custom)
- **Presets**: Prompt preset (natural language tasks) and Plugin preset (with extensions)
- **Custom scripts**: Deterministic Python with no LLM (for high-frequency/cost-sensitive tasks)
- **Repository cloning**: Auto-loads AGENTS.md and `.agents/skills/` from target repos
- **State persistence**: KV store for polling automations tracking last-processed state
- **Webhook alternatives**: Cron polling when deployment is not publicly reachable

### Automation Ideas for Conxian Gateway
1. **Daily CI monitor**: Cron→prompt preset to check all 6 workflows and report status to Slack
2. **Weekly dependency audit**: Cron→prompt preset to run `cargo audit` and file issues for new CVEs
3. **PR review agent**: Event→on `pull_request.labeled` with `openhands` label → auto-review
4. **Research issue updater**: Cron→prompt preset to refresh all research monthly (Canton, BitVMX, BRICS, mBridge, Machine Economy)
5. **Release note generator**: Event→on `release.published` → generate changelog from git history
6. **Canton ecosystem watcher**: Cron→monthly check Canton GitHub releases + CBTC attestation set changes

## Ethical Alignment
The Conxian Protocol is built to empower individuals and institutions within the Stacks/Bitcoin ecosystem. The dual-stack settlement architecture supports financial sovereignty across both Western and BRICS-aligned jurisdictions. Avoid any "dark patterns" or custodial shortcuts.

**Canton Principle**: Route without touching. Conxian serves as the sovereign membrane between institutional privacy DLTs and permissionless Bitcoin — verifying, attesting, translating — but never holding, controlling, or intermediating assets on either side.

**Machine Economy Principle**: Machines are sovereign economic actors. Conxian provides the identity, routing, and compliance infrastructure for autonomous M2M value exchange without ever taking custody of machine wallets or revenue streams.
