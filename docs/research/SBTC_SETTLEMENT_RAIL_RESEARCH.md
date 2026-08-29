# sBTC Settlement Rail: Evidence Review & Integration Strategy

**Status:** Live (T1 Production) | **Issue:** N/A (operational rail)
**Last refreshed:** 2026-08-07 | **Session:** 49

---

## Executive Summary

sBTC is a trust-minimized, 1:1 Bitcoin-backed asset on the Stacks layer that
enables Bitcoin DeFi without wrapping or federation. The two-way peg uses a
decentralized signer set (not a single custodian) with economic incentives for
honest behavior. Conxian Gateway integrates sBTC as a **T1 Production
settlement rail** through a 441-line bridge monitor that tracks peg operations
(deposits/withdrawals) via the Emily API.

**Current state:** The `SbtcBridgeMonitor` polls the Emily API for deposit and
withdrawal lifecycle events, maintains operation state, and exposes aggregated
liquidity metrics. sBTC liquidity is a first-class input to the Treasury
Monitor's sovereign yield index (SYI) calculation and ALEX settlement
strategies.

**Decision:** sBTC remains a T1 production rail. The monitor is complete for
observation and metrics. Settlement execution (initiating peg-in/peg-out) is
deferred pending signer API access and institutional custody requirements.

---

## 1. Protocol Evidence

### 1.1 sBTC Specification

sBTC is defined in SIP-021 (Stacks Improvement Proposal) and implemented in
the Stacks blockchain at the Clarity contract level.

- **SIP-021:** <https://github.com/stacksgov/sips/blob/main/sips/sip-021/sip-021-trust-minimized-bitcoin-peg.md>
- **sBTC Developer Guide:** <https://docs.stacks.co/docs/sbtc/overview>
- **Emily API Reference:** <https://docs.stacks.co/docs/sbtc/emily-api>
- **Signer model:** Decentralized threshold signer set (not a single custodian)

### 1.2 Peg Mechanism

```
Bitcoin L1                          Stacks L2
   │                                   │
   │  BTC deposit tx                   │
   ├──────────────────────────────────►│
   │  (OP_RETURN with Stacks addr)     │  Signer validation
   │                                   │  sBTC mint tx
   │                                   │  ← sBTC credited
   │                                   │
   │  BTC withdrawal                   │
   │◄──────────────────────────────────┤
   │  (Signer threshold sig)           │  sBTC burn tx
   │                                   │  ← sBTC debited
```

### 1.3 Mainnet Evidence

sBTC launched on Stacks mainnet in 2024 (post-Nakamoto upgrade). Key metrics:

| Metric | Value | Source |
|--------|-------|--------|
| sBTC supply | Variable (deposit-driven) | Emily API + Stacks explorer |
| Signer set size | 15-30 signers | Stacks signer registry |
| Peg-out timelock | ~24 hours | Bitcoin confirmation + signer threshold |
| Peg-in confirmation | ~3 Bitcoin blocks | Emily API accepted status |
| Fee model | Dynamic (Bitcoin fee market) | Signer-set determined |

### 1.4 Emily API

The Emily API is the canonical sBTC bridge information endpoint. Key endpoints:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/deposits` | GET | List deposit operations |
| `/withdrawals` | GET | List withdrawal operations |
| `/deposits/{txid}` | GET | Single deposit detail |
| `/withdrawals/{txid}` | GET | Single withdrawal detail |
| `/health` | GET | Bridge health check |

The Gateway's `SbtcEmilyClient` maps these to typed Rust structs.

---

## 2. Current Gateway Implementation

### 2.1 Code Surface

| Component | File | Lines | Status |
|-----------|------|-------|--------|
| sBTC bridge monitor | `internal/engine/src/stacks/sbtc.rs` | 441 | Live |
| Treasury integration | `internal/engine/src/treasury/mod.rs` | ~100 | Live |
| ALEX settlement integration | `pkg/conxian-core/src/alex_settlement.rs` | 1,433 | Live |

### 2.2 Bridge Monitor Capabilities

| Capability | Status | Notes |
|-----------|--------|-------|
| Deposit tracking | ✅ Live | Polls `/deposits`, tracks Pending→Accepted→Confirmed |
| Withdrawal tracking | ✅ Live | Polls `/withdrawals`, same lifecycle |
| Aggregated metrics | ✅ Live | `SbtcBridgeMetrics` — circulating supply, pending/confirmed counts |
| Liquidity exposure | ✅ Live | `sbtc_liquidity_btc()` for Treasury Monitor |
| Health monitoring | ✅ Live | `health_check()` on Emily API |
| SYI contribution | ✅ Live | sBTC yield rates feed the Sovereign Yield Index |
| Peg-in initiation | ❌ Not yet | Requires signer API with authentication |
| Peg-out initiation | ❌ Not yet | Requires burn transaction + signer approval |
| Signer set monitoring | ❌ Not yet | Signer rotation detection |
| Proof verification | ❌ Not yet | Merkle proof of peg-in on Bitcoin L1 |

### 2.3 Integration Points

| System | Integration | Purpose |
|--------|------------|---------|
| StacksListener | `sbtc_bridge` field | Real-time sync alongside block monitoring |
| TreasuryMonitor | `sbtc_liquidity_btc()` | SYI index computation |
| ALEX settlement | `alex_settlement.rs` | sBTC liquidity depth for swap pricing |
| Canton/M2M | G-C3 settlement | sBTC as programmable Bitcoin for M2M |
| BRICS integration | Multi-currency FX | sBTC as BTC bridge for BRICS corridors |

---

## 3. Gap Analysis

### 3.1 G-SB1: Peg-in/out Initiation (P2 — Medium Priority)

**Current:** The bridge monitor is read-only. The Gateway cannot initiate sBTC
deposits or withdrawals.

**Gap:** For autonomous settlement (e.g., M2M payments, SYI rebalancing), the
Gateway needs the ability to construct and submit peg transactions.

**Evidence:**
- Peg-in: Requires constructing a Bitcoin transaction with OP_RETURN output
  containing the destination Stacks address
- Peg-out: Requires submitting a Stacks burn transaction that triggers signer
  review and Bitcoin payout
- Both require the Gateway to hold/manage BTC and sBTC keys

**Promotion gates:**
1. Institutional custody solution for Gateway-held BTC/sBTC keys
2. Define `SbtcPegInRequest` and `SbtcPegOutRequest` types
3. Implement peg-in: construct + sign + broadcast Bitcoin deposit transaction
4. Implement peg-out: construct + sign + broadcast Stacks burn transaction
5. Add operation lifecycle with idempotency keys
6. Integration test with Stacks testnet sBTC deployment
7. Security review of key management

**Estimated effort:** 5-7 days.

### 3.2 G-SB2: Signer Set Monitoring (P3 — Low Priority)

**Current:** The bridge monitor does not track signer set composition or
rotation events.

**Gap:** Signer set changes affect the trust model. The Gateway should detect
rotation events and log them for compliance audit.

**Evidence:**
- Signer set is managed by the Stacks blockchain (Clarity contract)
- Rotation events are emitted as Stacks transactions
- Signer set changes can signal security-relevant events

**Promotion gates:**
1. Define `SignerSetObservation` struct
2. Subscribe to Stacks signer contract events
3. Log rotation events to compliance audit trail
4. Expose via `GET /api/v1/sbtc/signer-set`

### 3.3 G-SB3: Bitcoin L1 Proof Verification (P2 — Medium Priority)

**Current:** The bridge monitor trusts the Emily API for operation state. It
does not independently verify Bitcoin L1 transaction inclusion.

**Gap:** The Emily API could be compromised or lag behind Bitcoin confirmation.
Independent verification would strengthen the trust model.

**Evidence:**
- Bitcoin block headers are available via `BitcoinListener`
- Transaction inclusion can be verified via Merkle proof
- This is a defense-in-depth measure, not a current threat

**Promotion gates:**
1. Use existing `BitcoinListener` block data
2. Implement Merkle proof verification for deposit transactions
3. Cross-reference Emily API `Accepted` state with Bitcoin confirmation depth
4. Alert on divergence (Emily says Accepted but Bitcoin says unconfirmed)

---

## 4. Security Assessment

### 4.1 Custody Model

The Gateway **does not custody sBTC or BTC**. The bridge monitor is read-only
and only observes the Emily API. The Treasury Monitor reads liquidity metrics
but does not control funds.

Peg-in/out initiation (G-SB1) would change this — requiring careful key
management and institutional custody infrastructure.

### 4.2 Trust Model

| Trust Assumption | Mitigation | Status |
|-----------------|-----------|--------|
| Emily API availability | Poll-based with error handling; fail-closed on API failure | ✅ |
| Emily API correctness | Independent L1 verification (G-SB3 planned) | ⬜ |
| Signer set honesty | Economic incentives (slashing); Gateway observes only | ✅ |
| Bitcoin L1 finality | `BitcoinListener` provides independent block data | ✅ |

### 4.3 Attack Surface

| Vector | Mitigation | Status |
|--------|-----------|--------|
| Emily API spoofing | HTTPS + cert validation (minreq with TLS) | ✅ |
| Stale Emily data | `last_updated` timestamp tracking; stale detection | ✅ |
| Double-counting operations | `HashMap<String, SbtcOperation>` with operation ID dedup | ✅ |
| Rapid state transitions | Validated lifecycle: Pending→Accepted→Confirmed→Failed | ✅ |

---

## 5. Decision Gates Summary

| Gate | Status | Blocking |
|------|--------|----------|
| Deposit/withdrawal tracking | ✅ Deployed | — |
| Aggregated liquidity metrics | ✅ Deployed | — |
| Treasury/SYI integration | ✅ Deployed | — |
| ALEX liquidity integration | ✅ Deployed | — |
| Peg-in/out initiation | ❌ G-SB1 | Custody solution |
| Signer set monitoring | ❌ G-SB2 | Priority |
| L1 proof verification | ❌ G-SB3 | Priority |
| sBTC→Lightning atomic swap | ❌ Research | G-SB1 + G-LN1 |

---

## 6. Cross-References

- **ADAPTER_FAMILY_STRATEGY.md:** sBTC classified as T1 Production, 441-line adapter
- **CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md:** sBTC as programmable Bitcoin for M2M
- **SOVEREIGN_YIELD_INDEX_RESEARCH.md:** sBTC yield feeds SYI
- **BRICS_FINANCIAL_SYSTEMS_RESEARCH.md:** sBTC as BTC bridge for BRICS corridors
- **API surface:** sBTC metrics exposed via Treasury monitor endpoints

---

## 7. Recommendations

1. **Maintain read-only monitor as primary posture.** The Gateway's
   "route without touch" principle applies — observing sBTC liquidity is
   valuable without adding custody responsibility.

2. **Prioritize G-SB3 (L1 proof verification)** over G-SB1 (peg initiation).
   Independent verification strengthens the trust model without adding
   operational complexity.

3. **Defer G-SB1 until custody infrastructure is in place.** Peg initiation
   requires BTC/sBTC key management that is outside the Gateway's current
   scope.

4. **Add Prometheus metrics** for sBTC bridge health:
   `conxian_gateway_sbtc_circulating_sats` gauge and
   `conxian_gateway_sbtc_operations_total{kind,state}` counter.
