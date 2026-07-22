# Conxian Gateway — Knowledge Map

> Generated: 2026-07-21 | All P1/P2 strategic gaps implemented | P3 research-only

---

## Architecture Overview

```
                            ┌──────────────────────────────────────────────┐
                            │            Conxian Gateway (Axum)             │
                            │                                              │
                            │  ┌──────────┐  ┌──────────┐  ┌───────────┐  │
                            │  │ REST API │  │   Auth   │  │  Metrics   │  │
                            │  │ (16 EP)  │  │ (Bearer) │  │ (Prometheus)│  │
                            │  └────┬─────┘  └──────────┘  └───────────┘  │
                            │       │                                      │
                            │  ┌────┴─────────────────────────────────┐    │
                            │  │         UCV-1 Universal Verifier       │    │
                            │  │  (Ecdsa,Schnorr,Zkml,BitVm,Cbtc)     │    │
                            │  └────┬─────────────────────────────────┘    │
                            │       │                                      │
                            │  ┌────┴──────────────┬──────────────────┐    │
                            │  │   Compliance      │    Engine         │    │
                            │  │   (ZKC Pipeline)  │  (12 Adapters)   │    │
                            │  └───────────────────┴──────────────────┘    │
                            └──────────────────────────────────────────────┘
                                     │                      │
                            ┌────────┴──────┐    ┌─────────┴──────────┐
                            │  Sanctions     │    │  Protocol Adapters │
                            │  Screening     │    │  ┌──────────────┐  │
                            │  (OFAC/EU/UN)  │    │  │ Bitcoin Core │  │
                            └───────────────┘    │  ├──────────────┤  │
                                                  │  │ Lightning    │  │
                                                  │  ├──────────────┤  │
                            ┌───────────────┐    │  │ Liquid       │  │
                            │  Persistence   │    │  ├──────────────┤  │
                            │  (Atomic FS)   │    │  │ Stacks       │  │
                            └───────────────┘    │  ├──────────────┤  │
                                                  │  │ Rootstock    │  │
                                                  │  ├──────────────┤  │
                                                  │  │ RGB v0.12    │  │
                                                  │  ├──────────────┤  │
                                                  │  │ Babylon      │  │
                                                  │  ├──────────────┤  │
                                                  │  │ BitVM2       │  │
                                                  │  ├──────────────┤  │
                                                  │  │ Fedimint     │  │
                                                  │  ├──────────────┤  │
                                                  │  │ Citrea       │  │
                                                  │  ├──────────────┤  │
                                                  │  │ Strata       │  │
                                                  │  ├──────────────┤  │
                                                  │  │ RISC Zero ◐  │  │
                                                  │  └──────────────┘  │
                                                  └────────────────────┘
```

---

## Protocol Adapter Matrix

### Bitcoin Stack (Layer 0 → Layer 2)

| Protocol | Adapter | Status | Key Functions |
|:---|:---|:---|:---|
| **Bitcoin L1** | `internal/engine/src/bitcoin/` | ✅ Live | Block listener, UTXO tracking, reorg detection |
| **Lightning** | `internal/api/src/lightning.rs` | ✅ Live | x402 payment execution, replay guard, retry |
| **Liquid** | `internal/engine/src/bitcoin/liquid_adapter.rs` | 🟡 Harnessed / fail-closed proof boundary | Elements peg-in/peg-out harness; production state-proof backend unwired |
| **RGB v0.12** | `internal/engine/src/bitcoin/rgb_adapter.rs` + `rgb_native.rs` + `rgb_stash.rs` | ✅ Live | StashResolver (P1), ContractVerify pending (P2), consignment pending |
| **BitVM2** | `internal/engine/src/bitcoin/bitvm_adapter.rs` | 🟡 Boundary | Metadata adapter plus validated Groth16 envelope handoff; cryptographic backend pending |
| **BitVM3 / BitVMX** | [`docs/research/BITVM3_BITVMX_RESEARCH_EXPANSION.md`](./BITVM3_BITVMX_RESEARCH_EXPANSION.md) + `tools/bitvmx-eval/` | 🔬 Research only | BitVM3/GC are not integrated; BitVMX-CPU evaluator only; no production cryptographic verifier or settlement adapter |
| **Babylon** | `internal/engine/src/bitcoin/babylon_adapter.rs` | 🟡 Boundary stub | Header-chain/SPV verification pending while PR #253 is open |
| **Fedimint** | `internal/engine/src/bitcoin/fedimint_adapter.rs` | ✅ Live | Federated e-cash mint coordination |
| **Strata** | `internal/engine/src/bitcoin/strata_adapter.rs` | ✅ Testnet | Bitcoin rollup bridge |
| **RISC Zero** | `internal/engine/src/bitcoin/risc0_verifier.rs` | 🟡 Unwired | ZK proof verifier for Bitcoin state transitions |

### NTT (Non-Traditional Transfers)

| Protocol | Adapter | Status | Key Functions |
|:---|:---|:---|:---|
| **Rootstock** | `internal/engine/src/ntt/rootstock_adapter.rs` | ✅ Live | RSK peg-in/peg-out, merged mining monitor |
| **Citrea** | `internal/engine/src/ntt/citrea_adapter.rs` | ✅ Live | Groth16 settlement, BitVM bridge verification |

### Messaging & Rails

| Protocol | Implementation | Status |
|:---|:---|:---|
| **NWC NIP-47** | `internal/api/src/nwc_backend.rs` | ✅ Integrated |
| **Musig2** | `internal/api/src/handlers.rs` (`aggregate_musig2_keys`) | ✅ Key aggregation |
| **x402** | `internal/api/src/x402.rs` | ✅ Payment protocol |
| **DLC** | `POST /api/v1/dlc/bond` | ⚠️ Bond/API scaffold only; no cryptographic oracle verification or CET construction. See [`DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`](DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md). |

---

## BitVM3 / BitVMX Research Boundary (#189)

- Canonical evidence and promotion gates: [`BITVM3_BITVMX_RESEARCH_EXPANSION.md`](./BITVM3_BITVMX_RESEARCH_EXPANSION.md).
- [`tools/bitvmx-eval/`](../../tools/bitvmx-eval/) and [`BITVMX_EVAL.md`](./BITVMX_EVAL.md) are isolated, feature-gated BitVMX-CPU evaluation tooling; they are not BitVM3, BitVMX-GC, garbled-circuit verification, Groth16 verification, settlement, or compliance paths.
- [`bitvm_adapter.rs`](../../internal/engine/src/bitcoin/bitvm_adapter.rs) parses and validates the canonical envelope and delegates to an injected verifier. Its legacy state-proof method remains metadata-only.
- [`groth16_verifier.rs`](../../internal/engine/src/bitcoin/groth16_verifier.rs) defines a backend-neutral boundary and a deterministic fixture mock; it does not perform cryptographic Groth16 pairings.
- [`UniversalVerifier`](../../internal/compliance/src/verifier.rs) has no special production Groth16, BitVM3, BitVMX-GC, or recursive SNARK wiring.

---

## Compliance Pipeline

```
    Transaction Request
           │
    ┌──────┴──────┐
    │  Sanctions   │  ← OFAC SDN, EU Consolidated, UN Security Council
    │  Screening   │
    └──────┬──────┘
           │
    ┌──────┴──────┐
    │  Jurisdiction│  ← Source/destination jurisdiction classification
    │  Routing     │
    └──────┬──────┘
           │
    ┌──────┴──────┐
    │  ZKC Audit   │  ← Zero-Knowledge Compliance trail
    │  Trail       │
    └──────┬──────┘
           │
        Approved / Rejected
```

### SanctionsRisk Tiers
| Risk | Criteria | Action |
|:---|:---|:---|
| **Low** | Canton↔Ethereum, peaq↔Bitcoin, intra-BTC-stack | Auto-approve |
| **Medium** | CIPS, PAPSS, mBridge, unknown chains | Manual review |
| **High** | SPFS, BRICS-Pay-DCMS | Block by default |
| **Critical** | OFAC SDN match, escalated High | Hard block |

---

## Identity Systems

```
    ┌─────────────────────────────────────────────┐
    │              Identity Resolution              │
    │                                              │
    │  ┌──────────┐  ┌──────────┐  ┌───────────┐  │
    │  │  Human    │  │ Machine  │  │ Protocol   │  │
    │  │  (DID)    │  │ (peaq)   │  │ (NWC NIP)  │  │
    │  └────┬─────┘  └────┬─────┘  └─────┬──────┘  │
    │       │             │              │          │
    │  ┌────┴─────────────┴──────────────┴────┐     │
    │  │       Identity Exchange Layer         │     │
    │  │    (exchange + resolve endpoints)     │     │
    │  └──────────────────────────────────────┘     │
    └──────────────────────────────────────────────┘
```

### Identity Types
| Type | DID Format | Resolution | Issue |
|:---|:---|:---|:---|
| **Human** | `did:stack:STX_ADDR` | `POST /api/v1/identity/resolve` | Shipped |
| **Machine (peaq)** | `did:peaq:PEAQ_ADDR` | `POST /api/v1/identity/resolve/machine` | G-C2 ✅ |
| **Machine (DIMO)** | Vehicle ID (VIN-linked) | `POST /api/v1/identity/resolve/machine` | G-C2 ✅ |
| **Machine (device_key)** | Schnorr pubkey | `POST /api/v1/identity/resolve/machine` | G-C2 ✅ |

### Machine Types (G-C2)
EV, Drone, Robot, Sensor, ComputeNode, Charger, Storage, Transmitter, Vehicle, Gateway, Other

---

## Settlement Rails

```
    Settlement Request
           │
    ┌──────┴──────────────────┐
    │  SettlementSource       │
    │  ┌───────────────────┐  │
    │  │ JobCard           │  │  ← Human-to-human (DLC/Stacks)
    │  │ MachineToMachine  │  │  ← G-C3: Autonomous M2M
    │  │ LightningOnly     │  │  ← Lightning-native
    │  │ FiatRamp          │  │  ← A2P/OTP
    │  │ NwcRelay          │  │  ← Nostr Wallet Connect
    │  └───────┬───────────┘  │
    └──────────┼──────────────┘
               │
    ┌──────────┴──────────────────────────────┐
    │           M2M Settlement Rails            │
    │  ┌──────────┐  ┌──────────┐  ┌────────┐  │
    │  │Lightning │  │  peaq     │  │BTC L1  │  │
    │  │(Live)    │  │(Q4 2026) │  │(Q4 2026)│  │
    │  └──────────┘  └──────────┘  └────────┘  │
    │  ┌──────────┐                            │
    │  │Taproot   │  ← Taproot Assets           │
    │  │Assets    │    (Q4 2026)                │
    │  └──────────┘                            │
    └──────────────────────────────────────────┘
```

### Machine Services (G-C3)
Charging, Data, Compute, Storage, Delivery, Other

---

## Canton Network Integration

```
    ┌────────────────────────────────────────────────────────────┐
    │                 Canton Network (Public L1)                   │
    │                                                             │
    │  ┌──────────────────┐  ┌──────────────────┐                │
    │  │  Canton Coin (CC) │  │  CBTC (BitSafe)   │                │
    │  │  Native token    │  │  Wrapped Bitcoin  │                │
    │  └────────┬─────────┘  └────────┬─────────┘                │
    │           │                     │                           │
    │  ┌────────┴─────────────────────┴─────────┐                │
    │  │            Daml Smart Contracts         │                │
    │  │  (AssetTransfer, Token, Dvp, Swap...)  │                │
    │  └────────┬───────────────────────────────┘                │
    │           │                                                 │
    │  ┌────────┴────────────────────────────────┐               │
    │  │       Conxian Gateway (Observe-Only)     │               │
    │  │  ┌────────────────┐  ┌────────────────┐  │               │
    │  │  │ CBTC Verify    │  │ State Translate│  │               │
    │  │  │ (G-C1)         │  │ (G-C4)         │  │               │
    │  │  └────────────────┘  └────────────────┘  │               │
    │  │  ┌────────────────┐  ┌────────────────┐  │               │
    │  │  │ CCIP Route     │  │ Atomic Swap    │  │               │
    │  │  │ (G-C5)         │  │ (G-C7: Q1 27)  │  │               │
    │  │  └────────────────┘  └────────────────┘  │               │
    │  └──────────────────────────────────────────┘               │
    │                                                             │
    │  Key: Conxian observes — never a validator — never custody  │
    └────────────────────────────────────────────────────────────┘
```

### Canton Research Notes (2026-07-06)
- **~780 validators**, ~600 nodes, Canton 3.5.6 (June 2026)
- **$344.83B** represented asset value (RWA.xyz)
- **BitSafe CBTC**: FROST attestation by Kiln + Figment, validator-scoped state
- **LayerZero**: Live on Canton since March 2026 → 165+ blockchains
- **Zenith**: Atomic swap engine (Canton↔Ethereum), emerged March 2026
- **Chainlink Data Streams**: Integration guide published, requires Canton Party ID + DAR upload
- **Observer Nodes**: Officially supported (OPN) — read-only, cannot submit/confirm
- **ByBit, DTCC, Franklin Templeton, J.P. Morgan Kinexys**: All active on Canton
- **HSBC Orion, Goldman Sachs DAP, BNP Paribas Neobonds**: Operating with limited visibility

### Conxian Posture
- **Route without touching**: Verify CBTC attestations without joining FROST signer set
- **Observe without validating**: Read Canton state via OPN without running a validator
- **Screen without participating**: Route CCIP messages through compliance without CCIP consensus
- **No custody**: All paths are non-custodial — machines and users hold their own keys

---

## Machine Economy Integration

```
    ┌──────────────────────────────────────────────────────────────┐
    │                      Machine Economy                          │
    │                                                               │
    │  ┌──────────┐    ┌──────────┐    ┌──────────┐                │
    │  │  peaq L1  │    │  DIMO     │    │  Canton   │               │
    │  │ (DePIN)   │    │ (Vehicle) │    │  (RWA)    │               │
    │  └────┬─────┘    └────┬─────┘    └────┬─────┘               │
    │       │               │               │                       │
    │  ┌────┴───────────────┴───────────────┴────┐                  │
    │  │         Machine Identity Layer           │                  │
    │  │   (DID + device_key + attestation)       │                  │
    │  └────────────────────┬────────────────────┘                  │
    │                       │                                       │
    │  ┌────────────────────┴────────────────────┐                  │
    │  │            Revenue Routing                │                  │
    │  │  ┌──────────┐  ┌──────────┐  ┌────────┐  │                  │
    │  │  │ M2M Pay   │  │ RWA Proof │  │ Yield  │  │                  │
    │  │  │ (G-C3)    │  │ (G-C6)    │  │ Dist   │  │                  │
    │  │  └──────────┘  └──────────┘  └────────┘  │                  │
    │  └────────────────────┬────────────────────┘                  │
    │                       │                                       │
    │  ┌────────────────────┴────────────────────┐                  │
    │  │        Token Holder Distribution          │                  │
    │  │    90% to RWA holders via Lightning       │                  │
    │  └──────────────────────────────────────────┘                  │
    └──────────────────────────────────────────────────────────────┘
```

### peaq Research Notes (2026-07-06)
- **60+ DePINs** across 22 industries, millions of on-chain devices
- **$180M TVL** in machine wallets, 12,000+ daily active devices
- **Machine RWA Framework**: Registration → Issuance → Revenue Routing → Compliance
- **SDK**: EVM + ink! smart contracts, DePIN SDK, self-sovereign machine identities
- **Enterprise**: Mastercard, Bosch, Tether (QVAC for private edge AI inference)
- **x402**: Thirdweb integration for internet-native payments
- **ELOOP**: 100+ Tesla fleet, €2M+ cumulative transactions, tokenized ride revenue

---

## Complete API Surface (16 endpoints)

### Core (6 endpoints)
```
GET    /api/v1/health                      → Health check
GET    /api/v1/metrics                     → Prometheus metrics
GET    /api/v1/state                       → Gateway state
POST   /api/v1/verify                      → UCV-1 attestation verify
POST   /api/v1/identity/exchange           → Identity exchange
POST   /api/v1/identity/resolve            → Human identity resolution
```

### Canton Network (4 endpoints) ← NEW P1/P2
```
POST   /api/v1/canton/cbtc/verify          → G-C1: CBTC non-custodial verify
POST   /api/v1/canton/state/translate      → G-C4: Daml ACS → UniversalRef
POST   /api/v1/ccip/route                  → G-C5: CCIP compliance routing
```

### Machine Economy (3 endpoints) ← NEW P1/P2
```
POST   /api/v1/identity/resolve/machine    → G-C2: Machine identity resolution
POST   /api/v1/m2m/settle                  → G-C3: M2M settlement routing
POST   /api/v1/rwa/machine/verify-revenue  → G-C6: Machine RWA revenue verify
```

### Financial (3 endpoints)
```
POST   /api/v1/fiat/session                → Fiat ramp session
POST   /api/v1/fiat/webhook                → Fiat webhook verify
POST   /api/v1/a2p/otp                     → A2P OTP send
POST   /api/v1/a2p/verify                  → A2P OTP verify
```

### Bitcoin Stack (3 endpoints)
```
POST   /api/v1/dlc/bond                    → DLC bond scaffold (mock ID; no CET)
POST   /api/v1/musig2/aggregate-keys       → Musig2 key aggregation
POST   /api/v1/chains/{chain}/verify       → Chain state proof verify
```

### Protocol-Specific (4 endpoints)
```
GET    /api/v1/chains/list                 → Supported chains
GET    /api/v1/chains/{chain}/height       → Chain height
POST   /api/v1/chains/{chain}/prepare      → Prepare chain TX
POST   /api/v1/nwc/relay                   → NWC relay settle
POST   /api/v1/verify/worldcoin            → World ID verify
```

---

## Strategic Gap Status

### P1 (Q3 2026) — Shipped ✅
| Gap | Endpoint | Checks |
|:---|:---|:---|
| G-C1 CBTC | `POST /canton/cbtc/verify` | 6-point (contract_id, amount, utxo, quorum, frost, domain) |
| G-C2 Machine ID | `POST /identity/resolve/machine` | 3 providers (peaq, dimo, device_key) |
| G-C3 M2M Settlement | `POST /m2m/settle` | Lightning live, 3 rails roadmap Q4 |

### P2 (Q4 2026) — Shipped ✅
| Gap | Endpoint | Notes |
|:---|:---|:---|
| G-C4 State Translation | `POST /canton/state/translate` | Daml template-aware mapping |
| G-C5 CCIP Connector | `POST /ccip/route` | 6-chain risk classification + escalation |
| G-C6 Machine RWA | `POST /rwa/machine/verify-revenue` | 5-point check, 90% holder distribution |

### P3 (Q1 2027) — Research 🟡
| Gap | Blockers |
|:---|:---|
| G-C7 Canton↔Bitcoin Atomic Swap | Requires Daml↔Bitcoin HTLC/PTLC script compilation; LayerZero and Zenith already providing Canton↔EVM swaps |
| G-C8 DePIN Compliance ZKC | Requires jurisdictional tax classification for autonomous machine income |

---

## Interoperability Map

```
                    ┌──────────┐
                    │  Bitcoin  │
                    │    L1     │
                    └─────┬─────┘
            ┌─────────────┼─────────────┐
            │             │             │
       ┌────┴────┐   ┌────┴────┐   ┌────┴────┐
       │Lightning│   │ Liquid  │   │  RGB    │
       │  (LN)   │   │ (L-BTC) │   │ (v0.12) │
       └────┬────┘   └────┬────┘   └────┬────┘
            │             │             │
       ┌────┴─────────────┴─────────────┴────┐
       │          Conxian Gateway             │
       │         (Route + Verify)             │
       └────┬──────────────┬────────────┬─────┘
            │              │            │
    ┌───────┴───────┐ ┌────┴─────┐ ┌───┴──────────┐
    │  Canton (L1)  │ │ peaq (L1)│ │  165+ chains  │
    │  ┌──────────┐ │ │ ┌──────┐ │ │ (via LayerZero)│
    │  │  CBTC    │ │ │ │DePIN │ │ └───────────────┘
    │  │ (BitSafe)│ │ │ │ 60+  │ │
    │  ├──────────┤ │ │ ├──────┤ │
    │  │ Tokenized│ │ │ │RWA   │ │
    │  │ Assets   │ │ │ │Machin│ │
    │  │ ($344B)  │ │ │ │e NFTs│ │
    │  └──────────┘ │ │ └──────┘ │
    └───────────────┘ └──────────┘
            │              │
    ┌───────┴───────┐      │
    │  Zenith       │      │
    │ (Canton↔EVM   │      │
    │  Atomic Swap) │      │
    └───────────────┘      │
                           │
    ┌──────────────────────┘
    │  peaq↔Polkadot XCM
    │  peaq↔30+ chains (bridge)
    └──────────────────────
```

### Key Interop Protocols
| Path | Protocol | Status |
|:---|:---|:---|
| Canton→Ethereum | Zenith atomic swaps | Live (March 2026) |
| Canton→165+ chains | LayerZero | Live (March 2026) |
| Canton→Chainlink | CCIP Data Streams | Integration guide published |
| peaq→Polkadot | XCM native | Live |
| peaq→30+ chains | peaq Bridge | Live |
| Conxian→Canton | G-C1/C-C4 (observe-only) | ✅ Shipped |
| Conxian→peaq | G-C2/G-C3/G-C6 (identity+M2M) | ✅ Shipped (Lightning), Q4 2026 (peaq native) |

---

## File Map

```
conxian-gateway/
├── cmd/gateway/          # Entry point, config, wiring
├── internal/
│   ├── api/src/
│   │   ├── handlers.rs   # 16 endpoint handlers
│   │   ├── routes.rs     # Axum router
│   │   ├── lightning.rs  # Lightning adapter (x402)
│   │   ├── x402.rs       # X402 payment protocol
│   │   ├── nwc_backend.rs # NWC NIP-47
│   │   └── world_id.rs   # Worldcoin verification
│   ├── engine/src/
│   │   └── bitcoin/      # 9 Bitcoin stack adapters
│   │   └── ntt/          # 2 NTT adapters (Rootstock, Citrea)
│   └── compliance/       # ZKC pipeline, sanctions screening
├── pkg/conxian-core/
│   └── src/
│       ├── lib.rs        # 60+ shared types (models, DIDs, attestations)
│       ├── settlement.rs # Settlement types, sources, rails
│       ├── lightning.rs  # Lightning types
│       ├── musig2.rs     # Musig2 key aggregation
│       └── persistence.rs # Atomic filesystem persistence
├── apps/control-plane/   # Next.js dashboard
├── packages/
│   ├── client-sdk/       # TypeScript client SDK (vitest)
│   └── schemas/          # Shared JSON schemas
├── docs/research/        # Research documents
│   ├── KNOWLEDGE_MAP.md  # This file
│   ├── CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md
│   ├── CANDIDATE_MATRIX.md
│   ├── OPPORTUNITY_MAP_AND_EXPANSION.md
│   ├── BITVM3_BITVMX_RESEARCH_EXPANSION.md
│   └── BITVMX_EVAL.md
└── scripts/
    └── verify_contamination_guard.py
```

---

## Verification Protocol

```
1. cargo fmt --all -- --check
2. cargo clippy --workspace --all-targets --all-features -- -D warnings
3. cargo test --workspace && cargo test --workspace --features mock-integrations
4. pnpm install && pnpm build && pnpm test
5. GET /api/v1/health → HTTP 200 `{"status":"ok"}`
6. python3 scripts/verify_contamination_guard.py
```

---

## CI/CD Pipelines

| Pipeline | Trigger | Scope |
|:---|:---|:---|
| `rust-ci.yml` | PR + push to main | fmt, clippy, test (incl. mock-integrations), release build |
| `node-ci.yml` | PR + push to main | pnpm install, build, test (vitest + playwright) |
| `lightning-coverage.yml` | Push | Lightning-scoped coverage ≥90% |
| `cargo-audit.yml` | Weekly (cron) | Dependency audit + CVE triage |
| `secret-scan.yml` | PR | Gitleaks secret scanning |
| `release.yml` | Tag `v*` | GitHub Release + SBOM (CycloneDX) + SLSA L3 provenance |
