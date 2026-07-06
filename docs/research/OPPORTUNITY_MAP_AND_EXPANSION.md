# Opportunity Mapping & Research Expansion (2026-06-29)

This document expands on existing research and maps emerging opportunities for the Conxian Gateway stack. **Updated with BRICS+ financial systems research and multi-currency settlement opportunities.**

## 1. Emerging Protocol Opportunities

### A. BitVM3 & Recursive Proofs (Expansion of SSV-1)
- **Status**: Research
- **Opportunity**: BitVM3 introduces recursive SNARK verification on Bitcoin.
- **Expansion**:
    - Research integration of **recursive Groth16 verifiers** directly into the `UniversalVerifier`.
    - Propose a "State Compression" layer in the Gateway that aggregates multiple Job Card proofs into a single BitVM3 commitment, reducing on-chain footprint by an estimated 60%.
    - Target: Support 1000+ sharded labor attestations per Bitcoin anchor.

### B. Local-First (Wasm) UCV-1
- **Status**: Experimental
- **Opportunity**: Moving verification to the client (SDK/Wallet) improves latency and privacy.
- **Expansion**:
    - Audit `pkg/conxian-core` for `no_std` compatibility to support Wasm compilation.
    - Research a "Verified Lite-Client" mode for the SDK where the client verifies Stacks Nakamoto proofs locally using the Gateway only for data availability.

### C. ISO 20022 camt.* Expansion
- **Status**: Directional
- **Opportunity**: Move beyond payment initiation (pacs.008) to full treasury reporting.
- **Expansion**:
    - Research mapping of `TreasuryMonitor` events to `camt.053` (Bank-to-Customer Statement) messages.
    - Propose an "Institutional Reconciliation" endpoint that outputs audit-ready XML for ERP ingestion.

### E. Canton Network Interoperability (New — 2026-07-06)
- **Status**: Research
- **Opportunity**: Canton Network is a privacy-enabled institutional DLT from Digital Asset powering $6T+ in tokenized RWAs across Goldman Sachs, BNP Paribas, Deutsche Börse. Its eUTXO model (Daml) is architecturally isomorphic to Bitcoin UTXO.
- **Expansion** (see `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` for full analysis):
    - **G-C1**: CBTC non-custodial verification — DLC-based Bitcoin reserve attestation for CBTC (BitSafe wrapped Bitcoin on Canton). Verify FROST threshold attestations without joining the signer set.
    - **G-C4**: Canton state translation adapter — Map Daml Active Contract Set → Universal Contract Reference → Bitcoin anchor. Observe-only, never run a Canton validator.
    - **G-C5**: Chainlink CCIP Canton connector — Route CCIP messages through Conxian's compliance ZKC pipeline.
    - **G-C7**: Canton↔Bitcoin atomic swap engine — Trustless cross-chain settlement between Daml contracts and Bitcoin UTXOs (HTLC/PTLC).
- **Market Impact**: Canton tokenizes $6T+ in institutional assets. Conxian is the sovereign routing layer between this institutional capital and permissionless Bitcoin — "route without touching."
- **Sovereignty Alignment**: ✅ Observe only, never custody, never run a Canton validator.

### F. Machine Economy (DePIN + M2M — New — 2026-07-06)
- **Status**: Research
- **Opportunity**: The Machine Economy (DePIN, M2M payments) is emerging where machines own wallets, pay machines, and earn autonomously. Lightning Network has hit $1.1B/month volume with USDT via Taproot Assets — becoming the M2M settlement rail.
- **Expansion** (see `docs/research/CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md` for full analysis):
    - **G-C2**: Machine identity DID extension — Extend SovereignIdentity with MachineIdentity (peaq DID + device key). Leverage existing BNS/ENS/World ID stack.
    - **G-C3**: Lightning M2M settlement primitives — Add SettlementSource::MachineToMachine variant. Integrate d402/x402 for API-level machine payments.
    - **G-C6**: Machine RWA revenue verification — Verify machine revenue attestations (peaq, DIMO, ELOOP). Route verified revenue to token holders via Lightning.
    - **G-C8**: DePIN compliance ZKC — Jurisdictional tax reporting for autonomous machine income.
- **Market Impact**: peaq hosts 60+ dApps with 500K+ machines and $180M TVL. Machine identity + M2M routing is a first-mover infrastructure play.
- **Sovereignty Alignment**: ✅ Machines hold their own keys; Conxian routes and verifies.

### D. BRICS+ Multi-Currency Settlement (New — 2026-06-29)
- **Status**: Research → Active Development
- **Opportunity**: The global financial system is bifurcating. BRICS+ represents ~40% of global GDP with alternative payment rails (CIPS, mBridge, SPFS, BRICS Pay) that bypass Western SWIFT/CHIPS infrastructure.
- **Expansion** (see `docs/research/BRICS_FINANCIAL_SYSTEMS_RESEARCH.md` for full analysis):
    - **G-B1**: CIPS-direct message normalization — CIPS processes $24.47T/year across 1,690 institutions. The Gateway now handles CIPS-specific ISO 20022 message variants (Implemented Phase 3).
    - **G-B2**: Multi-currency FX tracking — Extended `TreasuryMonitor` to track RMB, RUB, INR, AED rates across BRICS settlement corridors (Implemented Phase 3).
    - **G-B3**: BRICS Pay DCMS connector — Monitor the decentralized messaging system pilot from Saint Petersburg State University.
    - **G-B4**: Sanctions-risk tagging — Critical for compliance (Implemented Phase 3). Each `SettlementSource` variant needs a `SanctionsRisk` classification (Implemented Phase 3).
    - **G-B5**: PAPSS settlement rail — Pan-African Payment and Settlement System integration (Implemented Phase 3) for African Union member states (Implemented Phase 3).
    - **G-B6**: mBridge validator node — EVM-compatible CBDC bridge; post-BIS exit, being repositioned as "BRICS Bridge."
- **Market Impact**: ~20% of global commodity trade has already shifted from USD to RMB/AED/INR corridors. The Gateway's dual-stack architecture (ISO 20022 + BRICS protocols) positions it for both G7-compliant and sanctions-resilient deployments.

## 2. Missing Canonical Artifacts

### A. Flagship Technical Whitepaper (CON-1300)
- **Requirement**: A single, versioned technical reference consolidating doctrine, architecture, and trust boundaries.
- **Target State**: 15-20 page PDF/Markdown document.
- **Key Section**: "The Progressive Sovereignty Model" – explicitly defining how the system transitions from trusted anchors to trustless proofs.
- **BRICS Context**: Whitepaper should include a dedicated section on multi-currency settlement architecture and sanctions-resilience by design.

### B. Developer Quickstart & Architecture Guide (CON-1301)
- **Requirement**: A "shortest path to value" for external builders.
- **Target State**: Multi-page GitHub Pages site.
- **Key Content**: "Hello World" settlement flow using the `@conxian/client-sdk`.

## 3. Architectural Improvements (Maturity Alignment)

### A. Transitioning Tier 2 Adapters
- **Current State**: Liquid and Babylon adapters are in "Shadow Mode" (rehearsal only).
- **Proposal**: Implement "Active Verification" mode where the Gateway rejects settlement intents if the sidechain/L2 state-proof verification fails.
- **Effort**: High (requires robust error handling for sidechain reorgs).

### B. Event-Bus Durability
- **Current State**: In-memory Tokio channels.
- **Proposal**: Back the event bus with a persistent log (e.g., SQLite or Redis Streams) to ensure zero-loss delivery during Gateway restarts.

### C. Dual-Stack Settlement Architecture (New — 2026-06-29)
- **Current State**: `SettlementSource` supports ISO 20022 (pacs.008/pacs.009), BRICS (generic), PAPSS (generic), and ERP (OData). All BRICS traffic goes through `normalize_brics_ingress()` with no distinction between CIPS, mBridge, or SPFS.
- **Proposal**: Split `SettlementSource::Brics` into specific variants: `Cips`, `MBridge`, `Spfs`, `BricsPay`. Each gets its own message normalization path and sanctions-risk classification.
- **Effort**: Medium (3-5 days engineering). Primarily type-system changes + normalization logic.
