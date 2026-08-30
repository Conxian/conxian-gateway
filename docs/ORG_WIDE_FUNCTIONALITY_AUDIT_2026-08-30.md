# Org-Wide Functionality Check & Alignment Audit

**Snapshot date:** 2026-08-30 (live GitHub API queries, not a historical snapshot)
**Scope:** Conxian organization — SDK/core → gateway → platform/application layers
**Method:** live `api.github.com` queries for repos, releases/tags, and open issues; source-tree inspection of `conxian-gateway` and dependency manifests.

---

## 1. Repository Inventory (corrected)

Fifteen repositories across four layers. Names below are the current GitHub
slugs (correcting stale references such as `Conxian_UI`, `stacksorbit`, and
`conxian.github.io` layering).

| Layer | Repository | Language | Production Path | Notes |
|-------|-----------|----------|-----------------|-------|
| L0 Protocol/DAO | `Conxian` | Clarity | main | Smart contracts, CXIP, system index |
| L1 Verification | `conxian-nexus` | Rust | main | Glass Node proof layer, MMR state roots, BLS12-381/Arkworks |
| L1 Routing | `conxian-gateway` | Rust | main | Transport/RPC adaptation, ISO 20022, settlement rails |
| L2 Core/SDK | `lib-conxian-core` | Rust | main | Shared protocol primitives, chain adapters, control models |
| L2 Enclave SDK | `conxius-enclave-sdk` | Rust | main | Hardware-backed signing/attestation/policy (beta/conditional) |
| L2 App surface | `conxius-wallet` | TypeScript | main | Sovereign wallet/reference client |
| L2 App surface | `conxian_ui` | TypeScript | main | Web UI |
| L2 App surface | `conxian_market` | TypeScript | main | Treasury dashboard/reporting |
| L2 Site | `conxian-labs-site` | HTML | main | Public site |
| L2 Site | `conxian.github.io` | HTML | main | GitHub Pages |
| L3 Control plane | `conxius-platform` | TypeScript | main | Admin dashboard / control plane |
| L3 Strategy | `conxian-business` | Python | main | BOS operating system, research |
| L3 Governance | `.github` | Python | main | Org operating model, rulesets |
| L3 Governance | `.github-private` | Python | main | Private governance (not audited) |
| L3 Archived | `conxius-orbit` | Python | main | Archived |

---

## 2. Dependency & Version Alignment (SDK/Core → Gateway)

The Gateway resolves two upstream Conxian crates, both via git tags.

| Dependency | Gateway source | Pinned version | Latest release | Status |
|-----------|----------------|----------------|----------------|--------|
| `lib-conxian-core` | workspace `Cargo.toml` (`tag = "v0.3.2"`) | v0.3.2 | v0.3.2 (2026-08-14) | ✅ Current |
| `conxius-enclave-sdk` | transitive via `lib-conxian-core` `full-sdk`/`sdk-blockchain` | v2.0.16 | v2.0.17 (2026-08-30) | 🟡 One patch behind (controlled by `lib-conxian-core`, not Gateway) |
| `conxian-nexus` | not a crate dependency | n/a | v0.4.22 (README says v0.4.23) | ⚠️ Cross-repo proof surface (see gaps) |

Notes:

- `lib-conxian-core` v0.3.2 pins `conxius-enclave-sdk` to git tag `v2.0.16`
  (manifest version `2.0.16`). The `lib-conxian-core` README still references
  `v2.0.14`, which is stale doc text relative to its own `Cargo.toml`.
- Gateway feature selection: `pkg/conxian-core` enables `lib-conxian-core/full-sdk`
  (all SDK modules); `internal/engine` enables `lib-conxian-core/sdk-blockchain`
  (24 blockchain modules). Both feature sets resolve to the same `v2.0.16` SDK.

### SDK capability surface (`lib-conxian-core` feature gates → enclave modules)

| Feature | Module count | Purpose |
|---------|--------------|---------|
| `sdk-blockchain` | 24 | Blockchain protocols |
| `sdk-cross-cutting` | 15 | Cross-cutting capabilities |
| `sdk-rails` | 6 | SDK rails (currently `pub(crate)`) |
| `sdk-nexus` | 2 | Nexus verification |
| `sdk-infrastructure` | 5 | Infrastructure & tooling |
| `sdk-signing` | 13 | Signing primitives |
| `full-sdk` | (all of the above) | Meta-feature |

---

## 3. Enabled Functionality Map (Gateway vs SDK/Core)

### Gateway chain adapter registry (`cmd/gateway/src/main.rs` → `multi_chain`)

| Chain key | Adapter | Verification posture |
|-----------|---------|----------------------|
| `liquid` | `LiquidAdapter` | Fail-closed (Elements proof backend unwired) |
| `rootstock` | `RootstockAdapter` | Structural (BTC tx SHA256d) |
| `babylon` | `BabylonAdapter` | Structural + BIP340 Schnorr EOTS |
| `bitvm` | `BitVmAdapter` | Boundary (BN254 Groth16 envelope; no pairing backend) |
| `bitvm3` | `BitVm3Adapter` | Research-only, fail-closed (garbled circuits / recursive proofs) |
| `fedimint` | `FedimintAdapter` | Structural + guardian pubkey blind-sig |
| `citrea` | `CitreaAdapter` | Structural (ZK-proof hex+len) |
| `strata` | `StrataAdapter` | Structural (32-byte Merkle root) |

Additional feature-gated / adjacent lanes: `rgb` (`rgb-native` feature),
`dlc` (`dlc_oracle`), `lightning` (`new_lightning_adapter`), `sBTC`
(L1 tx/block proof), and `risc0` verifier.

### Gateway API surface (high-level)

- **Settlement rails:** ISO 20022 (`pacs.008`/`pacs.009`), PAPSS, BRICS, CIPS,
  SPFS, mBridge, UBL invoicing, EDI purchase orders, POS/offline, job-card settle.
- **Identity/auth:** attestation, identity exchange/resolve, World ID, KYC ZK
  commitment, OTP (A2P), fiat session/webhook.
- **Chain/verification:** `/chains/*`, `/verify`, DLC bond, M2M settle, CCIP
  route, ALEX quote/prepare/swap.
- **Observability:** `/health`, `/metrics` (Prometheus), `/state`, `/handoff`.

### Alignment summary

- The Gateway correctly stays **backend-neutral** for BitVM/BitVM3/RGB/DLC and
  **fails closed** rather than fabricating production verification.
- The enclave SDK is **beta/conditional**: its README and PRODUCTION_READINESS
  explicitly forbid enabling value-bearing production signing/settlement from
  the current tree. Gateway wiring does **not** contradict this boundary.
- Seven of the Gateway's ten adapter lanes now perform real (structural or
  cryptographic) verification; three remain fail-closed rehearsal lanes:
  **BitVM3**, **BitVM**, and **Liquid**.

---

## 4. Org-Wide Open Issues (42)

Categorized by layer. Full list observed live on 2026-08-30.

### L2 Enclave SDK (conxius-enclave-sdk) — 6 open
- #202 [P0] Independent security review & release acceptance evidence
- #240 [P0] Attestation roots, collateral, revocation, distributed replay
- #241 [P0] Android KeyMint/StrongBox authorization & Play Integrity
- #242 [P0] AWS Nitro attestation & KMS secret-release boundary
- #200 [P1] WASM secret boundary & runtime/platform evidence
- #271 [P1] Lightning — implement LDK payment execution (structural)

### L1 Nexus (conxian-nexus) — 2 open
- #251 Wire `IdempotencyStore` to Neon + live-DB conformance suite
- #174 [governance] License policy & source/dependency licensing

### L1 Gateway (conxian-gateway) — 1 open
- #189 [research] BitVM3 adapter — garbled circuits & recursive proof verification

### L0 Protocol/DAO (Conxian) — 9 open
- #488 [CON-1427] 2% protocol fee collection
- #496 Partnership fee contracts
- #500 Production oracle source config & DEX deployment
- #507 sBTC vault implementation
- #515 Main-branch merge gates & CODEOWNERS
- #527 Partnership fee policy/legal/asset scope
- #529 Partner usage ledger & atomic split settlement
- #530 Partnership gateway, Stacks.js SDK, event indexing
- #532 Partnership security/legal/commercialization launch gate

### L2 Application surface (conxius-wallet / conxian_ui / conxian_market) — 5 open
- conxius-wallet #444 [P0] Centralized value-operation gate; quarantine software/synthetic success
- conxius-wallet #356/#357 CI/CD baseline & tech-debt hardening
- conxian_ui #161 Preview/production deployment evidence
- conxian_market #8 Treasury dashboard monthly reporting

### L3 Control plane (conxius-platform) — 7 open
- #1167 [ORG-WIDE] Protocol Handoff & Routing Layer Alignment
- #1168 [research] Founder Rights & Revenue Routing
- #854 Security rulesets / push protection
- #958 Auto-merge across repositories
- #1082 Missing CI validation scripts
- #1212 Stale branch review
- #1223 [P2][Security] Activate org-wide rulesets (evaluate-only)

### L3 Strategy (conxian-business) — 7 open
- #934 [BOS-001][Gate 2] Safe authority-transfer semantics
- #935 [BOS-001][Gate 3] Testnet rehearsal, readback, failure drills
- #936 [BOS-001][Gate 4] Hardware-backed signing/attestation & owner
- #937 [BOS-001][Gate 5] Independent security/release acceptance
- #938 [BOS-001][Gate 6] Mainnet handoff and post-state readback
- #940 [Research][FIBO] Pin ontology provenance, notices, trademark boundary
- #989 [Strategy] Conxian position research

### L3 Governance (.github / conxian.github.io) — 5 open
- .github #43 Org-wide GitHub operating model alignment
- .github #47 Security boundary & secret-prevention baseline
- .github #53 Public repository presentation metadata
- .github #60 Portfolio metadata, notices, and CI controls
- conxian.github.io #3 GitHub Pages deployment restore

### Cross-repo alignment gates (reconciled from Gateway #189)

| Original gate (2026-07-22) | Current state |
|----------------------------|---------------|
| Platform #1187 (simulation success quarantine) | Closed — superseded by Platform #1167 (ORG-WIDE alignment) |
| Nexus #169 (BLS12-381 ↔ BN254 reconcile) | Closed — Nexus now on v0.4.22; proof-surface ownership tracked separately |
| Enclave #202 (release acceptance) | **Still open [P0]** — blocks production proof enablement |
| Wallet #427 (simulation quarantine) | Closed — superseded by Wallet #444 (value-operation gate) |
| Core #188 (structural verifier) | Closed — fail-closed boundary recorded |
| .github #41 (readiness wording) | Closed |

---

## 5. Knowledge Base Inventory

Root-level KB files (markdown) per repository, observed live.

| Repository | Key KB artifacts |
|-----------|------------------|
| `conxian-gateway` | `AGENTS.md`, `PRD.md`, `docs/` (≈50 research/session/gap docs incl. `CROSS_REPO_STATUS.md`, `PORTFOLIO_MAP.md`, `GAP_ANALYSIS_*.md`, `research/*`) |
| `conxius-enclave-sdk` | `AGENTS.md`, `PRODUCTION_READINESS.md`, `GOVERNANCE.md`, `DEBT_INVENTORY.md`, `TRACKING.md`, `RESEARCH_LOG.md`, `SESSION_HISTORY.md`, `REPO_OWNERSHIP.md`, `docs/` |
| `conxian-nexus` | `AGENTS.md`, `README.md`, `SECURITY.md`, `docs/` (ADR-006, PRD, OpenAPI) |
| `lib-conxian-core` | `AGENTS.md`, `README.md`, `CHANGELOG.md`, `docs/` (MIGRATION, COMPATIBILITY, SIGNING_ARCHITECTURE) |
| `Conxian` | `AGENTS.md`, `PRD.md`, `SYSTEM_INDEX.md`, `CXIP-013/014`, `REPO_OWNERSHIP.md`, `docs/` |
| `conxian-business` | `AGENTS.md`, `BOS_KNOWLEDGE_GRAPH.md`, `DEPENDENCY_BASELINE.md`, `GOVERNANCE.md`, `spec.md`, `docs/` |
| `conxius-platform` | `AGENTS.md`, `GOVERNANCE.md`, `RELEASE_CONTROL.md`, `RELEASE_POLICY.md`, `REVIEWS.md`, `SESSION.md`, `docs/` |
| `conxius-wallet` | `AGENTS.md`, `BOS_KNOWLEDGE_GRAPH.md`, `docs/` |
| `conxian_ui` | `AGENTS.md`, `ARCHITECTURE.md`, `ALIGNMENT_PLAN.md`, `DEPLOYMENT.md`, `REPO_OWNERSHIP.md`, `progress.md`, `docs/` |
| `conxian_market` | `AGENTS.md`, `ROADMAP.md`, `docs/` |
| `.github` | `REVIEWS.md`, `repository-taxonomy.md`, `docs/` |
| `conxian-labs-site` | `AGENTS.md`, `DOMAIN_CUTOVER.md`, `GOVERNANCE.md`, `REPO_OWNERSHIP.md`, `progress.md`, `docs/` |
| `conxian.github.io` | `CONTRIBUTING.md`, `README.md` |

The single richest alignment KB in this repository is `docs/CROSS_REPO_STATUS.md`;
its historical snapshot is dated 2026-07-22 with session updates through
2026-08-20, so it does not yet reflect the 2026-08-30 state captured here.

---

## 6. Alignment Gaps & Recommended Actions

### G-1 (Dependency drift) — `conxius-enclave-sdk` v2.0.16 vs v2.0.17
- v2.0.17 was published 2026-08-30. The Gateway inherits v2.0.16 through
  `lib-conxian-core` v0.3.2, so this is a `lib-conxian-core` release-coordination
  item, not a Gateway-only change.
- **Action:** track a `lib-conxian-core` bump that re-pins the SDK; do not
  hand-patch the Gateway lockfile.

### G-2 (Proof-surface ownership) — Gateway BN254 vs Nexus BLS12-381/Arkworks
- Gateway exposes a backend-neutral BN254 Groth16 envelope; Nexus is the
  "Glass Node" proof layer with Arkworks/BLS12-381 verification. The curve and
  verifier-ownership contract remains unresolved across the two surfaces.
- **Action:** resolve one explicit curve / VK / public-input / state-root /
  verifier-ownership contract (the long-standing "next push" from #189).

### G-3 (Production proof enablement) — Enclave SDK #202 (P0) still open
- The enclave SDK is beta/conditional. Gateway must keep BitVM/BitVM3/Liquid
  fail-closed until #202 (and #240/#241/#242) close.
- **Action:** no Gateway promotion of these lanes until enclave release-acceptance
  evidence lands.

### G-4 (Value-operation alignment) — Wallet #444 (P0)
- Wallet still lacks a centralized value-operation gate. This is the wallet-side
  counterpart of Gateway's fail-closed discipline.
- **Action:** align Gateway settlement authorization with the wallet's eventual
  value-operation gate once #444 lands.

### G-5 (ORG-WIDE routing alignment) — Platform #1167
- Platform #1167 formalizes "Conxian-Labs owns infrastructure, community owns
  protocol" routing. Gateway is positioned as the routing middleware layer.
- **Action:** keep Gateway as a backend-neutral router; do not introduce
  protocol-owned settlement authority that would conflict with this directive.

### G-6 (KB freshness) — `docs/CROSS_REPO_STATUS.md`
- The canonical cross-repo KB is dated. This audit supersedes it for 2026-08-30.
- **Action:** refresh `CROSS_REPO_STATUS.md` (or treat this document as the
  current snapshot) in the next session.

---

*Generated by an AI agent (OpenHands) on 2026-08-30 from live GitHub API data.*
