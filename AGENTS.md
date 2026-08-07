# Conxian Gateway: Agent Instructions

You are working on the **Conxian Gateway**, an institutional-grade Rust middleware designed for high-performance Bitcoin/Stacks state logic and enterprise compliance.

---

## Architecture Note: `conxian_core` vs `lib-conxian-core`

The Gateway workspace has its own `conxian_core` crate (`pkg/conxian-core/`)
that provides Gateway-specific types (settlement, Lightning, MuSig2, Alex
settlement, trust policy, persistence). This is **not** the same crate as
`lib-conxian-core` (at the monorepo root), which provides shared protocol
primitives (control models, verifier, signing, anchoring, chain adapters).

| Crate | Scope | Location |
|-------|-------|----------|
| `conxian_core` | Gateway-local types & utilities | `pkg/conxian-core/` |
| `lib-conxian-core` | Shared protocol primitives | `lib-conxian-core/` |
| `conxius-enclave-sdk` | Hardware enclave & production signing | `conxius-enclave-sdk/` |

When adding protocol-level capabilities (BitVM2 verification, DLC, FROST,
control model enforcement), prefer `lib-conxian-core` types. When adding
Gateway-specific operational types (persistence, trust policies, settlement
envelopes), extend `conxian_core`.

### Contract Bridge (Session 47 — Aug 2026)
The Gateway now has a Clarity contract-call bridge at
`internal/engine/src/stacks/contract_bridge.rs`. This enables typed,
validated, and signed contract calls to the Conxian protocol's Clarity
contracts via the Stacks RPC layer. Canonical contract names are
enumerated for defense-in-depth validation.

### Sovereign Persistence (Session 48 — Aug 2026)
Multi-backend persistence at `internal/engine/src/persistence.rs` (118 lines):
- `SovereignBackend` enum: File (default), Tableland, Kwil
- Environment-driven selection via `GATEWAY_PERSISTENCE_BACKEND`
- Uses `lib_conxian_core::Persistence` trait for atomic transactional updates
- Designed for sovereignty: no single cloud provider dependency

### MRR Billing Engine (Session 48 — Aug 2026)
Usage-based billing at `internal/engine/src/billing.rs` (362 lines):
- Tiered pricing: self-hosted (zero-cost) vs managed (per-operation)
- Counters: relay messages, RWA verifications, settlement ops
- Daily aggregation → monthly billing periods
- JSON export for Stripe/accounting integration
- Base fee: $200/mo managed; per-op costs: $0.01–$0.10

### Settlement Rail Adapters (Session 48 — All Wired)
Every Bitcoin L2 settlement rail now has a Gateway adapter:

| Rail | Adapter | Location | Market Doc |
|------|---------|----------|------------|
| sBTC | SBTCBridge + Emily API | `stacks/sbtc.rs` (441 lines) | conxian_market: SETTLEMENT_RAILS.md §3, monitoring.md §1 |
| RGB | GatewayRgbAdapter | `bitcoin/rgb_adapter.rs` (201 lines) | SETTLEMENT_RAILS.md §4 |
| Babylon | StakingIntent adapter | `bitcoin/babylon_adapter.rs` | SETTLEMENT_RAILS.md §5, FUNDING §3.4 |
| Fedimint | FedimintMint adapter | `bitcoin/fedimint_adapter.rs` | SETTLEMENT_RAILS.md §6, monitoring.md §2 |
| Statechain (Spark) | Via enclave-sdk | `conxius-enclave-sdk/src/protocol/statechain.rs` | SETTLEMENT_RAILS.md §2 |
| Lightning | LightningAdapter | Via nexus compat bridge | SETTLEMENT_RAILS.md §7 |

> Monitoring specification: `conxian_market/docs/knowledge_base/monitoring.md` covers alert thresholds,
> Prometheus endpoints, and dashboard queries for all 6 rails.

### TrustTier Pricing (Session 48)
4-tier pricing model defined in market `trust_tier_pricing.md`:
- **ObserverOnly**: Free (no settlement)
- **Expedient**: 2% flat (Fedimint, Lightning, ALEX)
- **Managed**: 2% + 0.5% premium (Statechain, sBTC, RGB, Babylon)
- **Strict**: Negotiated (all rails, TEE+ZK)

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
- `docs/CROSS_REPO_STATUS.md` — Timestamped snapshot of all Conxian-Labs repos; refresh before relying on it
- `docs/SPRINT_REVIEW_*.md` — Sprint boundary documentation

### If Previous Session Work is Missing or Broken
- **STOP** and report what was expected vs. what exists
- Do NOT continue with new work until verification is complete
- Document the gap in a new `docs/SESSION_SUMMARY_*.md`

---

## Current State (2026-08-07 — PR #324 merged)
- **Status Audit**: Holistic review of Nexus/Gateway alignment complete (CON-1353).
- **Protocol Drift**: Resolved — Fedimint, Citrea, and Strata adapters implemented and in production paths.
- **RGB G-1385 (current correction, 2026-07-26)**: StashResolver, the pinned `rgb-persist-fs::StockpileDir`/consignment boundary, transactional existing-contract updates, and process-lifetime stash ownership are merged. An opt-in fail-closed BIP340 public-key allowlist backend is implemented on `charlie/issue-228-bip340-issuer-policy`; controlled runtime/import wiring and a complete state-changing signed Bitcoin/RGB regtest fixture remain open. `RejectIssuerSignatures` remains the runtime/default policy.
- **PR #233 (G-1389)**: Tech debt reduction merged (`5e6613e`). Includes Fedimint/Citrea/Strata adapters, Redis coordination module, auth middleware timing stubs, reqwest 0.13 upgrade, dead_code cleanup. Citrea adapter moved from `bitcoin/` to `ntt/`.
- **Gap Analysis (2026-07-22)**: Current six-issue inventory, weighted ranking, and evidence-backed acceptance slices are recorded in `docs/GAP_ANALYSIS_2026-07-22.md`; `docs/GAP_ANALYSIS_2026-07-14.md` remains the dated historical snapshot. Key findings and current corrections:
  - ⚠️ #222 CI/CD: Phase 3 release-governance implementation is prepared on the audit branch — fail-closed tag/version validation, production binary packaging, checksums, normalized CycloneDX SBOM, SLSA subjects, protected release job, and rollback runbook are present; merge, admin ruleset/environment configuration, a live release rehearsal, and Cargo publication prerequisites remain
  - ✅ #236 SDK: Version/documentation alignment is fixed in the tree — `packages/client-sdk/package.json` is `0.1.4` and the README says "Developer Preview"; the dated gap-analysis entry is retained as historical context
  - ⚠️ #220 DLC: HTTP oracle/event/key/outcome scaffold; cryptographic Schnorr verification ✅ implemented (Session 50); funding/CET/refund/adaptor-signature construction, and real bond construction remain open. The current scaffold uses UUID/mock bond IDs only; see `docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`
  - ⚠️ #219 Groth16: canonical BN254 contract, BitVM handoff, fixture, and rejection tests merged in PR #255; production cryptographic backend remains open
  - [x] #216 Babylon: BTC header-chain retrieval and bounded SPV-style verification merged in PR #253; EOTS/finality extensions remain separate
- **Sprint Protocol (2026-07-14)**: Session Continuity Protocol implemented. All agent sessions now verify prior work before proceeding. See:
  - `docs/SPRINT_SESSION_PROTOCOL.md` — Org-wide standards
  - `docs/CROSS_REPO_STATUS.md` — Timestamped cross-repo snapshot; refresh before relying on it
  - `docs/SPRINT_REVIEW_2026-W28.md` — Sprint W28 documentation
- **CI status**: All workflows green on main. `cargo-audit.yml` augmented with `.cargo/audit.toml` ignore list for transitive `rustls-webpki` CVEs.
- **P3 Sprint Review (commit `07c9508`)**: All review findings resolved — G-C6 verdict logic, signature verification for all machine providers, SystemTime→now_unix, inline test backend. 29 canton_m2m_tests pass; 158 workspace tests pass.
- **Strategic Research (2026-07-06)**: Canton Network & Machine Economy deep-dive complete. Key finding: "route without touch" — Conxian as sovereign routing layer between Canton's $6T+ institutional capital and Bitcoin's permissionless settlement. Machine Economy: $1.1B/month Lightning M2M volume, peaq 60+ DePINs, DIMO vehicle identity. See `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` and `docs/research/KNOWLEDGE_MAP.md`.
- **Issue #219 boundary update (2026-07-20)**: `internal/engine/src/bitcoin/groth16_verifier.rs` now defines the BN254 canonical statement/hash contract, witness-commitment public-input binding, circuit/key association, witness-privacy boundary, and deterministic test verifier. `bitvm_adapter.rs` parses and validates the envelope before delegating to an injected verifier. This remains a boundary milestone, not cryptographic Groth16 verification.

### Protocol Implementations (2026-07-22)
| Protocol | Status | File |
|---|---|---|
| NWC NIP-47 | тЬЕ Integrated | `internal/api/src/nwc_backend.rs` |
| Rootstock | тЬЕ Integrated | `internal/engine/src/ntt/rootstock_adapter.rs` |
| Babylon | Implemented boundary — PR #253 merged; EOTS/finality extensions remain separate | `internal/engine/src/bitcoin/babylon_adapter.rs` |
| BitVM2 | Partial — metadata adapter plus validated Groth16 handoff on `main`; cryptographic backend remains open | `internal/engine/src/bitcoin/bitvm_adapter.rs` |
| RGB | 🟡 v0.12 + Stash/consignment boundary; Phase 2 hardening merged, issuer backend and signed regtest fixture remain open | `internal/engine/src/bitcoin/rgb_adapter.rs` + `rgb_native.rs` + `rgb_stash.rs` |
| Liquid | тЬЕ Integrated | `internal/engine/src/bitcoin/liquid_adapter.rs` |
| Citrea | тЬЕ Integrated | `internal/engine/src/ntt/citrea_adapter.rs` |
| RISC Zero | ЁЯЯб Unwired | `internal/engine/src/bitcoin/risc0_verifier.rs` |
| Fedimint | тЬЕ Integrated | `internal/engine/src/bitcoin/fedimint_adapter.rs` |
| Strata | тЬЕ Testnet | `internal/engine/src/bitcoin/strata_adapter.rs` |
| BitVMX GC | Research only — no stable public GC SDK/release or production deployment verified | `docs/research/BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md` |
| BRICS Pay | ЁЯЯб Research only | N/A |
| mBridge | ЁЯЯб Research only | N/A |
| Canton Network | 🟡 Research — "route without touch" (G-C7 P3) | `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` |
| Machine Economy (peaq/DIMO/DePIN) | 🟡 Research — BTC/LN gateway opportunity (G-C8 P3) | `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` |
| UCV-2 | 🟡 Architecture — Cross-ledger settlement protocol | `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` |

## Protocol Coverage — SDK → Gateway Alignment

The Conxius Enclave SDK (`lib-conclave-sdk` v0.2.5) defines the canonical **41-chain AssetRegistry**, **33 protocol modules**, and **6 settlement rails**. The Gateway is the primary runtime adapter layer — every SDK protocol with production intent must have a Gateway adapter.

### SDK Chain → Gateway Adapter Map (41 chains)

| # | Chain | SDK Registry | Gateway Adapter | Status |
|---|-------|-------------|-----------------|--------|
| 1 | Bitcoin | ✅ BTC | bitcoin core | ✅ Live |
| 2 | Lightning | ✅ BTC | lightning (via LND) | ✅ Integrated |
| 3 | Liquid | ✅ L-BTC | `liquid_adapter.rs` | ✅ Integrated |
| 4 | Rootstock | ✅ RBTC | `rootstock_adapter.rs` | ✅ Integrated |
| 5 | Babylon | ✅ BTC | `babylon_adapter.rs` | ✅ Integrated |
| 6 | BitVM2 | ✅ BTC | `bitvm_adapter.rs` | ✅ Integrated |
| 7 | RGB | ✅ BTC | `rgb_adapter.rs` + `rgb_stash.rs` | ✅ v0.12 P1 |
| 8 | Citrea | ✅ BTC | `citrea_adapter.rs` | ✅ Integrated |
| 9 | Fedimint | ✅ BTC | `fedimint_adapter.rs` | ✅ Integrated |
| 10 | Strata | ✅ BTC | `strata_adapter.rs` | ✅ Testnet |
| 11 | BOB | ✅ BTC | ⚠️ Planned | Q4 2026 |
| 12 | Botanix | ✅ BTC | ⚠️ Planned | Q4 2026 |
| 13 | Mezo | ✅ BTC | ⚠️ Planned | Q4 2026 |
| 14 | Stacks | ✅ STX | Via Hiro API | ✅ Integrated |
| 15 | Ethereum | ✅ ETH | Via EVM stack | ✅ Integrated |
| 16 | Solana | ✅ SOL | ⚠️ Planned | SolanaAdapter P2 |
| 17 | Arbitrum | ✅ ETH | Via EVM | ✅ Via EVM |
| 18 | Base | ✅ ETH | Via EVM | ✅ Via EVM |
| 19 | Optimism | ✅ ETH | Via EVM | ✅ Via EVM |
| 20 | Linea | ✅ ETH | Via EVM | ✅ Via EVM |
| 21 | Polygon | ✅ POL | Via EVM | ✅ Via EVM |
| 22 | BSC | ✅ BNB | Via EVM | ✅ Via EVM |
| 23 | Avalanche | ✅ AVAX | Via EVM | ✅ Via EVM |
| 24 | Celo | ✅ CELO | Via EVM | ✅ Via EVM |
| 25 | Fantom | ✅ FTM | Via EVM | ✅ Via EVM |
| 26 | Gnosis | ✅ GNO | Via EVM | ✅ Via EVM |
| 27 | Cronos | ✅ CRO | Via EVM | ✅ Via EVM |
| 28 | Kava | ✅ KAVA | Via EVM | ✅ Via EVM |
| 29 | Mantle | ✅ MNT | Via EVM | ✅ Via EVM |
| 30 | zkSync | ✅ ETH | Via EVM | ✅ Via EVM |
| 31 | Scroll | ✅ ETH | Via EVM | ✅ Via EVM |
| 32 | Taiko | ✅ TAIKO | Via EVM | ✅ Via EVM |
| 33 | Blast | ✅ BLAST | Via EVM | ✅ Via EVM |
| 34 | Berachain | ✅ BERA | Via EVM | ✅ Via EVM |
| 35 | Starknet | ✅ STRK | ⚠️ Planned | StarknetAdapter P3 |
| 36 | Monad | ✅ MONAD | ⚠️ Planned | MonadAdapter P3 |
| 37 | Near | ✅ NEAR | ⚠️ Planned | NearAdapter P3 |
| 38 | Cosmos | ✅ ATOM | ⚠️ Planned | CosmosAdapter P3 |
| 39 | XRP Ledger | ✅ XRP | ⚠️ Planned | XRPLAdapter P3 |
| 40 | Tron | ✅ TRX | ⚠️ Planned | TronAdapter P3 |
| 41 | Sui/Aptos/Sei/Stellar | ✅ Various | ⚠️ Planned | Move/Stellar P3 |

### Settlement Rails Coverage

| Rail | SDK | Gateway | Status |
|------|-----|---------|--------|
| x402 | ✅ | ✅ Integrated | Open payment protocol |
| Wormhole | ✅ | ✅ NTT adapter | Cross-chain messaging |
| NTT | ✅ | ✅ `ntt/` | Native token transfer |
| Bisq | ✅ | ❌ Not covered | P2P exchange (wallet-side) |
| Boltz | ✅ | ❌ Not covered | Atomic swap (wallet-side) |
| Changelly | ✅ | ❌ Not covered | Instant exchange (wallet-side) |

### SDK Protocol Module → Gateway Coverage

| SDK Module | Gateway Status | Notes |
|-----------|---------------|-------|
| bitcoin, lightning, liquid, rootstock, babylon, bitvm, rgb, citrea, fedimint, strata | ✅ Adapter | All in `internal/engine/src/` |
| stacks, ethereum, evm-l2s | ✅ RPC/API | Via Hiro/EVM RPC |
| dlc | ✅ NWC adapter | `nwc_backend.rs` |
| nwc (NIP-47) | ✅ Integrated | Nostr Wallet Connect |
| mmr, frost, musig2 | ✅ Enclave SDK | Via `conxius-enclave-sdk` |
| solana, near, cosmos, xrp, tron, sui, aptos | ⚠️ Planned | P2/P3 adapters |
| a2p, account_abstraction, cctp, chain_abstraction, credit, fiat, intent, job_card, opportunity, solver, swap_router | ❌ Not covered | Application/logic layer |
| stablecoin_orchestrator | ✅ Partial | Regional stablecoins |
| ark, bip322, covenant | ❌ Not covered | P3 research |

## Core Philosophy
- **Sovereignty**: All code must prioritize non-custodial logic and user sovereignty.
- **Institutional Grade**: Maintain SLA-grade interfaces, high-performance async Rust, and robust error handling.
- **Compliance Pipe**: The gateway is a pass-through for compliance data (ZKC), not a storage for PII.

## Technical Standards
- **Rust Edition**: 2021
- **MSRV**: 1.96 (declared minimum and CI-tested baseline: 1.96.0)
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
- **node-ci.yml**: TypeScript build + vitest across all Node workspaces.
- **release.yml**: Tag-triggered, fail-closed GitHub Release with the production Gateway archive, checksum manifest, normalized CycloneDX 1.5 SBOM, SLSA provenance subjects, protected publication job, and optional crates.io environment gate.

## Known Gaps (2026-07-14 snapshot; current corrections noted)
- [ ] #222: strict CI/CD release governance — Phase 3 workflow/runbook implementation is prepared on the audit branch; merge, required-check/ruleset and `release` environment administration, live tagged-release evidence, and publishable Cargo package metadata remain
- [ ] #228: RGB stash resolver (G-1385) — transactional existing-contract updates and process-lifetime stash ownership are merged; an opt-in BIP340 issuer-policy backend is implemented on `charlie/issue-228-bip340-issuer-policy`; controlled runtime/import wiring and a complete state-changing signed Bitcoin/RGB regtest fixture remain open
- [x] #233 (G-1389): Tech debt reduction — merged `5e6613e`
- [x] G-1276: Redis AUTH + token expiry — merged `2ef6df1`
- [x] G-1380: SBOM and Provenance to release workflow — merged `19181c5`
- [x] #236: SDK version drift + README overclaim — fixed in tree (`packages/client-sdk/package.json` is `0.1.4`; README says "Developer Preview"); issue state is tracked separately
- [ ] #220: DLC CET construction — HTTP oracle/event/key/outcome scaffold only; research/API spike required before selecting `rust-dlc` or DDK. No cryptographic announcement/attestation verification, DLC dependency, funding/CET/refund/adaptor-signature construction, or real bond construction is present; UUID/mock bond IDs only. See `docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`
- [ ] #219: Groth16 cryptographic backend — boundary contract and deterministic fixture handoff merged in PR #255; production pairing backend remains open
- [x] #216: Babylon BTC header-chain SPV — bounded header-chain retrieval/verification merged in PR #253; EOTS/finality extensions remain separate
- [ ] #189: BitVM3/BitVMX-GC adapter — research-only; PRs #259, #267, and #268 (the comprehensive SDK/paper/network-proof/cross-repo triage) are merged; no stable GC SDK or production deployment is verified
- [x] #231: BRICS Pay — DCMS settlement rail (closed — research complete, no adapter needed)
- [x] #232: mBridge — BIS multi-CBDC DLT (closed — research complete, observation only)

Current gap analysis: `docs/GAP_ANALYSIS_2026-07-22.md`
Historical snapshot: `docs/GAP_ANALYSIS_2026-07-14.md`

### Critical P0 Actions (W29 — historical approval list)
The #236 version and README corrections listed below are complete in the current
tree. This historical list is retained for continuity and does not imply that
those two fixes remain open.
1. **#236 SDK version** — ✅ Applied: `packages/client-sdk/package.json` is `0.1.4`
2. **#236 SDK README** — ✅ Applied: the status is "Developer Preview", not "Production Ready"
3. **Align DLC research and API gate** — Compare pinned `rust-dlc` v0.8.0 and DDK v1.1.2 in an isolated spike before any workspace dependency or CET implementation for #220
4. **Define Groth16 boundary** — Canonical contract and BitVM handoff merged in PR #255; add a real backend separately
5. **Implement Babylon SPV** — BTC header-chain retrieval and bounded verification merged in PR #253; EOTS/finality remains separate

**Status:** P0 items remain approved; #216's header-chain/SPV boundary and #219's Groth16 boundary milestone are merged, while a production Groth16 backend and additional Babylon finality/EOTS work remain separate. DLC remains research/status alignment only until the gates in `docs/research/DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md` pass.

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

### #189: BitVM3 / BitVMX-GC (Garbled Circuits)
- **BitVMX-CPU**: Public Rust/RISC-V emulator and isolated Gateway evaluator; README says under development, unaudited, and not production-ready. Repository/`LICENSE` metadata says Apache-2.0 while README says MIT; unresolved.
- **BitVMX-GC**: Official design/article material exists, but no stable public GC SDK/API, release, reproducible integration target, or production deployment is verified.
- **GOATNetwork/bitvm2-gc**: Public research/reference source — Groth16 + DV-SNARK via GC, approximately 10.4B gates and 51–374 GB upstream-reported peak memory; no verified release/license artifact.
- **BitVM3 authority**: IACR ePrint 2026/933, received 2026-05-11 and revised 2026-06-08; paper/prototype evidence only.
- **BitVMX mainnet evidence**: Upstream SNARK-verifier prototype transaction exists, but it is not BitVM3-GC, a stable SDK, a production bridge, or Conxian verifier evidence.
- **Conxian posture**: Keep #189 open/research-only; use `docs/research/BITVM3_BITVMX_EVIDENCE_AND_TRIAGE_2026-07-22.md` as the canonical evidence and cross-repo triage record.

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

## Session State (2026-08-07 — PR #324 merged ✅)

### ✅ PR #324: Full-Scope Branch Integration (merged `05f6843`)
**Branches merged into main**: `docs/session-48-market-integration` + `feat/session-49-50-gap-closures-and-research`
- 28 files, +3,518/-144 lines
- All gates: cargo check ✅, clippy 0 warnings ✅, 308 lib tests ✅, fmt ✅, contamination guard ✅
- Branches deleted post-merge, org-wide 11 issues updated with cross-references

### Sessions 49–50: Full-Scope Production Readiness + Research Expansion + Gap Closure

**8 commits · 28 files · 3,397 lines**

#### Production Completeness (Session 49)
- **ENS resolver**: Production path now calls The Graph ENS subgraph for real resolution (was "disabled in this build")
- **BNS resolver**: Improved error message directing operators to set `STACKS_RPC_URL`
- **RGB BIP340 issuer policy**: Wired at runtime — `Bip340IssuerPolicy` loaded from `RGB_ISSUER_POLICY_PATH`, stored in `NodeRgbAdapter`, fail-closed on error
- **Chain classification**: `CCIP_HIGH/MEDIUM/LOW_RISK_CHAINS` env vars replace hardcoded lists (the only TODO in the codebase is now resolved)
- **Persistence stubs**: Tableland/Kwil now emit `warn!` logs on fallback, expose `as_str()` and `PERSISTENCE_BACKEND_METRIC`
- **Unsafe hygiene**: Replaced `#[allow(unused_unsafe)]` with `SAFETY` comments in `ntt/relayer.rs`

#### Code Quality & Security (Session 49)
- **Admin endpoint tests**: 8 new tests covering auth rejection, invalid tokens, valid tokens (all 3 endpoints), and malformed JSON
- **XML injection fix**: Entity escaping in CAMT XML generators (`camt.rs`); 8 unit tests covering &, <, >, ", ', injection attempts
- **Orphan module fix**: `camt.rs` was never declared in `lib.rs` as a module; now wired
- **CI verification**: clippy clean (0 warnings), 420+ lib tests passing, cargo check clean, contamination guard clean

#### Research Documentation (10 docs, ~2,500 lines)
| # | Document | Focus |
|---|----------|-------|
| 1 | `LIGHTNING_SETTLEMENT_RAIL_RESEARCH.md` | BOLTs, 5 implementations, 3 backends, $1.1B M2M/month |
| 2 | `SBTC_SETTLEMENT_RAIL_RESEARCH.md` | SIP-021, Emily API, peg mechanism, trust model |
| 3 | `BABYLON_ADAPTER_RESEARCH.md` | BTC header SPV, EOTS/finality, staking lifecycle |
| 4 | `FEDIMINT_ADAPTER_RESEARCH.md` | Chaumian e-cash, privacy-compliance tension |
| 5 | `DLC_SETTLEMENT_RAIL_RESEARCH.md` | 6-stage plan, 13/14 vectors, Schnorr roadmap |
| 6 | `FIAT_ISO20022_SETTLEMENT_RAIL_RESEARCH.md` | 4 providers, CAMT, XML injection found+fixed |
| 7 | `BITVM_VERIFICATION_FAMILY_RESEARCH.md` | Groth16 verifier, BitVM3 9 promotion gates |
| 8 | `RGB_SETTLEMENT_RAIL_RESEARCH.md` | 3-tier RolloutMode, 3,255-line stash |
| 9 | `NTT_SOVEREIGN_BRIDGE_RESEARCH.md` | Trust-policy relay, RSK/Citrea/Strata |
| 10 | `GAP_ANALYSIS_2026-08-07.md` | 20 gaps, dependency graph, scoring, 3-phase roadmap |

#### Gaps Closed (Session 50)
| Gap | Description | Impact |
|-----|-------------|--------|
| **G-DL1** | BIP340 Schnorr oracle attestation verification | Unblocks entire 6-stage DLC pipeline; `secp256k1` + `sha2` now non-optional deps |
| **G-FM2** | Fedimint federation discovery | `FederationConfig` struct + `discover_federation()` + JSON/URI parsing + 10 tests |
| **XML injection** | CAMT entity escaping | Production-grade institutional banking compliance |

#### Gap Analysis (2026-08-07)
20 gaps identified across 9 settlement rails. Priority triage:
- **P1 (1 remaining):** G-BB1 (Babylon EOTS verification, 3-5d)
- **P2 (8):** G-FI1/2/3, G-BB2/3, G-FM1, G-SB3
- **P3 (4):** G-FI4, G-LN2/3, G-FM3 (governance)
- **Infra-gated (4):** G-SB1/2, G-LN1, G-FI4

#### Adapter Registry
All adapters now have research docs linked in `ADAPTER_FAMILY_STRATEGY.md`.
DLC CET added to Bitcoin L2/Sidechain family. RGB detail section with modularization note.
NTT detail section with Rootstock/Citrea/Strata comparison.

### Remaining
- crates.io publish: `gh workflow run release.yml -R Conxian/conxian-gateway -f release_version=0.1.5 -f publish_to_crates_io=true`
- G-BB1: Babylon EOTS verification (highest remaining P1)
- `canton_m2m_tests` binary: LLVM linker crash (SIGBUS) — pre-existing infrastructure issue; lib tests all pass

---

## Session 48 Gap Analysis Integration

Cross-repo gap analysis published in `conxian_market/docs/research/CROSS_REPO_GAP_ANALYSIS_SESSION_48.md`.
Gateway-specific gaps with implementation tracking:

| Gap | Issue | Severity | Sprint | Adapter Impact |
|-----|-------|:--------:|:------:|:---------------|
| CI/CD strict baseline | [#222](https://github.com/Conxian/conxian-gateway/issues/222) | P1 | S1 | All adapters |
| RGB stash resolver | [#228](https://github.com/Conxian/conxian-gateway/issues/228) | P1 | S5 | RGB rail |
| DLC CET construction | [#220](https://github.com/Conxian/conxian-gateway/issues/220) | P1 | S5 | DLC rail |
| BitVM3 adapter | [#189](https://github.com/Conxian/conxian-gateway/issues/189) | P2 | S5 | BitVM3 rail |
| BIP-110 fee market eval | [#245](https://github.com/Conxian/conxian-gateway/issues/245) | P2 | S3 | Routing |
| MRR/billing module | [#306](https://github.com/Conxian/conxian-gateway/issues/306) | P2 | S3 | Billing |

### Adapter Production Readiness

```
Production: sbtc.rs, alex.rs, babylon_adapter.rs, fedimint_adapter.rs — wired, green CI
Partial:    rgb_adapter.rs — core wired, stash resolver (#228) needed
Stub:       dlc_oracle.rs — CET path not built (#220)
Research:   bitvm_adapter.rs — garbled circuits (#189)
```

Cross-ref: `SETTLEMENT_RAILS.md` §10 (market-side adapter readiness table).
