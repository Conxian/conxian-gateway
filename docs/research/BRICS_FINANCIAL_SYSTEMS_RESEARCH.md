# BRICS+ Financial Systems Research (2026-06-28)

Comprehensive analysis of the global financial system bifurcation: Western-led ISO 20022/SWIFT standards versus BRICS-plus alternative payment infrastructures. This research informs the Conxian Gateway's multi-currency settlement architecture (BRICS, PAPSS, ERP lanes) and sanctions-resilience design.

---

## 1. Macro Context: The Great Bifurcation

The global financial system is splitting into two distinct halves:

| Bloc | GDP Share | Backbone | Core Currency | Standards |
|------|-----------|----------|---------------|-----------|
| Western G7 | ~45% | SWIFT (messaging) + CHIPS (settlement) | USD (~48% SWIFT), EUR (~24%) | ISO 20022 XML, ISO 9362 (BIC), FATF Rec. 16 |
| BRICS+ | ~40% | CIPS, SPFS, mBridge, BRICS Pay | RMB, RUB, INR, AED | Hybrid — CIPS uses ISO 20022; mBridge uses proprietary ledger |

**Key dynamic**: Western SWIFT dominance is eroding structurally. USD global FX reserves slipped from ~70% (early 2000s) to ~58% (2025). Approximately 20% of global commodity trade has shifted away from USD into RMB, AED, and INR corridors.

**Source**: [SEC.gov](https://www.sec.gov), [Forbes](https://www.forbes.com), [BIS](https://www.bis.org), [SAIIA](https://saiia.org.za)

---

## 2. BRICS Alternative Payment Infrastructures

### 2.1 CIPS — Cross-Border Interbank Payment System (China)

| Metric | Value |
|--------|-------|
| Operator | People's Bank of China (PBOC) |
| HQ | Shanghai |
| Launch | 2015; Phase 2: May 2018 |
| 2024 Transaction Value | RMB 175.49 trillion (~$24.47T USD) |
| 2025 Transaction Value | RMB 180.15 trillion |
| Direct Participants | 176 (as of June 2025) |
| Indirect Participants | 1,514 |
| Global Reach | 4,900+ institutions across 189 countries |

**Architecture**: Unlike SWIFT (pure messaging), CIPS combines messaging AND final clearing in RMB. Uses ISO 20022 standards for payment messaging. Critical dependency: >80% of CIPS transactions still rely on SWIFT's underlying message transport.

**Conxian Relevance**: CIPS is the most operationally significant BRICS infrastructure. The Conxian Gateway's `SettlementSource::Brics` path must eventually support CIPS-direct settlement without SWIFT relay. Current `normalize_brics_ingress()` currently treats all BRICS traffic as mBridge-originated; CIPS-specific message normalization should be added as a distinct code path.

### 2.2 SPFS — System for Transfer of Financial Messages (Russia)

| Metric | Value |
|--------|-------|
| Launch | 2014 (post-Crimea sanctions) |
| Participants | 550 organizations (Q3 2023) |
| Foreign Participants | 150 from 16 countries (Q3 2023) |
| Transaction Cost | 0.80–1.00 ₽ (~$0.012–0.015) |
| Sanctions Status | EU banned SPFS for EU banks (June 2024); US OFAC warned against joining (Nov 2024) |

**Integration Status**: Ongoing talks to link SPFS with CIPS (China) and SFMS (India). Russia and PBOC exploring blockchain-based transfer schemes using digital ruble/digital yuan.

**Conxian Relevance**: SPFS is the primary sanctions-evasion rail for Russia. The Gateway should model SPFS as a distinct `SettlementSource` variant with appropriate risk tagging for compliance screening. Integration with CIPS-SPFS bridge could unlock Russia-China trade settlement.

### 2.3 mBridge — Multi-CBDC Bridge (BIS/BRICS)

| Metric | Value |
|--------|-------|
| Launch | 2021 (BIS Innovation Hub) |
| Core Participants | HKMA, Bank of Thailand, CBUAE, PBOC, Saudi Central Bank |
| Observers | ~30 central banks (ECB, IMF, Fed NY, RBI, SARB, BCB, etc.) |
| Architecture | Proprietary mBridge Ledger; EVM-compatible |
| Status | MVP achieved mid-2024; BIS exited October 2024 |
| BIS Exit Reason | Concerns platform could evade US sanctions |

**Technical**: Real-time, peer-to-peer cross-border payments with FX transactions. Each central bank deploys a validating node. Commercial banks conduct real-value transactions. EVM compatibility enables add-on solutions and interoperability.

**Conxian Relevance**: mBridge is the most technically advanced multi-CBDC platform. The Conxian Gateway's `normalize_brics_ingress()` explicitly handles mBridge payloads. Post-BIS exit, mBridge is being positioned as "BRICS Bridge." Chinese regulators have directed banks to use mBridge; firms in Xinjiang reportedly using it to avoid US sanctions.

### 2.4 BRICS Pay — Decentralized Cross-Border Messaging System

| Metric | Value |
|--------|-------|
| Launch | 2018 (BRICS Business Council); formally backed Oct 2024 |
| Architecture | DCMS — Decentralized Cross-border Messaging System |
| Developer | Saint Petersburg State University, Russia |
| Throughput Claim | 20,000 messages/second |
| License | Planned open-source after pilot |
| Status | Pilot phase |

**Key features**: No central owner — participants manage own nodes. No mandatory transaction fees. Automatic transaction route-building. Messages encrypted and signed with multiple mechanisms. Participants can set currency conversion rates and transaction limits.

**Member positions**: Russia (strongest proponent), China (fully backed Oct 2024), Iran (top priority), Brazil (Lula supportive), South Africa (stated "would not replace SWIFT"), India (cautious, pursuing bilateral settlements separately).

**Conxian Relevance**: BRICS Pay DCMS represents a true alternative to SWIFT messaging, not just settlement. If it achieves production deployment, the Gateway would need a DCMS connector alongside current SWIFT-reliant paths. The decentralized architecture aligns with Conxian's sovereignty principles.

### 2.5 BRICS Clear (Proposed)

**Status**: Conceptual only. The October 2024 Kazan Declaration stated BRICS nations "consented to deliberate and investigate the feasibility of creating an autonomous cross-border settlement and depository system." No concrete timeline, technical architecture, or governance structure exists.

**Conxian Relevance**: Monitor only. BRICS Clear is years from implementation. The Gateway should be architected to accommodate a future BRICS clearing system as a `SettlementSource` variant.

---

## 3. BRICS CBDC Landscape

| Country | CBDC | Status | mBridge Role |
|---------|------|--------|--------------|
| China | e-CNY (Digital Renminbi) | 261M users (end 2021), $13.8B transactions | Core co-developer |
| Russia | Digital Ruble | Law adopted July 2023; pilot April 2023 | Observer |
| India | e₹ (Digital Rupee) | Wholesale pilot Nov 2022; Retail Dec 2022 | Observer |
| Brazil | Drex | In development; delayed from end-2024 | Observer |
| UAE | Digital Dirham | Active development | Core co-developer |
| South Africa | — | Wholesale research only (Project Dunbar) | Observer |
| Iran | — | No confirmed CBDC | — |
| Saudi Arabia | — | Active development | Core participant (joined June 2024) |

**Conxian Relevance**: The Gateway's `UniversalVerifier` and multi-chain adapter framework can be extended to validate CBDC state proofs. e-CNY integration via mBridge is the nearest-term opportunity. Digital ruble integration via SPFS is the sanctions-resilience path.

---

## 4. Local Currency Settlement Corridors

| Corridor | Currencies | Status |
|----------|------------|--------|
| Russia-China | RUB-CNY | Substantial; CIPS-SPFS integration underway |
| India-Russia | INR-RUB | Explored via SPFS-Vnesheconombank-RBI (March 2022) |
| UAE-India | AED-INR | Established pre-BRICS; now within BRICS framework |
| Russia-Iran | RUB-IRR | Active; both under sanctions |
| Brazil-China | BRL-CNY | Growing; bilateral trade agreements |

**Conxian Relevance**: The Gateway's treasury module (`TreasuryMonitor`) should eventually track multi-currency FX rates across these corridors. The `SettlementSource::Brics` path should support currency conversion between corridor pairs.

---

## 5. Sanctions-Resilience Architecture

### Western Sanctions Leverage Points
1. SWIFT exclusion (weaponized against Iran 2012, Russia 2022)
2. Correspondent banking denial (US Treasury OFAC)
3. USD clearing access (CHIPS/Fedwire)
4. FATF greylisting/blacklisting

### BRICS Countermeasures
1. **SPFS**: Russian SWIFT alternative (operational, 550 participants)
2. **CIPS**: Chinese settlement system bypassing USD clearing ($24.47T in 2024)
3. **mBridge**: CBDC bridge settling directly between central banks (no correspondent banks)
4. **BRICS Pay DCMS**: Decentralized messaging (no central choke point)
5. **Local currency swaps**: Bilateral agreements eliminating USD intermediary

### Sanctions Effectiveness Trend
SWIFT exclusion is losing absolute leverage. Russia continues international trade via SPFS/CIPS despite being the most sanctioned nation. OFAC recognized this with its November 2024 alert specifically targeting SPFS. The BIS exited mBridge in October 2024 partly due to sanctions-evasion concerns.

**Conxian Relevance**: The Gateway must be architected for sanctions-resilience by default. This means:
- Supporting multiple settlement rails (ISO 20022, BRICS, PAPSS, ERP)
- Avoiding single-point-of-failure dependencies on SWIFT
- Implementing jurisdictional sharding (SSV-1) to isolate sanctionable from sanctions-resilient flows
- Maintaining audit trails that satisfy both Western and BRICS compliance requirements

---

## 6. Co-Dependence Reality Check

Despite accelerating multipolar momentum, complete decoupling remains difficult:

| Factor | Detail |
|--------|--------|
| SWIFT Dependency | >80% of CIPS transactions still use SWIFT message transport |
| RMB Global Share | ~3% of standard global transactional payments (outside China corridors) |
| Internal BRICS Friction | India-China rivalry creates resistance to centralized frameworks |
| Saudi Positioning | Balancing US alliance with BRICS membership |
| Tech Stack | ISO 20022 dominates — CIPS, Fedwire, T2, NPP all use it |

**Conclusion for Conxian**: The Gateway should support a **hybrid architecture** — ISO 20022 for G7-aligned corridors AND BRICS-specific protocols for alternative rails. This dual-stack approach mirrors the real-world financial system.

---

## 7. Conxian Gateway BRICS Integration Roadmap

### Current State (v0.1.4)
- `SettlementSource::Brics` enum variant (conxian-core/src/settlement.rs)
- `normalize_brics_ingress()` — mBridge payload parsing (compliance/src/zkc.rs)
- `POST /api/v1/settlement/brics` + `POST /api/v1/ingress/brics` routes
- `SovereignCommit` trait interface for BRICS settlements

### Near-Term (v0.2.x)
- [ ] **G-B1**: CIPS-specific message normalization (ISO 20022 CIPS variant)
- [ ] **G-B2**: Multi-currency FX rate tracking in TreasuryMonitor (RMB, RUB, INR, AED)
- [ ] **G-B3**: BRICS Pay DCMS connector research (pre-pilot monitoring)
- [ ] **G-B4**: Sanctions-risk tagging on SettlementSource variants
- [ ] **G-B5**: PAPSS (Pan-African Payment and Settlement System) settlement rail

### Medium-Term (v0.3.x)
- [ ] **G-B6**: mBridge node deployment capability (EVM-compatible validator)
- [ ] **G-B7**: SPFS message format normalization
- [ ] **G-B8**: e-CNY / digital ruble CBDC state proof verification
- [ ] **G-B9**: BRICS Clear readiness (architecture placeholder)

### Long-Term (v1.0+)
- [ ] **G-B10**: Full BRICS Pay DCMS node operation
- [ ] **G-B11**: Multi-hop BRICS corridor settlement (e.g., INR→AED→RUB)
- [ ] **G-B12**: Jurisdictional sharding with automatic rail selection based on sanction status

---

## 8. Research Sources

| Source | URL | Topic |
|--------|-----|-------|
| BIS mBridge | https://www.bis.org/about/bisih/topics/cbdc/mcbdc_bridge.htm | mBridge architecture and status |
| Wikipedia CIPS | https://en.wikipedia.org/wiki/Cross-Border_Interbank_Payment_System | CIPS participants and volumes |
| Wikipedia BRICS PAY | https://en.wikipedia.org/wiki/BRICS_PAY | BRICS Pay DCMS details |
| Wikipedia SPFS | https://en.wikipedia.org/wiki/System_for_Transfer_of_Financial_Messages | SPFS status and sanctions |
| Wikipedia BRICS | https://en.wikipedia.org/wiki/BRICS | Expansion timeline and GDP |
| Wikipedia ISO 20022 | https://en.wikipedia.org/wiki/ISO_20022 | ISO 20022 global standard |
| Wikipedia mBridge | https://en.wikipedia.org/wiki/MBridge | mBridge participants and timeline |
| SEC.gov | https://www.sec.gov | Global financial system analysis |
| Forbes | https://www.forbes.com | BRICS economic analysis |
| SAIIA | https://saiia.org.za | South Africa BRICS positioning |
| Disruption Banking | https://www.disruptionbanking.com | CIPS/SWIFT comparison |
| ResearchGate | https://www.researchgate.net | Global payment ecosystems |
| LinkedIn (multiple) | https://www.linkedin.com | Financial infrastructure analysis |

---

*Last updated: 2026-06-28 — Comprehensive BRICS+ payment systems research incorporating 2024-2026 developments (16th BRICS Summit, BIS mBridge exit, OFAC SPFS alert, Indonesia accession)*
