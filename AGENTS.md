# Conxian Gateway: Agent Instructions

You are working on the **Conxian Gateway**, an institutional-grade Rust middleware designed for high-performance Bitcoin/Stacks state logic and enterprise compliance.

## Current State (2026-07-05, updated)
- **Status Audit**: Holistic review of Nexus/Gateway alignment complete (CON-1353).
- **Protocol Drift**: Resolved тАФ Fedimint, Citrea, and Strata adapters implemented and in production paths.
- **RGB G-1385 (Phase 1)**: StashResolver delivered (commit `124d17e`) with `rgb-std` v0.12.0-rc.3 + `bp-esplora` v0.12.0-rc.3 behind `rgb-native` feature. Phase 2 (ContractVerify, consignment) blocked on rgb-std ecosystem stabilization.
- **PR #233 (G-1389)**: Tech debt reduction merged (`5e6613e`). Includes Fedimint/Citrea/Strata adapters, Redis coordination module, auth middleware timing stubs, reqwest 0.13 upgrade, dead_code cleanup. Citrea adapter moved from `bitcoin/` to `ntt/`.
- **PR #228 (G-1385)**: RGB stash resolver merged (`124d17e`), retained through rebase.
- **Hardening Stubs**: ✅ CON-1276 (Redis AUTH + token expiry) now fully implemented (commit `2ef6df1`). Adds `REDIS_USERNAME`/`REDIS_PASSWORD`/`TOKEN_TTL_SECONDS` env vars, explicit AUTH+PING at Redis init, uptime-based token expiry in auth middleware.
- **UCV-1**: Fully implemented and unifying Babylon, BitVM2, Liquid, Rootstock, and RGB.
- **CI status**: All workflows green on main. `cargo-audit.yml` augmented with `.cargo/audit.toml` ignore list for transitive `rustls-webpki` CVEs.

### Protocol Implementations (2026-07-05)
| Protocol | Status | File |
|---|---|---|
| NWC NIP-47 | тЬЕ Integrated | `internal/api/src/nwc_backend.rs` |
| Rootstock | тЬЕ Integrated | `internal/engine/src/ntt/rootstock_adapter.rs` |
| Babylon | тЬЕ Integrated | `internal/engine/src/bitcoin/babylon_adapter.rs` |
| BitVM2 | тЬЕ Integrated | `internal/engine/src/bitcoin/bitvm_adapter.rs` |
| RGB | тЬЕ v0.12 + Stash (P1) | `internal/engine/src/bitcoin/rgb_adapter.rs` + `rgb_native.rs` + `rgb_stash.rs` |
| Liquid | тЬЕ Integrated | `internal/engine/src/bitcoin/liquid_adapter.rs` |
| Citrea | тЬЕ Integrated | `internal/engine/src/ntt/citrea_adapter.rs` |
| RISC Zero | ЁЯЯб Unwired | `internal/engine/src/bitcoin/risc0_verifier.rs` |
| Fedimint | тЬЕ Integrated | `internal/engine/src/bitcoin/fedimint_adapter.rs` |
| Strata | тЬЕ Testnet | `internal/engine/src/bitcoin/strata_adapter.rs` |
| BitVMX GC | ЁЯЯб Pending 2026 | N/A |
| BRICS Pay | ЁЯЯб Research only | N/A |
| mBridge | ЁЯЯб Research only | N/A |

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
5. Verify health check: `GET /api/v1/health` returns `healthy`.
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

## Known Gaps (2026-07-05 Update)
- [x] #228: RGB stash resolver (G-1385 P1) — merged `124d17e`
- [x] #233 (G-1389): Tech debt reduction — merged `5e6613e`
- [x] G-1276: Redis AUTH + token expiry — merged `2ef6df1`
- [x] G-1380: SBOM and Provenance to release workflow — merged `19181c5`
- [ ] #189: BitVMX GC adapter — pending 2026 garbled circuits release (see research below)
- [x] #231: BRICS Pay — DCMS settlement rail (closed — research complete, no adapter needed)
- [x] #232: mBridge — BIS multi-CBDC DLT (closed — research complete, observation only)

Protocol drift resolved — 10 of 10 identified protocols now have adapters.
All implementation-level gaps are closed. Remaining items are research/policy only.

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
4. **Research issue updater**: Cron→prompt preset to refresh #189/#231/#232 research monthly
5. **Release note generator**: Event→on `release.published` → generate changelog from git history

## Ethical Alignment
The Conxian Protocol is built to empower individuals and institutions within the Stacks/Bitcoin ecosystem. The dual-stack settlement architecture supports financial sovereignty across both Western and BRICS-aligned jurisdictions. Avoid any "dark patterns" or custodial shortcuts.
