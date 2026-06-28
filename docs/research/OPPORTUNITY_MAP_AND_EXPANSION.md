# Opportunity Mapping & Research Expansion (2026-06-28)

This document expands on existing research and maps emerging opportunities for the Conxian Gateway stack.

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

## 2. Missing Canonical Artifacts

### A. Flagship Technical Whitepaper (CON-1300)
- **Requirement**: A single, versioned technical reference consolidating doctrine, architecture, and trust boundaries.
- **Target State**: 15-20 page PDF/Markdown document.
- **Key Section**: "The Progressive Sovereignty Model" – explicitly defining how the system transitions from trusted anchors to trustless proofs.

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
