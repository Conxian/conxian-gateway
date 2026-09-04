# Canton Network & Machine Economy: Strategic Research for Conxian Gateway

**Date**: 2026-07-06
**Status**: Research — Active
**Scope**: Canton Network state translation, non-custodial capital routing, Machine Economy monetization

> **DLC status boundary — 2026-07-22:** DLC orchestrator, CBTC DLC
> verification, and Canton-to-Bitcoin DLC references in this document are
> strategic research and target architecture. They do not describe a live
> gateway CET engine or cryptographic oracle verifier. The current implementation
> boundary and promotion gates are recorded in
> [`DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md`](DLC_ECOSYSTEM_AND_MAINNET_EVIDENCE.md).

---

## Executive Summary

Canton Network represents the institutional privacy-preserving DLT frontier — a permissioned "network of networks" that already tokenizes $6T+ in real-world assets across Goldman Sachs, BNP Paribas, Deutsche Börse, and others. Its eUTXO model is architecturally adjacent to Bitcoin but philosophically divergent: Canton prioritizes sub-transaction privacy and configurable governance, while Conxian prioritizes non-custodial sovereignty and permissionless verification.

**The opportunity is not to compete with Canton, but to serve as the sovereign routing layer between Canton's institutional capital and Bitcoin's permissionless settlement.** This is "routing without touching" — Conxian never takes custody, but enables value to flow across the boundary.

The Machine Economy (DePIN, peaq, M2M Lightning settlements) represents a parallel growth vector where Conxian's non-custodial ethos directly aligns with machines owning their own wallets and settling autonomously.

---

## Part 1: Canton Network Deep Dive

### 1.1 What is Canton?

Canton is a privacy-enabled DLT protocol developed by Digital Asset (founded 2014). It powers the **Canton Network** — a "network of networks" connecting sovereign participant nodes through shared synchronizer infrastructure.

| Attribute | Canton Network | Bitcoin | Conxian Gateway |
|:---|:---|:---|:---|
| **Ledger Model** | eUTXO (typed contracts) | UTXO | UTXO-aware orchestration |
| **Privacy** | Sub-transaction (need-to-know) | Pseudonymous (public) | ZKC pass-through |
| **Consensus** | Stakeholder-based 2PC + BFT ordering | PoW Nakamoto | Verification-only |
| **Smart Contracts** | Daml (Haskell-derived, functional) | Bitcoin Script | Multi-protocol adapters |
| **Governance** | Configurable (permissioned → public) | Permissionless | Institutional SLAs |
| **Interoperability** | Native cross-domain atomic | External (bridges, L2s) | Protocol-agnostic routing |
| **Target Market** | Regulated institutional finance | Global sovereign money | Both — sovereign routing layer |

### 1.2 Architecture Deep Dive

```
┌─────────────────────────────────────────────────────────┐
│                  CANTON NETWORK                          │
│                                                          │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐            │
│  │Participant│   │Participant│   │Participant│            │
│  │  Node A   │   │  Node B   │   │  Node C   │            │
│  │(Goldman)  │   │ (BNP Paribas)│ │(Deutsche) │            │
│  └─────┬─────┘   └─────┬─────┘   └─────┬─────┘            │
│        │               │               │                  │
│        └───────────────┼───────────────┘                  │
│                        │                                  │
│              ┌─────────┴─────────┐                        │
│              │    SYNCHRONIZER    │                        │
│              │  (Global + Domain) │                        │
│              │  • Orders messages │                        │
│              │  • Coordinates 2PC │                        │
│              │  • Blind to payload│                        │
│              └────────────────────┘                        │
│                                                          │
│  Key Properties:                                         │
│  • Each node only sees its entitled contracts             │
│  • Atomic cross-domain commits via 2PC                    │
│  • Daml contracts: immutable, create→exercise→archive     │
│  • Canton 3.x: no contract keys (limitation)              │
└─────────────────────────────────────────────────────────┘
```

### 1.3 Canton's eUTXO Model vs Bitcoin UTXO

Both use UTXO semantics, which makes state translation tractable:

| Concept | Canton (Daml) | Bitcoin |
|:---|:---|:---|
| **State unit** | Active Contract (typed) | Unspent Transaction Output |
| **Creation** | `create` Daml command | Transaction output |
| **Consumption** | `exercise` (consuming choice) | Transaction input |
| **Double-spend prevention** | Contract archival | UTXO set validation |
| **Metadata** | Rich typed fields (Party, Decimal, Text) | Script + amount (constrained) |
| **Ownership** | Signatory/Observer/Controller roles | Script-defined spending conditions |
| **Privacy** | Per-stakeholder views | Public (except Confidential Tx on Liquid) |

**Key insight**: Canton's Active Contract Set (ACS) is structurally isomorphic to Bitcoin's UTXO set. A contract `C` with signatories `[A, B]` and amount `V` maps to a Bitcoin UTXO with multisig `2-of-2(A, B)` locking `V` sats. This isomorphism is the foundation for state translation.

### 1.4 Canton's Bitcoin Integration (CBTC)

Canton does **not** have native Bitcoin bridging. The only documented bridge is **CBTC by BitSafe**:

- **Model**: 1:1 wrapped Bitcoin, threshold-signature attested
- **Custody model**: FROST-based threshold signatures (decentralized attestation, no single custodian)
- **Trust model**: Requires trust in the FROST signer set (not trustless like atomic swaps)
- **Status**: Launched on Canton, but low-level protocol specs are not public
- **Gap**: No SPV proofs, no PSBT flows, no Lightning integration documented

**Conxian opportunity**: Build a trustless, non-custodial Bitcoin↔Canton bridge using:
- DLCs for conditional Bitcoin lockup with Canton-side attestation
- Adaptor signatures (PTLCs) for atomic cross-chain settlement
- Discreet Log Contracts to prove Canton state on Bitcoin

### 1.5 Canton's Ecosystem Integrations

- **Chainlink CCIP**: Cross-Chain Interoperability Protocol integrated Sep 2025
- **LayerZero**: Connected March 2026 — 165+ blockchain reach
- **Fireblocks**: Institutional custody and settlement
- **Polyglot Canton (Feb 2025)**: EVM compatibility coming to Canton
- **Canton Coin (CC)**: Native utility token for fees, burn-mint equilibrium

---

## Part 2: State Translation — Conxian's Role

### 2.1 The State Translation Problem

Canton Daml contracts and Bitcoin UTXOs speak different languages. Conxian can serve as the **universal state translator**:

```
┌─────────────┐         ┌──────────────────┐         ┌─────────────┐
│   Canton    │ ◄─────► │  Conxian Gateway │ ◄─────► │   Bitcoin   │
│  (Daml/eUTXO)│         │  State Translator │         │   (UTXO)    │
└─────────────┘         └──────────────────┘         └─────────────┘
                                │
                        ┌───────┴───────┐
                        │  Stacks (Clarity)│
                        │  Liquid (CTx)   │
                        │  RGB (seals)    │
                        └───────────────┘
```

### 2.2 Translation Primitives

| Canton Concept | → | Conxian Translation | → | Bitcoin Artifact |
|:---|:---|:---|:---|:---|
| Daml `ContractId` | → | `UniversalContractRef` | → | `OutPoint (txid:vout)` |
| Daml `Party` | → | `SovereignIdentity` | → | `x-only pubkey` / `BNS name` |
| Daml `Numeric 10` | → | `Amount(sats)` | → | BTC output value |
| Daml `Time` | → | `LedgerTime` | → | Bitcoin block height/timestamp |
| Daml `Choice` (consuming) | → | `SettlementIntent` | → | PSBT input consumption |
| Daml `Choice` (non-consuming) | → | `Observation` | → | View-key disclosure |
| Daml `Signatory` set | → | `ThresholdPolicy` | → | MuSig2/FROST aggregate key |

### 2.3 Architecture: Canton Adapter for Conxian

```rust
// Proposed: internal/engine/src/institutional/canton_adapter.rs

#[async_trait]
pub trait CantonStateTranslator: Send + Sync {
    /// Translate a Canton Daml ACS contract into a Universal Contract Reference
    async fn translate_contract(
        &self,
        contract: CantonActiveContract,
    ) -> Result<UniversalContractRef, TranslationError>;

    /// Attest to Canton contract state via a threshold signature (for Bitcoin anchoring)
    async fn attest_contract_state(
        &self,
        contract_ref: &UniversalContractRef,
        quorum: &ThresholdConfig,
    ) -> Result<AttestationProof, AttestationError>;

    /// Observe Canton transaction finality via synchronizer timestamp
    async fn observe_finality(
        &self,
        tx_id: &CantonTransactionId,
        domain: &CantonDomain,
    ) -> Result<FinalityProof, FinalityError>;
}
```

### 2.4 Strategic Posture: Observe, Don't Embed

Conxian should **not** embed a Canton participant node. Instead:

1. **Observe**: Connect to Canton synchronizers as a read-only observer (where permitted)
2. **Attest**: Use the existing MuSig2/FROST infrastructure to co-sign state attestations
3. **Translate**: Map Canton ACS state → Universal Contract References → Bitcoin anchors
4. **Route**: Never hold Canton Coin or Daml assets — route settlement intents only

This preserves Conxian's non-custodial ethos while enabling institutional capital flow.

---

## Part 3: Non-Custodial Capital Routing

### 3.1 The Capital Routing Stack

```
     Institutional Side              Sovereign Side
     ───────────────────              ──────────────

  ┌──────────┐                    ┌──────────────┐
  │  Canton   │                    │   Bitcoin    │
  │  (Daml)   │                    │   (UTXO)     │
  │  $6T+ RWA │                    │   ₿21M cap   │
  └────┬─────┘                    └──────┬───────┘
       │                                 │
       │   ┌─────────────────────────┐   │
       └──►│   Conxian Gateway       │◄──┘
           │                         │
           │  • State Translation    │
           │  • Atomic Swap Engine   │
           │  • DLC Orchestrator     │
           │  • Threshold Attestation│
           │  • Compliance ZKC Pipe  │
           │  • Settlement Guard     │
           └───────────┬─────────────┘
                       │
           ┌───────────┴─────────────┐
           │                         │
     ┌─────┴─────┐            ┌──────┴──────┐
     │  Stacks   │            │  Lightning  │
     │  (sBTC)   │            │  (μBTC)     │
     └───────────┘            └─────────────┘
```

### 3.2 Routing Mechanisms (Custody-Never)

| Mechanism | Trust Model | Latency | Use Case |
|:---|:---|:---|:---|
| **HTLC Atomic Swaps** | Trustless (hash-lock) | ~60 min (2× confirmations) | Large-value cross-chain settlement |
| **PTLC (Adaptor Signatures)** | Trustless, private | ~30 min | Institutional atomic swaps |
| **DLC (Discreet Log Contracts)** | Oracle-attested | ~10 min (1 confirmation) | Conditional settlement based on Canton state |
| **FROST Threshold Attestation** | M-of-N signer quorum | ~5 sec | Fast bridge with decentralized custody |
| **Lightning + Taproot Assets** | Trustless channels | <1 sec | M2M micropayments, machine economy |
| **sBTC peg (Stacks)** | Trustless (Nakamoto) | ~100 blocks | Sovereign Bitcoin L2 settlement |

### 3.3 The "Sovereign Memo" Pattern

When institutional capital flows from Canton → Bitcoin via Conxian, the Gateway emits a **Sovereign Memo** — a compliance attestation that proves:

1. **Origin**: Canton domain, contract ID, involved parties
2. **Authorization**: Signatory attestation (Daml exercise proof)
3. **Translation**: How Canton state mapped to Bitcoin UTXO
4. **Settlement Intent**: Target Bitcoin address, amount, script conditions
5. **Compliance**: ZKC pass-through with jurisdictional tags (G7, BRICS, neutral)

The Sovereign Memo is **not stored** in Conxian — it's embedded in the Bitcoin transaction as `OP_RETURN` data or Taproot annex, then immediately discarded from Gateway memory.

### 3.4 Monetization: Fee Models That Preserve Sovereignty

| Fee Model | Mechanism | Sovereignty Impact |
|:---|:---|:---|
| **Basis-point routing fee** | 1-5 bps on settled volume | ✅ Non-custodial, proportional to value routed |
| **Attestation fee** | Flat fee per state attestation | ✅ Pay-per-proof, no ongoing custody |
| **Sovereign Memo stamp** | Fee per compliance memo embedded in tx | ✅ One-time, stateless |
| **Liquidity pulse subscription** | Monthly for mempool orchestration access | ✅ SaaS model, no asset custody |
| **Machine economy channel lease** | Lightning channel liquidity leasing | ✅ Non-custodial liquidity provisioning |
| **Verification-as-a-Service** | UCV-1 proof verification for institutional clients | ✅ Compute-only, no asset touch |

**Key principle**: Every fee model must pass the "custody test" — Conxian never holds, controls, or intermediates the underlying assets. Revenue comes from routing, attesting, and verifying — never from custody or lending.

---

## Part 4: Machine Economy Expansion

### 4.1 What is the Machine Economy?

The Machine Economy (aka Economy of Things) is the emerging paradigm where:
- **Machines own wallets** — non-custodial, with their own identities
- **Machines pay machines** — autonomous M2M value transfer
- **Machines earn** — providing services, selling data, leasing capacity
- **Humans co-own** — tokenized machine RWAs stream yield to human stakeholders

### 4.2 Key Protocols & Projects

| Protocol | Category | Scale (2026) | Relevance to Conxian |
|:---|:---|:---|:---|
| **peaq** | DePIN L1 (Polkadot parachain) | 60+ dApps, 500K+ machines, $180M TVL, 12K+ daily active devices | Machine identity standard, M2M settlement |
| **Helium** | Wireless DePIN (Solana) | 1M+ hotspots | Proven DePIN token incentive model |
| **Hivemapper** | Mapping DePIN (Solana) | 8M+ km mapped | Sensor data monetization |
| **DIMO** | Vehicle data DePIN | 100K+ connected cars | Machine RWA tokenization |
| **Lightning Network** | Bitcoin L2 payments | $1.1B/month volume, 5,637 BTC capacity, USDT live via Taproot Assets | **Primary M2M settlement rail** |
| **d402 / x402** | HTTP 402 payment protocol | Coinbase/DecentraLab | API-level M2M payments |
| **NATIX** | Drive-to-earn (peaq) | 40K+ smartphone cameras | Real-time machine data economy |
| **ELOOP** | Tokenized car sharing (peaq) | 100+ tokenized Teslas in Vienna | Machine RWA proof-of-concept |
| **375ai** | Edge AI (peaq) | Edge data intelligence | Machine-native AI agents |
| **MachineX** | Machine DeFi DEX (peaq) | First M2M DEX | Machine-native DeFi primitives |

### 4.3 Lightning as the Machine Economy Settlement Rail

The Lightning Network is converging on the ideal M2M payment infrastructure:

- **$1.1B monthly volume** (River, Nov 2025) — no longer a hobbyist network
- **USDT on Lightning** (Taproot Assets, Mar 2026) — stablecoin M2M settlement
- **$1M single transaction** (SDM → Kraken, Feb 2026) — institutional scale
- **Voltage revolving credit** (Feb 2026) — USD credit lines settling via Lightning
- **Machine-native**: Non-custodial, 24/7, sub-cent fees, instant finality

```
┌─────────────┐     Lightning      ┌─────────────┐
│  EV Charger │ ◄───────────────► │  EV Driver   │
│  (peaq ID)  │   pay-per-kWh     │  (LN wallet) │
└──────┬──────┘                    └──────┬──────┘
       │                                  │
       │    ┌─────────────────────────┐   │
       └───►│   Conxian Gateway       │◄──┘
            │                         │
            │  • Machine Identity     │
            │  • M2M Settlement Intent│
            │  • Compliance ZKC       │
            │  • Treasury Monitor     │
            └─────────────────────────┘
```

### 4.4 Conxian's Machine Economy Entry Points

#### 4.4.1 Machine Identity Verification (MIV-1)

Machines need sovereign identities to participate in non-custodial value exchange. Conxian's existing identity resolution stack (BNS, ENS, World ID) can extend to machine identities:

```rust
// Proposed extension to identity types
pub enum SovereignIdentity {
    Human(HumanIdentity),       // BNS, ENS, World ID
    Machine(MachineIdentity),   // peaq DID, DIMO Vehicle ID, IoT device key
    Organization(OrgIdentity),  // Legal entity
}

pub struct MachineIdentity {
    pub peaq_did: Option<String>,       // peaq decentralized identifier
    pub device_key: XOnlyPublicKey,     // Schnorr pubkey (Taproot-ready)
    pub attestation_proof: AttestationProof, // Manufacturer/DePIN attestation
    pub machine_type: MachineType,      // EV, drone, sensor, robot, etc.
}
```

#### 4.4.2 M2M Settlement Pipeline

The existing `SettlementSource` and `SettlementIntent` types extend naturally to machine settlement:

```rust
pub enum SettlementSource {
    // ... existing variants ...
    MachineToMachine {
        source_machine: MachineIdentity,
        target_machine: MachineIdentity,
        service_type: MachineService,      // charging, data, compute, storage
        settlement_rail: M2MSettlementRail, // Lightning, peaq, direct on-chain
    },
}
```

#### 4.4.3 Machine RWA Tokenization Gateway

Conxian can serve as the verification layer for tokenized machine RWAs:
- **Verify** machine identity and attestation proofs
- **Monitor** machine revenue streams (charging fees, data sales, compute leasing)
- **Route** yield distributions to token holders via Lightning
- **Comply** — ZKC pass-through for machine revenue (jurisdictional tax reporting)

### 4.5 Machine Economy Revenue Model

| Service | Description | Target Market |
|:---|:---|:---|
| **Machine identity attestation** | Verify DePIN device identity, issue sovereign machine DID | peaq, DIMO, Helium ecosystem |
| **M2M settlement routing** | Non-custodial routing between machine wallets via Lightning | EV charging, drone delivery, compute grid |
| **Machine RWA verification** | Prove machine revenue for tokenized machine RWAs | MachineX, ELOOP, tokenization platforms |
| **Compliance ZKC for machines** | Jurisdictional tax reporting for autonomous machine income | Regulated DePIN deployments |
| **Liquidity pulse for M2M** | Lightning channel management for high-frequency M2M flows | peaq↔Bitcoin bridges |

---

## Part 5: Knowledge Graph

### 5.1 Protocol Alignment Map

```
                          ┌──────────────────┐
                          │  Conxian Gateway  │
                          │  (Sovereign Router)│
                          └────────┬─────────┘
                                   │
          ┌────────────────────────┼────────────────────────┐
          │                        │                        │
    ┌─────┴─────┐           ┌──────┴──────┐          ┌──────┴──────┐
    │  Bitcoin  │           │ Sovereignty  │          │Institutional│
    │  Native   │           │   Bridge     │          │  Perimeter  │
    └─────┬─────┘           └──────┬──────┘          └──────┬──────┘
          │                        │                        │
    ┌─────┴────────┐        ┌──────┴──────────┐     ┌──────┴──────────┐
    │• BTC L1      │        │• Stacks/sBTC    │     │• Canton/Daml    │
    │• Lightning    │        │• Liquid/CTx     │     │• Fedimint       │
    │• DLC          │        │• RGB v0.12      │     │• Chainlink CCIP │
    │• BitVM2       │        │• Babylon        │     │• LayerZero      │
    │• Ark          │        │• Citrea (ZK)    │     │• BRICS/CIPS     │
    │• Strata (ZK)  │        │• Rootstock      │     │• mBridge        │
    │               │        │• RISC Zero      │     │• PAPSS          │
    └───────────────┘        └─────────────────┘     └─────────────────┘
          │                        │                        │
          │                  ┌─────┴─────┐                  │
          │                  │  Machine  │                  │
          │                  │  Economy  │                  │
          │                  └─────┬─────┘                  │
          │                       │                         │
          │                 ┌─────┴──────────┐              │
          │                 │• peaq (DePIN)  │              │
          │                 │• Lightning M2M │              │
          │                 │• Machine RWAs  │              │
          │                 │• DePAI/Edge AI │              │
          │                 └────────────────┘              │
          │                                                 │
    ┌─────┴─────────────────────────────────────────┐       │
    │          Non-Custodial Capital Routing          │       │
    │  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │       │
    │  │HTLC/PTLC │  │   DLC    │  │  Threshold   │ │       │
    │  │  Swaps   │  │Orchestr. │  │ Attestation  │ │       │
    │  └──────────┘  └──────────┘  └──────────────┘ │       │
    └───────────────────────────────────────────────┘       │
                                                            │
    ┌───────────────────────────────────────────────────────┘
    │
    │   ┌──────────────────────────────────────────────┐
    │   │         Compliance ZKC Pipeline              │
    │   │  • Sanctions-risk tagging (G7/BRICS/neutral) │
    │   │  • Jurisdictional sharding                   │
    │   │  • Sovereign Memo (OP_RETURN embed, discard) │
    │   │  • Zero PII persistence                      │
    │   └──────────────────────────────────────────────┘
```

### 5.2 Opportunity Heatmap

| Opportunity | Strategic Fit | Technical Feasibility | Time Horizon | Priority |
|:---|:---|:---|:---|:---|
| **Canton state translation adapter** | 🟢 High | 🟡 Medium (Daml runtime needed) | Q4 2026 | P2 |
| **CBTC non-custodial verification** | 🟢 High | 🟢 High (DLC + adaptor sigs) | Q3 2026 | P1 |
| **Machine identity DID integration** | 🟢 High | 🟢 High (extend existing ID stack) | Q3 2026 | P1 |
| **Lightning M2M settlement routing** | 🟢 High | 🟢 High (existing Lightning adapter) | Q3 2026 | P1 |
| **Machine RWA revenue verification** | 🟡 Medium | 🟡 Medium (requires DePIN oracle) | Q4 2026 | P2 |
| **Canton↔Bitcoin atomic swap engine** | 🟢 High | 🟡 Medium (cross-chain HTLC/PTLC) | Q1 2027 | P3 |
| **Chainlink CCIP Canton connector** | 🟡 Medium | 🟢 High (CCIP SDK available) | Q4 2026 | P2 |
| **DePIN compliance ZKC** | 🟡 Medium | 🟡 Medium (jurisdictional complexity) | Q1 2027 | P3 |

### 5.3 The Sovereignty Test

Every opportunity must pass Conxian's sovereignty test:

| Criterion | Canton Adapter | Machine Economy | BRICS/CIPS |
|:---|:---|:---|:---|
| **Non-custodial?** | ✅ Observe only, never hold | ✅ Machines hold own keys | ✅ Message routing only |
| **Sovereignty-preserving?** | ✅ User chooses to bridge | ✅ Machines are self-sovereign | ✅ Dual-stack, user selects rail |
| **Compliance pipe only?** | ✅ ZKC pass-through | ✅ Revenue attestation only | ✅ Sanctions tags, no PII |
| **Revenue without custody?** | ✅ Basis-point routing fee | ✅ Attestation + verification fees | ✅ Message normalization fees |
| **Institutional grade?** | ✅ SLA-grade adapters | ✅ M2M SLA (uptime, latency) | ✅ ISO 20022 + CIPS compliance |

---

## Part 6: Strategic Recommendations

### 6.1 Immediate (Q3 2026) — High Confidence

1. **CBTC Non-Custodial Verification (G-C1)**
   - Build DLC-based verification of CBTC Bitcoin reserves
   - Verify FROST attestation proofs without joining the signer set
   - Expose via UCV-1 universal verifier: `POST /api/v1/chains/canton/verify`
   - **Rationale**: CBTC is live today. Verifying its Bitcoin backing is a sovereignty service.

2. **Machine Identity Extension (G-C2)**
   - Extend `SovereignIdentity` to support `MachineIdentity` with peaq DID + device key
   - Add `/api/v1/identity/resolve/machine` endpoint
   - Integrate with existing BNS/ENS/World ID stack
   - **Rationale**: Machine identity is the prerequisite for all M2M routing.

3. **Lightning M2M Settlement Primitives (G-C3)**
   - Extend Lightning adapter to support M2M settlement intents
   - Add `SettlementSource::MachineToMachine` variant
   - Integrate d402/x402 for API-level machine payments
   - **Rationale**: Lightning is the M2M settlement rail; Conxian routes it.

### 6.2 Medium-Term (Q4 2026) — Research → Build

4. **Canton State Translation Adapter (G-C4)**
   - Implement `CantonStateTranslator` trait (observe-only mode)
   - Map Daml ACS → Universal Contract Reference → Bitcoin anchor
   - Research: Can we run a read-only Canton participant node without joining the network?
   - **Rationale**: Institutional demand for Canton→Bitcoin sovereign routing.

5. **Chainlink CCIP Canton Connector (G-C5)**
   - Leverage Chainlink CCIP's existing Canton integration (Sep 2025)
   - Route CCIP messages through Conxian's compliance ZKC pipeline
   - **Rationale**: CCIP solves the cross-chain messaging; Conxian adds sovereignty and compliance.

6. **Machine RWA Verification Pipeline (G-C6)**
   - Verify machine revenue attestations (peaq, DIMO, ELOOP)
   - Route verified revenue to token holders via Lightning
   - **Rationale**: Tokenized machine RWAs need trustless revenue verification.

### 6.3 Long-Term (Q1-Q2 2027) — Research

7. **Canton↔Bitcoin Atomic Swap Engine (G-C7)**
   - Full trustless atomic swap between Canton Daml contracts and Bitcoin UTXOs
   - Requires: Daml→Bitcoin HTLC/PTLC script compilation
   - **Rationale**: The holy grail — trustless institutional↔sovereign settlement.

8. **DePIN Compliance ZKC (G-C8)**
   - Jurisdictional tax reporting for autonomous machine income
   - Machine revenue classification across G7/BRICS/neutral jurisdictions
   - **Rationale**: As DePIN scales, tax authorities will demand machine revenue reporting.

### 6.4 What We Explicitly Do NOT Do

- ❌ Run a Canton validator/participant node (custodial risk)
- ❌ Hold Canton Coin or Daml assets (breaks non-custodial principle)
- ❌ Build a wrapped Bitcoin on Canton (CBTC already exists; verify, don't compete)

---

## Part 7: peaq Network Deep Dive (2026-07-06 Expansion)

### 7.1 peaq Overview: The Machine Economy Blockchain

**peaq** is a purpose-built blockchain for the Machine Economy — enabling robots, devices, sensors, and AI agents to operate as autonomous economic actors.

| Metric | Value (2026) |
|:---|:---|
| **Total Transactions** | 174M+ |
| **Machine Addresses** | 3.35M+ |
| **Human Wallets** | 2.67M+ |
| **Daily Machine Transactions** | 70,000–120,000 |
| **DePIN Apps** | 60+ |
| **Industries Reshaped** | 22 |
| **TVL** | $180M+ |
| **MachineX DEX Volume** | $60M+ |
| **Consensus** | NPoS (Substrate/Polkadot SDK) |
| **Cross-Chain** | LayerZero V2 (omnichain) |

**Enterprise Partners**: Deutsche Telekom, Lufthansa, NTT, Continental, Bosch, Denso, Airbus, Mastercard, Gaia-X, Fetch.ai

### 7.2 peaq Architecture: Four-Layer Stack

```
┌─────────────────────────────────────────────────────────────┐
│                      peaq NETWORK                           │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                   peaqOS Scale                       │   │
│  │  • Open Wallet Standard (OWS) v1.3                  │   │
│  │  • AI Agent pairing (Scale function)                  │   │
│  │  • Machine Markets API                                │   │
│  │  • Payment Rails: x402, MPP, USDT, onchain-escrow   │   │
│  └────────────────────────┬────────────────────────────┘   │
│                           │                                │
│  ┌────────────────────────▼────────────────────────────┐   │
│  │                   Trust Layer                        │   │
│  │  • Machine Trust Attestations                        │   │
│  │  • Machine Credit Rating (MCR) — AAA to NR           │   │
│  │  • Trust Validators (stake-based)                    │   │
│  │  • DID Registry (W3C standard)                        │   │
│  └────────────────────────┬────────────────────────────┘   │
│                           │                                │
│  ┌────────────────────────▼────────────────────────────┐   │
│  │                  peaq Chain (EVM)                     │   │
│  │  • Substrate/Polkadot SDK fork                       │   │
│  │  • GRANDPA finality gadget                          │   │
│  │  • Chain ID 3338                                     │   │
│  │  • Solidity precompiles for DID, IdentityRegistry   │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 7.3 peaqID: W3C Decentralized Identity for Machines

```
did:peaq:<0x-address>
```

**Registration Flow**:
1. `registerMachine()` or `registerFor()` (proxy-managed)
2. Identity NFT minted by `IdentityRegistry` (tokenId = machineId, soulbound)
3. DID resolves to flat key-value attribute store on peaq DID Registry precompile

**DID Attributes**: `machineId`, `nftTokenId`, `operator`, `documentation_url`, `data_api`, `data_visibility`

**Ownership Proof**: EIP-191 challenge → signature → verification → pairing persisted

**Cross-Chain**: DIDLite contracts on satellite chains maintain portability

### 7.4 Open Wallet Standard (OWS): Multi-Chain Machine Wallets

OWS derives accounts across ALL chains from a single BIP-39 mnemonic:

| Chain Family | Examples |
|:---|:---|
| **EVM** | peaq, Ethereum, Base, Polygon, Arbitrum, Optimism |
| **Non-EVM** | **Bitcoin**, Solana, Cosmos, Tron, TON, Sui, XRPL, Spark, Filecoin |

**Key**: Machines already have Bitcoin addresses derivable from their mnemonic (`m/84'/0'/0'/0/{index}`) — but **NO existing Bitcoin/Lightning infrastructure on peaq today**.

### 7.5 peaq DePIN Ecosystem: 60+ Projects

**Infrastructure/DePIN (40+ projects)**: Silencio (audio data), MapMetrics (navigation), NATIX (mapping), Roam (telecom), DATS (compute/security), DeNet (storage), Chirp (IoT), Arkreen (energy), Combinder (VPP), CPIN (solar), NYX Carbon (sustainability DeFi), penomo (energy RWA), AquaSave (water), Farmsent (agriculture), dTelecom, Anyone, Aizel Network (AI), Acurast (compute), aZen (edge compute), iGam3 (AI agents), BigWater, BitDoctor, HOFA, JuiceUp, Powerpod, charge, AXI, PING, Quakecore, Reflex DAO, Menthol Protocol, Pickspot, NetSepio

**Robotics/DePAI (10+)**: XMAQUINA (Robotics DAO, $35M+ treasury), Auki (robot positional awareness), Over the Reality, Alpha AI (drone surveys), Dronedash, Homebrew Robotics Club, Robostack, RiceAI, CodecFlow

**AI (5+)**: 375ai (edge AI), Newcoin, Kaisar

**Tokenization/RWA (3+)**: DualMint (tokenized machines, 20% avg yield), Octo Prestige

**Finance**: MachineX (world's first Machine Economy DEX, $60M+ volume)

### 7.6 The Critical Gap: NO BITCOIN/LIGHTNING ON peaq

**Key Finding**: peaq has NO existing Bitcoin or Lightning Network infrastructure despite having:
- 3.35M+ machines with Bitcoin-capable addresses (via OWS)
- Growing machine revenue streams
- Enterprise demand for payment rails
- USDT/USDC integration via Tether WDK

This is Conxian's **first-mover opportunity** — be the first Bitcoin/Lightning gateway for peaq machines.

### 7.7 Proposed peaq Adapter Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Conxian Gateway                           │
│  ┌───────────────────────────────────────────────────────┐   │
│  │                  peaq Adapter Module                    │   │
│  │                                                        │   │
│  │  ┌─────────────┐  ┌──────────────┐  ┌────────────┐  │   │
│  │  │ peaq RPC    │  │ OWS Wallet   │  │ peaqID     │  │   │
│  │  │ Connector   │  │ Bridge       │  │ Resolver   │  │   │
│  │  │ (Chain ID   │  │ (BIP-39 →    │  │ (W3C DID)  │  │   │
│  │  │  3338)      │  │  BTC deriv)  │  │            │  │   │
│  │  └──────┬──────┘  └──────┬───────┘  └─────┬────┘  │   │
│  │         │                │                 │        │   │
│  │  ┌──────▼────────────────▼─────────────────▼─────┐  │   │
│  │  │          Machine Identity & Settlement          │  │   │
│  │  │   peaqID ←→ Lightning Invoice Bridge           │  │   │
│  │  │   PEAQ/USDT Revenue → sats → BTC savings      │  │   │
│  │  └─────────────────────┬────────────────────────┘  │   │
│  └────────────────────────│────────────────────────────┘   │
│                           │                                │
│  ┌────────────────────────▼────────────────────────────┐   │
│  │              Bitcoin/Lightning Layer                  │   │
│  │  • LND/CLN Node                                      │   │
│  │  • Nostr relays (NWC/NIP-47)                        │   │
│  │  • On-chain BTC address                              │   │
│  └────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

**Development Phases**:
| Phase | Feature | Priority |
|:---|:---|:---|
| **P1** | peaq RPC monitoring, OWS wallet import | First-mover |
| **P2** | Lightning invoice generation for machines | Revenue routing |
| **P3** | PEAQ/USDT → sats exchange integration | Liquidity |
| **P4** | Machine RWA verification pipeline | Enterprise |

### 7.8 Machine Revenue → Lightning → Bitcoin Flow

```
Machine Service Delivery (EV charging, compute, data)
        ↓
peaq Escrow (ERC-8004) ← LayerZero cross-chain
        ↓
peaq Machine Wallet (OWS) — PEAQ/USDT balance
        ↓
Conxian Gateway (peaq Adapter)
        ├── DID verification (peaqID → trust score)
        ├── Revenue attestation (machine revenue → ZKC)
        └── Exchange to sats (DEX/CEX integration)
                ↓
        Lightning Network (HODL invoices, P2P routing)
                ↓
        Machine Bitcoin Savings / Investor Distributions
        ├── Hodl position ( sovereign store of value)
        ├── DCA into ETFs (institutional)
        └── Stacks sBTC (Bitcoin L2 yield)
```

---

## Part 8: State Translation Patterns & UCV-2

### 8.1 State Translation Taxonomy

Conxian must translate between heterogeneous ledgers. Here's the comprehensive mapping:

| Ledger | State Model | Contract Primitive | Identity Model | Conxian Translation |
|:---|:---|:---|:---|:---|
| **Bitcoin** | UTXO | OutPoint | x-only pubkey | Anchor / root of trust |
| **Stacks** | UTXO-like | Clarity contract | BNS name / STX address | Bitcoin-aligned L2 |
| **Liquid** | UTXO+CTx | Confidential asset | Asset blindning key | BTC sidechain |
| **RGB** | UTXO+seals |Contract Instance | Blinded UTXO | Bitcoin-native assets |
| **Canton** | eUTXO | Daml Contract | W3C Party | Institutional eUTXO |
| **peaq** | EVM Account | Solidity contract | peaqID (W3C DID) | Machine economic actor |
| **Rootstock** | EVM Account | Solidity | ETH address | BTC peg L2 |
| **Babylon** | BTC anchor | Checkpoint proof | BTC pubkey | BTC security |
| **Fedimint** | e-cash note | Federated mint | Mint member set | BTC e-cash |
| **Lightning** | HTLC state | Payment channel | Node pubkey | BTC microtx |
| **DIMO** | Vehicle DID | Vehicle contract | VIN-based DID | Machine RWA |

### 8.2 Universal Contract Reference (UCR) Pattern

```
UniversalContractRef = {
    ledger: LedgerType,           // BTC, STX, peaq, Canton, etc.
    contract_id: Bytes,            // Ledger-native identifier
    state_hash: Digest,            // Merkle commitment to state
    version: u64,                  // Monotonic version
    metadata: {
        owner: SovereignIdentity,  // Human/Machine/Org
        controller: SovereignIdentity,
        attestations: Vec<AttestationProof>,
        jurisdiction: JurisdictionTag,
    }
}
```

### 8.3 Cross-Ledger Settlement Protocol (UCV-2 Concept)

```
┌──────────────────────────────────────────────────────────────┐
│           Cross-Ledger Settlement Protocol (UCV-2)             │
│                                                              │
│  Step 1: Intent Declaration                                   │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  SettlementIntent {                                    │   │
│  │    source: LedgerType,    // peaq, Canton, Liquid...   │   │
│  │    target: LedgerType,    // Bitcoin, Stacks, LN...   │   │
│  │    amount: Amount,                                     │   │
│  │    conditions: Vec<SettlementCondition>,              │   │
│  │    sovereign_memo: SovereignMemo,                      │   │
│  │  }                                                    │   │
│  └────────────────────────────────────────────────────────┘   │
│                           ↓                                   │
│  Step 2: State Translation                                    │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  Conxian translates:                                   │   │
│  │  • peaq DID → SovereignIdentity                       │   │
│  │  • Canton Daml Contract → UCR                         │   │
│  │  • peaq PEAQ balance → sats estimate                  │   │
│  └────────────────────────────────────────────────────────┘   │
│                           ↓                                   │
│  Step 3: Verification (UCV-1)                                 │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  UniversalVerifier {                                   │   │
│  │    Ecdsa: Bitcoin SPV proofs                           │   │
│  │    Schnorr: Stacks Nakamoto, BitVM2                    │   │
│  │    Zkml: Chainproofs, Citadel                          │   │
│  │    Cbtc: CBTC FROST attestation                       │   │
│  │    peaq: DID + MCR verification ← NEW                 │   │
│  │    Canton: Daml ACS translation ← NEW                 │   │
│  │  }                                                    │   │
│  └────────────────────────────────────────────────────────┘   │
│                           ↓                                   │
│  Step 4: Atomic Settlement Execution                          │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  HTLC/PTLC: Bitcoin-anchored hash/adaptor locks        │   │
│  │  DLC: Oracle-attested conditional settlement            │   │
│  │  FROST: Threshold signature for multi-party             │   │
│  │  Lightning: HTLC routing to final recipient            │   │
│  └────────────────────────────────────────────────────────┘   │
│                           ↓                                   │
│  Step 5: Sovereign Memo Embedding                            │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  OP_RETURN / Taproot annex:                           │   │
│  │  • Source ledger + contract reference                  │   │
│  │  • Authorization proof (signature/attestation)        │   │
│  │  • Compliance ZKC (jurisdiction tag)                  │   │
│  │  • Discard immediately from Gateway memory             │   │
│  └────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

### 8.4 peaq Adapter: Detailed Design

```rust
// internal/engine/src/institutional/peaq_adapter.rs

/// peaq RPC connector configuration
pub struct PeaqConfig {
    pub rpc_url: Url,              // https://rpc.peaq.network or self-hosted
    pub chain_id: u64,             // 3338 for mainnet
    pub escrow_contract: Address,   // ERC-8004 escrow contract
    pub identity_registry: Address, // DID registry precompile
    pub layer_zero_endpoint: Address,
}

/// peaq machine identity resolved from chain
pub struct PeaqMachineIdentity {
    pub did: Did,                      // did:peaq:<address>
    pub machine_id: U256,              // On-chain machine ID
    pub nft_token_id: U256,            // Identity NFT tokenId
    pub operator: Option<Did>,          // Proxy operator if registered
    pub data_api: Option<Url>,          // Machine data API endpoint
    pub machine_credit_rating: MCR,     // AAA to NR
    pub is_trust_activated: bool,
}

/// peaq event subscription types
pub enum PeaqEvent {
    /// Machine registered on peaq
    MachineRegistered { machine_id: U256, owner: Did, timestamp: u64 },
    /// Payment received in escrow
    EscrowDeposited { escrow_id: U256, amount: U256, token: TokenType },
    /// Payment released from escrow
    EscrowReleased { escrow_id: U256, recipient: Did, amount: U256 },
    /// Machine trust attestation updated
    TrustUpdated { machine_id: U256, new_rating: MCR },
}

/// OWS wallet bridge: derive Bitcoin addresses from peaq mnemonic
pub struct OwsWalletBridge {
    mnemonic: Mnemonic,    // Never stored — passed at runtime
    hd_path: HdPath,        // m/84'/0'/0'/0/{index} for BTC
}

impl OwsWalletBridge {
    pub fn derive_btc_address(&self, index: u32) -> Result<BitcoinAddress> {
        // BIP-39 → BIP-84 derivation for native SegWit BTC address
        let private_key = self.mnemonic.derive(BIP84_PATH, index);
        let public_key = private_key.to_x_only_public_key();
        Ok(BitcoinAddress::p2tr(public_key, None, Network::Bitcoin))
    }
}
```

### 8.5 Non-Custodial Revenue Verification (peaq → Lightning)

```
┌──────────────────────────────────────────────────────────────┐
│         Machine Revenue → Lightning → Bitcoin Flow            │
│                                                              │
│  1. Machine provides service (EV charging, compute, data)     │
│                    ↓                                         │
│  2. Payment escrowed in peaq ERC-8004 contract               │
│     Events: EscrowDeposited(machine_id, amount, PEAQ/USDT)   │
│                    ↓                                         │
│  3. Conxian peaq adapter subscribes to events                │
│     → Verifies machine DID via peaqID resolver                │
│     → Checks MCR credit rating                                │
│     → Confirms service_type matches payment                   │
│                    ↓                                         │
│  4. Revenue attestation (ZKC pass-through)                    │
│     → Machine revenue: <amount> <currency>                   │
│     → Jurisdiction: <G7/BRICS/neutral>                       │
│     → No PII persisted — ephemeral attestation               │
│                    ↓                                         │
│  5. Settlement execution                                      │
│     → Route PEAQ to exchange (DEX or CEX)                   │
│     → Convert to sats                                         │
│     → Generate Lightning invoice (or HODL invoice)            │
│     → Deliver to machine's BTC address (on-chain sweep)       │
│                    ↓                                         │
│  6. Sovereign Memo (OP_RETURN)                                │
│     "peaq:machine:<id>|revenue:<amount>|jurisdiction:<tag>" │
│     → Embedded in BTC transaction                             │
│     → Discarded from Gateway memory immediately              │
└──────────────────────────────────────────────────────────────┘
```

---

## Part 9: Strategic Knowledge Graph (Updated 2026-07-06)

### 9.1 Protocol Universe Map

```
                           ┌─────────────────────────────────────────────┐
                           │           Conxian Gateway (Sovereign Router)│
                           │                                              │
                           │  ┌──────────┐  ┌──────────┐  ┌─────────────┐  │
                           │  │ REST API │  │   Auth   │  │  Metrics    │  │
                           │  │ (16 EP)  │  │ (Bearer) │  │ (Prometheus)│  │
                           │  └────┬─────┘  └──────────┘  └─────────────┘  │
                           │       │                                       │
                           │  ┌────▼─────────────────────────────────┐    │
                           │  │     UCV-2 Cross-Ledger Settlement    │    │
                           │  │  (extends UCV-1: +peaq +Canton)     │    │
                           │  └────┬─────────────────────────────────┘    │
                           │       │                                       │
                           │  ┌────▼──────────────┬───────────────────┐    │
                           │  │    Compliance     │      Engine       │    │
                           │  │   (ZKC Pipeline)  │   (14 Adapters)  │    │
                           │  └──────────────────┴───────────────────┘    │
                           └───────────────────────────────────────────────┘
                                         │                      │
                           ┌──────────────┴──────┐    ┌──────────┴──────────────┐
                           │   Sanctions         │    │   Protocol Adapters     │
                           │   Screening          │    │   ┌──────────────────┐  │
                           │   (OFAC/EU/UN)      │    │   │  Bitcoin Core    │  │
                           └─────────────────────┘    │   │  ├──────────────┤  │
                                                       │   │  Lightning      │  │
                           ┌────────────────────────┐  │   │  ├──────────────┤  │
                           │   Sovereignty Bridge    │  │   │  Liquid        │  │
                           │   (Stacks/RGB/BTC)     │  │   │  ├──────────────┤  │
                           └────────────────────────┘  │   │  Stacks        │  │
                                                       │   │  ├──────────────┤  │
                           ┌────────────────────────┐  │   │  Rootstock     │  │
                           │  Institutional Perimeter│  │   │  ├──────────────┤  │
                           │  (Canton/LayerZero)    │  │   │  RGB v0.12     │  │
                           └────────────────────────┘  │   │  ├──────────────┤  │
                                                       │   │  Babylon       │  │
                           ┌────────────────────────┐  │   │  ├──────────────┤  │
                           │   Machine Economy       │  │   │  BitVM2       │  │
                           │   (peaq/DePIN/M2M)     │  │   │  ├──────────────┤  │
                           └────────────────────────┘  │   │  Fedimint      │  │
                                                       │   │  ├──────────────┤  │
                           ┌────────────────────────┐  │   │  Citrea        │  │
                           │   BRICS+ Multi-Currency│  │   │  ├──────────────┤  │
                           │   (CIPS/PAPSS/mBridge) │  │   │  Strata        │  │
                           └────────────────────────┘  │   │  ├──────────────┤  │
                                                       │   │  peaq ◄ NEW   │  │
                                                       │   │  ├──────────────┤  │
                                                       │   │  Canton ◄ NEW │  │
                                                       │   │  ├──────────────┤  │
                                                       │   │  RISC Zero ◐  │  │
                                                       │   └──────────────────┘  │
                                                       └────────────────────────┘
```

### 9.2 Gaps & Opportunities Matrix

| Gap/Opportunity | Type | Priority | Sovereignty Fit | Technical Complexity | Action |
|:---|:---|:---|:---|:---|:---|
| **peaq BTC/LN Gateway** | Integration | P1 | ✅ Non-custodial routing | Medium | Build peaq adapter module |
| **Machine DID extension** | Feature | P1 | ✅ Extend identity stack | Low | Add peaqID → SovereignIdentity |
| **Lightning M2M primitives** | Feature | P1 | ✅ Machines hold keys | Low | SettleSource::MachineToMachine |
| **Canton Daml state translation** | Integration | P2 | ✅ Observe-only | High | Daml ACS → UCR |
| **CBTC DLC verification** | Feature | P1 | ✅ FROST attestation | Medium | DLC + adaptor signatures |
| **Machine RWA revenue ZKC** | Feature | P2 | ✅ Compliance pipe | Medium | Revenue attestation |
| **Canton↔BTC atomic swap** | Integration | P3 | ✅ Trustless | Very High | HTLC/PTLC on Bitcoin for Daml |
| **DePIN compliance ZKC** | Feature | P3 | ✅ Jurisdictional | Medium | Machine revenue classification |
| **Chainlink CCIP Canton** | Integration | P2 | ✅ ZKC compliance | Medium | CCIP → Gateway pipeline |
| **UCV-2: Cross-ledger protocol** | Architecture | P2 | ✅ Universal routing | High | Extend UCV-1 → UCV-2 |

### 9.3 Revenue Model: Sovereignty-Aligned Monetization

| Revenue Stream | Mechanism | Custody Model | Conxian Value |
|:---|:---|:---|:---|
| **Basis-point routing fee** | 1-5 bps on settled volume | ✅ Never holds assets | Routing + compliance |
| **Attestation fee** | Flat fee per proof | ✅ Stateless verification | UCV verification |
| **Sovereign Memo stamp** | Per embedded compliance memo | ✅ One-time, ephemeral | Compliance pipe |
| **Machine ID subscription** | Monthly for DID resolution | ✅ Identity-as-service | peaqID resolver |
| **Lightning channel lease** | Inbound liquidity provision | ✅ Non-custodial LP | M2M payment rails |
| **peaq→BTC conversion spread** | FX margin on PEAQ→sats | ✅ Convert & forward | Exchange integration |
| **Verification-as-a-Service** | UCV proof verification | ✅ Compute-only | Universal verifier |
| **Liquidity pulse subscription** | Mempool orchestration access | ✅ SaaS, no custody | Treasury monitor |

---

## References

### Canton Network
- Main: https://canton.network/
- Foundation: https://canton.foundation/
- Ecosystem: https://sync.global/canton-apps/
- Whitepaper: https://canton.network/whitepaper
- Wikipedia: https://en.wikipedia.org/wiki/Canton_Network
- peaq: https://peaq.xyz/
- peaq Docs: https://docs.peaq.xyz/
- Purple Paper: https://www.peaq.xyz/purple-paper
- peaq Ecosystem: https://www.peaq.xyz/learn/ecosystem
- peaq Blog: https://www.peaq.xyz/blog
- MachineX: https://www.machinex.xyz
- peaq GitHub: https://github.com/peaqnetwork
- Digital Asset: https://digitalasset.com/
- LayerZero: https://layerzero.network/

---

*Research expanded: 2026-07-06 | Canton Network + Machine Economy + peaq deep-dive*

### What We Explicitly Do NOT Do (peaq)

- ❌ Issue machine tokens or DePIN tokens (competing with peaq/Helium ecosystem)
- ❌ Store PII or machine telemetry (compliance pipe only)
- ❌ Run a peaq validator or full node (custodial risk)
- ❌ Hold peaq tokens or assets (breaks non-custodial principle)

---

## Part 7: Research Gaps & Open Questions

1. **Canton read-only observer mode**: Can an external node observe Canton synchronizer messages without being a network participant? (Check Canton docs for public observer API)
2. **Daml-to-Bitcoin-Script compilation**: Is there a path to compile Daml contract conditions into Bitcoin Script for trustless bridging?
3. **CBTC FROST signer set**: Who are the signers? What is the threshold? Is the attestation proof publicly verifiable?
4. **peaq↔Bitcoin bridge status**: peaq is Polkadot-based; what is the current state of Polkadot↔Bitcoin bridges?
5. **Machine identity standards war**: peaq DID vs DIMO Vehicle ID vs W3C DID — which standard wins?
6. **DePIN regulatory landscape**: How are jurisdictions classifying machine income? (capital gains? business income? utility token?)
7. **Canton 4.x contract keys**: When will Canton restore contract key support (removed in 3.x)? This affects state translation fidelity.
8. **Polyglot Canton EVM**: When EVM compatibility lands on Canton (announced Feb 2025), does this simplify or complicate Conxian's integration path?

---

## Part 8: Integration with Existing Research

| Existing Research Doc | Canton/Machine Economy Connection |
|:---|:---|
| **OPPORTUNITY_MAP_AND_EXPANSION.md** | Add Canton as "Section 1.E: Institutional Privacy DLT Interop"; add Machine Economy as "Section 1.F" |
| **SOVEREIGN_SETTLEMENT_ORCHESTRATION.md** | Extend Maneuver Engine to include Canton domain finality awareness; add M2M settlement intent type |
| **CANDIDATE_MATRIX.md** | Add Canton state translator (Score: 6.5), Machine identity (Score: 7.8), M2M settlement (Score: 7.5) |
| **UNIVERSAL_CHAIN_RESEARCH.md** | Add "Section 7: Institutional Privacy DLT Family (Canton/Daml)" with adapter pattern |
| **ADAPTER_FAMILY_STRATEGY.md** | Add "Section 5: Institutional UTXO Family (Canton)" as UTXO-family sibling to Bitcoin/Liquid |
| **BRICS_FINANCIAL_SYSTEMS_RESEARCH.md** | Canton is Western-aligned institutional; BRICS is Eastern-aligned — Gateway serves both via dual-stack |

---

## Part 9: Implementation Status (2026-07-06)

### P1 (Q3 2026) — Implemented ✅
| Gap | Status | Artifacts |
|:---|:---|:---|
| G-C1 | ✅ Done | `CbtcAttestation`, `CbtcVerificationRequest/Response` + `POST /api/v1/canton/cbtc/verify` (6-point check) |
| G-C2 | ✅ Done | `MachineType` (11 variants), `MachineIdentity` + `POST /api/v1/identity/resolve/machine` (3 providers) |
| G-C3 | ✅ Done | `SettlementSource::MachineToMachine`, `M2MSettlementRail`, `M2MSettlementRequest/Response` + `POST /api/v1/m2m/settle` (Lightning live) |

### P2 (Q4 2026) — Implemented ✅
| Gap | Status | Artifacts |
|:---|:---|:---|
| G-C4 | ✅ Done | `UniversalContractRef`, `CantonDomainRef`, `CantonStateTranslationRequest/Response` + `POST /api/v1/canton/state/translate` (Daml template-aware) |
| G-C5 | ✅ Done | `CcipMessageRoute`, `CcipRouteRequest/Response` + `POST /api/v1/ccip/route` (sanctions escalation logic) |
| G-C6 | ✅ Done | `MachineRwaRevenue`, `RevenueSource`, `MachineRwaVerificationRequest/Response` + `POST /api/v1/rwa/machine/verify-revenue` (5-point check) |

### P3 (Q1 2027) — Research Only 🟡
| Gap | Status | Notes |
|:---|:---|:---|
| G-C7 | Research | Requires Daml↔Bitcoin HTLC/PTLC script compilation |
| G-C8 | Research | Requires jurisdictional tax classification for autonomous machine income |

### API Surface Summary (6 new endpoints)

```
POST /api/v1/identity/resolve/machine     → G-C2: Machine identity resolution
POST /api/v1/m2m/settle                   → G-C3: M2M settlement routing
POST /api/v1/canton/cbtc/verify           → G-C1: CBTC non-custodial verification
POST /api/v1/canton/state/translate       → G-C4: Canton state translation
POST /api/v1/ccip/route                   → G-C5: CCIP compliance routing
POST /api/v1/rwa/machine/verify-revenue   → G-C6: Machine RWA revenue verification
```

### Verification Status (2026-07-06)
- ✅ `cargo fmt --all` — passes
- ✅ `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- ✅ `cargo test --workspace` — 129 tests, 0 failures
- ✅ `cargo test --workspace --features mock-integrations` — 132 tests, 0 failures
- ⚠️ `pnpm install && pnpm build && pnpm test` — npm registry unreachable in this environment
- ✅ `python3 scripts/verify_contamination_guard.py` — clean (59 files)

## Part 10: Workflow Enhancement

### 9.1 Automated Research Refresh

Create a GitHub Actions workflow to periodically refresh this research:

```yaml
# .github/workflows/research-refresh.yml
name: Research Refresh
on:
  schedule:
    - cron: '0 0 1 * *'  # Monthly
  workflow_dispatch:
jobs:
  refresh:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - name: Check for Canton/Daml ecosystem updates
        run: |
          # Check Canton GitHub releases
          gh release list --repo digital-asset/canton --limit 5
          # Check peaq ecosystem updates
          gh api repos/peaqnetwork/peaq-network/releases --jq '.[0:3].[].tag_name'
```

### 9.2 Research Tracking Issue

File a tracking issue in the repository to monitor these research items:
- **Title**: "Research: Canton Network & Machine Economy Strategic Expansion"
- **Labels**: `research`, `strategic`, `institutional`
- **Checklist**: 8 research gaps from Part 7

---

## References

1. Canton Network Whitepaper: https://digitalasset.com/hubfs/Canton/Canton%20Network%20-%20White%20Paper.pdf
2. Polyglot Canton (EVM): https://www.canton.network/hubfs/Canton%20Network%20Files/whitepapers/Polyglot_Canton_Whitepaper_11_02_25.pdf
3. Canton Ledger Model: https://docs.canton.network/overview/learn/ledger-model
4. CBTC (BitSafe): https://docs.bitsafe.finance/product-suite/cbtc
5. Canton Interoperability: https://forum.canton.network/t/canton-interoperability/6680
6. Chainlink CCIP Canton: https://docs.chain.link/data-streams/canton-integration
7. peaq Network: https://www.peaq.xyz
8. Lightning Network $1.1B/month: River Bitcoin Adoption Report 2026
9. USDT on Lightning (Taproot Assets): Tether/Voltage, March 2026
10. M2M Payments with Lightning: https://medium.com/@ABussutil/m2m-payments-with-lightning-network-a472562b181
11. d402/x402 (HTTP 402): DecentraLab/Coinbase protocols
12. KPMG Machine Economy Report: https://assets.kpmg.com/content/dam/kpmgsites/ie/pdf/insights/consulting/ie-the-next-era-of-payments.pdf
13. Canton Coin: https://digitalasset.com/hubfs/Canton%20Network%20Files/Documents%20(whitepapers%2C%20etc...)/Canton%20Coin_%20A%20Canton-Network-native%20payment%20application.pdf
14. Deep Dive on Canton: https://collective.flashbots.net/t/deep-dive-on-permissioned-blockchains-the-canton-network/5517

## 10. Canton Daml ACS Anchor to Bitcoin UCR Mapping Specification (2026-09 Expansion)

The Canton Daml Active Contract Set (ACS) commitment model requires translating private institutional smart contract states into public, verifiable Bitcoin references:

1. **State Key Extraction**: Daml templates (`AssetTransfer`, `CollateralBond`, `InvoicePayable`) expose key fields (contract ID, template ID, owner/signatories, payload parameters).
2. **SHA-256 State Commitment**: The normalized Daml payload JSON is hashed: `H(contract_id || template_id || payload_bytes)`.
3. **UCR Derivation**: Construct `ucr:canton:<domain>:<contract_id>` referencing the calculated state root hash.
4. **Bitcoin L1 Verification**: The UCR state commitment is anchor-checked against Bitcoin L1 `OP_RETURN` transactions or Discreet Log Contract (DLC) oracle attestations.
