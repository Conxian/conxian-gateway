# Research: Sovereign Yield Index (SYI)

## 1. Concept
The SYI is a protocol-level metric that tracks the relative yield performance of sBTC vs native BTC and other Stacks-based yield instruments. It is the core incentive mechanism for the "sBTC Suction" pattern.

## 2. Calculation
$SYI = \frac{\text{sBTC Realized Yield}}{\text{BTC Opportunity Cost}} \times \text{Sovereignty Multiplier}$

- **Sovereignty Multiplier**: Higher for non-custodial instruments.
- **Opportunity Cost**: Derived from ALEX and other DEX oracles.

## 3. Implementation in Gateway
The `TreasuryMonitor` in `internal/engine/src/treasury/mod.rs` will be enhanced to poll ALEX liquidity pools and calculate the SYI in real-time, exposing it via the `/api/v1/metrics` endpoint.

## 4. Next Steps
- Integrate ALEX oracle feeds for real-time yield tracking.
- Implement TEE-signed attestation for SYI reports to ensure institutional-grade reporting.


## 5. Multi-Cloud Telemetry Ingress & Real-Time 80/10/10 Yield Splitting (CON-1329 / CON-1600)

To validate the **80/10/10 yield splitting calculations** in real time, Phase 10 implements multi-cloud scorecard alignment across all Gateway infrastructure:

1. **Multi-Cloud Data Aggregation**:
   - Telemetry streams are collected from **Supabase BOS** (auth and edge state events), **Neon Postgres** (persistence health and query performance), and **Render** (container availability and API latency).
2. **Autonomous SLA Engine Feed**:
   - SLA health scores feed directly into `TreasuryMonitor` and `SettlementProposal` yield streaming calculators.
   - When SLA metrics meet institutional thresholds (>=99.9% uptime, <200ms latency), yield distribution dynamically allocates:
     - **80%**: Productive Liquidity Yield / Treasury
     - **10%**: Founder Royalty / Protocol Sustainability
     - **10%**: Ecosystem Reserve / Safety Module
