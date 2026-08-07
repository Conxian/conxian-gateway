# Lightning Network Settlement Rail: Evidence Review & Integration Strategy

**Status:** Live (T1 Production) | **Issue:** N/A (operational rail)  
**Last refreshed:** 2026-08-07 | **Session:** 49

---

## Executive Summary

The Lightning Network (LN) is a layer-2 payment protocol built on Bitcoin that
enables instant, low-fee, high-throughput payments through bidirectional payment
channels. Conxian Gateway integrates Lightning as a **T1 Production settlement
rail** through an 806-line adapter with X402 payment gating, Nostr Wallet
Connect (NWC/NIP-47) relay, and machine-to-machine (M2M) settlement support.

**Current state:** The adapter's trait-based design supports three backends
(Simulated, NWC, Production), but the `ProductionLightningBackend` is a stub —
no real LND or CLN integration is wired. The NWC backend provides a functional
path to real Lightning payments through any NIP-47-compatible wallet.

**Decision:** Lightning remains a T1 rail with the NWC backend as the primary
production path. Direct LND/CLN integration is deferred pending operator demand
and institutional custody requirements.

---

## 1. Protocol Evidence

### 1.1 Lightning Network Specification

The Lightning protocol is defined by a family of BOLT (Basis of Lightning
Technology) specifications maintained at <https://github.com/lightning/bolts>.
Key BOLTs relevant to Gateway integration:

| BOLT | Title | Relevance |
|------|-------|-----------|
| BOLT 1 | Base Protocol | Connection establishment, feature negotiation |
| BOLT 2 | Peer Protocol | Channel open/close, commitment transactions |
| BOLT 3 | Transactions | Bitcoin transaction format for channels |
| BOLT 4 | Onion Routing | Sphinx-based source-routed payments |
| BOLT 7 | P2P Node Discovery | Node announcement, channel graph gossip |
| BOLT 11 | Invoice Protocol | Human-readable payment requests |
| BOLT 12 | Offers Protocol | Reusable payment offers (successor to BOLT 11) |

### 1.2 Mainnet Evidence

The Lightning Network has been operational on Bitcoin mainnet since 2018. Key
metrics as of Q3 2026:

| Metric | Value | Source |
|--------|-------|--------|
| Public channels | ~60,000 | 1ML, Amboss |
| Public node count | ~14,000 | 1ML, Amboss |
| Estimated capacity | ~5,000 BTC | 1ML aggregated |
| Average fee rate | ~0.003% (median) | Lightning Network statistics |
| M2M monthly volume | ~$1.1B (est.) | Conxian internal research (see Canton doc) |

Source: <https://1ml.com/statistics>, <https://amboss.space>

### 1.3 Implementations

| Implementation | Language | Maturity | Gateway Compatibility |
|---------------|----------|----------|----------------------|
| LND (Lightning Labs) | Go | Production | REST + gRPC API |
| Core Lightning (CLN) | C | Production | JSON-RPC via `lightningd` |
| Eclair (ACINQ) | Scala | Production | REST API |
| LDK (Spiral) | Rust | Production | Library, embeddable |
| Phoenixd (ACINQ) | Kotlin | Production | HTTP API |

**Gateway assessment:** LND and CLN are the dominant institutional
implementations. Both expose HTTP APIs that the Gateway's `LightningBackend`
trait could wrap. LDK is notable for its Rust-native library form, but requires
channel state management.

### 1.4 NIP-47 (Nostr Wallet Connect)

NIP-47 defines a Nostr-based protocol for remote wallet control. The Gateway's
`NwcLightningBackend` implements the client side:

- **URI format:** `nostr+walletconnect://<pubkey>?relay=<url>&secret=<key>`
- **Request kind:** 23194 (wallet request)
- **Response kind:** 23195 (wallet response)
- **Supported methods:** `pay_invoice`, `make_invoice`, `lookup_invoice`,
  `get_balance`
- **Gateway usage:** `pay_invoice` for X402 settlement, others for operational
  monitoring

Canonical spec: <https://github.com/nostr-protocol/nips/blob/master/47.md>

---

## 2. Current Gateway Implementation

### 2.1 Architecture

```
┌─────────────────────────────────────────────────┐
│                 HTTP Request                     │
│  x402_filter ──► X402PaymentPayload             │
│  POST /m2m/settle (Lightning rail)              │
│  POST /nwc/relay                                │
└────────────────────┬────────────────────────────┘
                     │
              ┌──────▼──────────┐
              │ LightningAdapter│  ← validation, replay guard, retry
              │ (orchestrator)  │
              └──────┬──────────┘
                     │
         ┌───────────┼───────────┐
         │           │           │
   ┌─────▼─────┐ ┌──▼───┐ ┌────▼──────┐
   │ Simulated │ │ NWC  │ │ Production│
   │ Backend   │ │Backend│ │ Backend   │
   │  (default)│ │(live) │ │  (stub)   │
   └───────────┘ └──────┘ └───────────┘
```

### 2.2 Code Surface

| Component | File | Lines | Status |
|-----------|------|-------|--------|
| Core types | `pkg/conxian-core/src/lightning.rs` | ~200 | Live |
| Adapter + backends | `internal/api/src/lightning.rs` | 806 | Live |
| NWC backend | `internal/api/src/nwc_backend.rs` | ~300 | Live |
| X402 middleware | `internal/api/src/x402.rs` | 776 | Live |
| M2M settlement | `internal/api/src/handlers.rs` | ~200 | Live |
| **Total** | | **~2,600** | |

### 2.3 Adapter Capabilities

| Capability | Status | Notes |
|-----------|--------|-------|
| X402 payment gating | ✅ Live | All protected API routes require Lightning payment |
| NWC relay settlement | ✅ Live | `POST /nwc/relay` |
| M2M Lightning settlement | ✅ Live | `POST /m2m/settle` with `settlement_rail: "Lightning"` |
| Replay protection | ✅ Live | `InMemoryReplayGuard` with challenge-based dedup |
| Payment lifecycle tracking | ✅ Live | `PaymentIntent` state machine with audit events |
| Error taxonomy | ✅ Live | 12 `LightningAdapterError` variants with HTTP mapping |
| Direct LND integration | ❌ Stub | `ProductionLightningBackend` returns `Unavailable` |
| Direct CLN integration | ❌ Stub | Same as LND — no JSON-RPC client |
| Multi-hop routing control | ❌ Not implemented | Relies on backend for path selection |
| Channel management | ❌ Not implemented | Delegated to backend |
| BOLT 12 Offers | ❌ Not implemented | BOLT 11 invoice-based only |
| Hold invoices | ❌ Not implemented | Stateless payment only |
| Multi-path payments (MPP) | Partial | Backend-dependent |

---

## 3. Gap Analysis

### 3.1 G-LN1: Production Lightning Backend (P2 — Medium Priority)

**Current:** `ProductionLightningBackend` is a stub that always returns
`Unavailable`.

**Gap:** No real LND or CLN integration exists. The NWC backend is the only
production-capable Lightning payment path, but it introduces an external
dependency on a NIP-47 wallet (e.g., Alby, Zeus, Mutiny).

**Evidence:**
- LND REST API: <https://lightning.engineering/api-docs/> — stable v1 API
- CLN JSON-RPC: <https://docs.corelightning.org/reference> — documented, stable
- Both are widely deployed in institutional contexts

**Promotion gates:**
1. Operator demand signal (at least one institutional operator requesting direct LND/CLN)
2. Define `LightningBackendConfig` with LND (macaroon + TLS cert) and CLN (rune + socket) variants
3. Implement `LndLightningBackend` wrapping `tonic` gRPC or REST client
4. Implement `ClnLightningBackend` wrapping JSON-RPC client
5. Add integration tests with regtest LND/CLN nodes
6. Add Prometheus metrics for Lightning backend health

**Estimated effort:** 3-5 days for either backend, 5-7 days for both.

### 3.2 G-LN2: BOLT 12 Offers Support (P3 — Low Priority)

**Current:** Payment requests use BOLT 11 invoices (the `challenge` field in
`X402PaymentPayload`).

**Gap:** BOLT 12 Offers enable reusable, static payment requests without the
invoice expiry problem. This would simplify M2M recurring settlement.

**Evidence:**
- BOLT 12 spec: <https://github.com/lightning/bolts/blob/master/12-offer-encoding.md>
- LND 0.18+ has experimental BOLT 12 support
- CLN 24.02+ has BOLT 12 support

**Promotion gates:**
1. At least one backend (LND or CLN) implemented (G-LN1)
2. BOLT 12 stable in target backend
3. Gateway uses offers for recurring M2M settlement

### 3.3 G-LN3: Channel Liquidity Management (P3 — Low Priority)

**Current:** Channel management is entirely delegated to the backend. The
Gateway has no visibility into channel balances or inbound/outbound capacity.

**Gap:** For high-throughput M2M settlement, operators need liquidity monitoring
and automated rebalancing signals.

**Evidence:**
- LND: `lncli channelbalance`, `lncli feereport`, `lncli queryroutes`
- CLN: `lightning-cli listfunds`, `lightning-cli getroutes`
- Loop: Lightning Labs' submarine swap service for inbound liquidity

**Promotion gates:**
1. Operator demand signal with specific liquidity requirements
2. Define `ChannelLiquidityMetrics` struct with balance, capacity, fee rate
3. Expose via `GET /api/v1/lightning/liquidity`
4. Integrate with Treasury monitor for sBTC ↔ Lightning arbitrage

---

## 4. Security Assessment

### 4.1 Custody Model

The Gateway **does not custody Lightning funds**. The `LightningBackend` trait
delegates all channel operations to an external node or NWC wallet. The Gateway
only:
- Validates payment payloads (expiry, asset, amount)
- Enforces replay protection
- Routes settlement receipts to compliance audit logs

This follows the "route without touch" principle established for Canton Network
integration.

### 4.2 Attack Surface

| Vector | Mitigation | Status |
|--------|-----------|--------|
| Replay attacks | `InMemoryReplayGuard` with challenge dedup | ✅ |
| Expired invoice payments | Expiry validation in `execute_payment` | ✅ |
| Amount mismatch | Post-settlement amount verification | ✅ |
| Missing/invalid preimage | Preimage non-empty + proof_refs containment check | ✅ |
| NWC relay MITM | TLS + Nostr event signatures (NIP-47) | ✅ |
| Backend timeout DoS | 250ms default timeout + retry cap | ✅ |
| Replay store corruption | Fail-closed: store failure → error, not silent pass | ✅ |

### 4.3 Production LND/CLN Hardening Requirements

When G-LN1 is implemented, additional hardening is required:
- **Macaroon/rune scoping:** Minimal permissions (invoice payment only, no channel open/close)
- **TLS/mTLS:** Mutual TLS for LND gRPC
- **Network isolation:** Backend node on same host or private network
- **Read-only macaroon:** Gateway should never need admin permissions

---

## 5. M2M Lightning Settlement Integration

### 5.1 Canton Network Cross-Reference

The Canton Network Machine Economy research (G-C3: M2M Settlement) identifies
Lightning as the primary M2M settlement rail for autonomous machine-to-machine
payments. Key integration points:

| Canton Gap | Lightning Role | Status |
|-----------|---------------|--------|
| G-C3: M2M Settlement | `settle_m2m` handler with Lightning rail | ✅ Live |
| G-C2: Machine Identity | Identity resolution for LN node pubkeys | ✅ Live |
| G-C6: RWA Revenue Verification | Lightning settlement receipts as revenue proof | Research |
| G-C7: DePIN Compliance | LN payment → compliance verification pipeline | Research |

### 5.2 Volume Estimates

Based on the Canton research (`CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md`):

- peaq ecosystem: 60+ DePINs, ~$180M TVL
- Lightning M2M volume: ~$1.1B/month (estimated across peaq + emerging DePINs)
- Projected Gateway M2M Lightning TAM: $50-200M/month at 5-20% market capture
- Average M2M payment: $0.01-10.00 (machine data/energy micropayments)

---

## 6. Decision Gates Summary

| Gate | Status | Blocking |
|------|--------|----------|
| NWC relay settlement | ✅ Deployed | — |
| X402 payment gating | ✅ Deployed | — |
| Replay protection | ✅ Deployed | — |
| Payment lifecycle tracking | ✅ Deployed | — |
| Direct LND integration | ❌ G-LN1 | Operator demand |
| Direct CLN integration | ❌ G-LN1 | Operator demand |
| BOLT 12 Offers | ❌ G-LN2 | G-LN1 + backend support |
| Channel liquidity management | ❌ G-LN3 | Operator demand |
| Hold invoices | ❌ Not planned | Use case undefined |
| Multi-path payments (MPP) | Partial | Backend-dependent |

---

## 7. Cross-References

- **ADAPTER_FAMILY_STRATEGY.md:** Lightning classified as T1 Production, 806-line handler
- **CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md:** G-C3 M2M Settlement via Lightning
- **NOSTR_WALLET_CONNECT_NWC.md:** NWC implementation details
- **CANDIDATE_MATRIX.md:** NWC at maturity 7 (Production)
- **KNOWLEDGE_MAP.md:** Lightning adapter in settlement rails diagram
- **Issue #245 (BIP-110):** Fee market telemetry may inform Lightning routing decisions
- **API surface:** `POST /m2m/settle`, `POST /nwc/relay`, x402 middleware

---

## 8. Recommendations

1. **Maintain NWC as primary production path.** NIP-47 wallets are secure,
   widely available, and eliminate the need for Gateway to manage channel state.

2. **Defer G-LN1 (direct LND/CLN) until operator demand.** The NWC backend is
   production-capable. Direct integration adds operational complexity (channel
   management, macaroon/rune rotation) without clear demand.

3. **Implement Lightning-specific Prometheus metrics.** At minimum:
   `conxian_gateway_lightning_payments_total{backend,status}` and
   `conxian_gateway_lightning_payment_duration_seconds`.

4. **Add NWC backend health check.** The NWC backend should expose a health
   endpoint (e.g., periodic `get_balance` probe) to detect backend degradation.

5. **Monitor BOLT 12 adoption.** When major implementations stabilize BOLT 12,
   migrate M2M settlement from BOLT 11 to BOLT 12 for reusable offers.
