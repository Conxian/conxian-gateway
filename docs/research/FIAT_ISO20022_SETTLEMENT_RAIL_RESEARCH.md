# Fiat Settlement Rail: ISO 20022 + Global Payment Networks

**Status:** Live (T1 Production) | **Lines:** 1,396 (fiat.rs + camt.rs + x402.rs)
**Last refreshed:** 2026-08-07 | **Session:** 49

---

## Executive Summary

The Fiat settlement rail enables Conxian Gateway to interface with traditional
banking and international payment networks through ISO 20022 messaging (CAMT.053
bank statements, CAMT.054 credit/debit notifications) and fiat on/off-ramp
providers (Ramp, Investec, AlchemyPay, Banxa). The rail is classified as **T1
Production** across the entire adapter family strategy.

**Current state:**
- **Fiat on/off-ramp:** 4 provider session builders (redirect URL construction)
  + HMAC-SHA256 webhook verification for payment confirmations
- **ISO 20022 (CAMT):** `camt.053` bank statement and `camt.054` notification
  XML generation via `writeln!` string formatting
- **X402 payment gating:** HTTP 402 middleware protecting settlement endpoints
- **BRICS corridors:** SPFS, PAPSS, CIPS, mBridge — all referenced in fiat
  routing but implemented as placeholder stubs

**Decision:** Fiat remains T1 Production. The on-ramp session builders and CAMT
XML generators are functional for institutional banking integration. BRICS
corridor deep integration is deferred pending operator demand and regulatory
clarification.

---

## 1. Protocol Evidence

### 1.1 ISO 20022

ISO 20022 is the global standard for financial messaging, mandated by SWIFT for
cross-border payments (migration deadline: November 2025).

| Message | Type | Gateway Status |
|---------|------|----------------|
| **camt.053** | Bank-to-Customer Statement | ✅ XML generation |
| **camt.054** | Bank-to-Customer Debit/Credit Notification | ✅ XML generation |
| **pacs.008** | Customer Credit Transfer | ❌ Not implemented |
| **pacs.009** | Financial Institution Credit Transfer | ❌ Not implemented |
| **pacs.010** | Direct Debit | ❌ Not implemented |

Source: <https://www.iso20022.org/catalogue-messages>

### 1.2 BRICS Payment Corridors

The Gateway's fiat routing references 4 BRICS-aligned payment networks:

| Network | Full Name | Jurisdiction | Gateway Status |
|---------|-----------|-------------|----------------|
| **SPFS** | System for Transfer of Financial Messages | Russia (Bank of Russia) | ✅ Referenced |
| **PAPSS** | Pan-African Payment and Settlement System | Africa (Afreximbank) | ✅ Referenced |
| **CIPS** | Cross-Border Interbank Payment System | China (PBOC) | ✅ Referenced |
| **mBridge** | Multiple CBDC Bridge | BIS Innovation Hub + 4 central banks | ✅ Referenced |

Source: `BRICS_FINANCIAL_SYSTEMS_RESEARCH.md`

### 1.3 Fiat On/Off-Ramp Providers

| Provider | Region | Integration Type | Gateway Status |
|----------|--------|-----------------|----------------|
| **Ramp** | Global | Redirect URL + HMAC webhook | ✅ Live |
| **Investec** | UK/South Africa | Redirect URL + HMAC webhook | ✅ Live |
| **AlchemyPay** | APAC | Redirect URL + HMAC webhook | ✅ Stub (CON-41) |
| **Banxa** | Global | Redirect URL + HMAC webhook | ✅ Stub (CON-41) |

---

## 2. Current Gateway Implementation

### 2.1 Architecture

```
HTTP Request
    │
    ├─ POST /fiat/onramp/session
    │   └─ FiatRouter::create_session(request)
    │       ├─ create_ramp_session()      → buy.ramp.network
    │       ├─ create_investec_session()  → investec.com/banking/pay
    │       ├─ create_alchemypay_session()→ ramp.alchemypay.org  (CON-41)
    │       └─ create_banxa_session()     → conxian-labs.banxa.com (CON-41)
    │
    ├─ POST /fiat/webhook
    │   └─ FiatRouter::verify_webhook(payload, secret)
    │       └─ HMAC-SHA256 verification per provider
    │
    ├─ POST /ingress/iso20022 (CAMT.053)
    │   └─ generate_camt053(state, request)
    │       └─ build_camt053_xml() → camt.053.001.08
    │
    └─ POST /ingress/iso20022/camt054
        └─ generate_camt054(state, request)
            └─ build_camt054_xml() → camt.054.001.08
```

### 2.2 Code Surface

| Component | File | Lines | Status |
|-----------|------|-------|--------|
| Fiat router | `internal/api/src/fiat.rs` | 476 | Live |
| CAMT handlers | `internal/api/src/camt.rs` | 144 | Live |
| X402 middleware | `internal/api/src/x402.rs` | 776 | Live |
| **Total** | | **1,396** | |

### 2.3 Capabilities

| Capability | Status | Notes |
|-----------|--------|-------|
| On-ramp session creation | ✅ Live | 4 providers, redirect URL construction |
| Webhook verification | ✅ Live | HMAC-SHA256, per-provider secrets |
| CAMT.053 statement generation | ✅ Live | `writeln!` XML; `include_transactions` field unused |
| CAMT.054 notification generation | ✅ Live | Credit/debit indicator, booking/value dates |
| X402 payment gating | ✅ Live | HTTP 402 middleware on settlement endpoints |
| BRICS corridor integration | ⬜ Placeholder | Routes referenced, no protocol integration |
| pacs.008/009/010 | ❌ Not implemented | Core ISO 20022 credit transfer messages |
| CAMT XML schema validation | ❌ Not implemented | No XSD validation of generated XML |

---

## 3. Gap Analysis

### 3.1 G-FI1: XML Schema Validation (P2 — Medium Priority)

**Current:** CAMT XML is generated via `writeln!` string formatting without
validation against ISO 20022 XSD schemas.

**Gap:** Generated XML may not conform to the ISO 20022 XSD schema. A
single-char typo in the format string would produce invalid XML accepted by
the Gateway but rejected by the recipient bank.

**Evidence:**
- ISO 20022 XSD schemas are published at <https://www.iso20022.org>
- `quick-xml` crate supports XML validation (already in workspace or easily added)
- XSD validation is standard practice for SWIFT/cross-border messaging

**Promotion gates:**
1. Bundle `camt.053.001.08.xsd` and `camt.054.001.08.xsd` as test fixtures
2. Validate generated XML against XSD in CI
3. Add validation to handler path (reject non-conformant XML)
4. Return structured error with XSD violation details

### 3.2 G-FI2: pacs.008 Credit Transfer (P2 — Medium Priority)

**Current:** Only CAMT (statement) messages are generated. No credit transfer
(pacs) messages are supported.

**Gap:** `pacs.008` is the core ISO 20022 message for customer credit
transfers — the fundamental message for sending payments. Without it, the
Gateway cannot initiate cross-border fiat payments.

**Evidence:**
- `pacs.008.001.08` is the current SWIFT-mandated format
- Message structure: GroupHeader + CreditTransferTransactionInformation
- Required fields: Debtor, DebtorAccount, Creditor, CreditorAccount, Amount,
  Currency, SettlementMethod

**Promotion gates:**
1. Define `Pacs008Request` struct with required ISO 20022 fields
2. Implement `build_pacs008_xml()` following ISO 20022 XSD
3. Integrate with SPFS/PAPSS/CIPS corridor routing
4. Add HMAC signing for message integrity
5. End-to-end test with recipient bank sandbox

### 3.3 G-FI3: BRICS Corridor Protocol Integration (P2 — Medium Priority)

**Current:** BRICS corridor names are referenced in fiat routing but there
is no protocol-level integration with any BRICS payment network.

**Gap:** The Gateway cannot actually send or receive payments over SPFS,
PAPSS, CIPS, or mBridge. The routes are addressable but generate stub
responses.

**Evidence:**
- SPFS uses ISO 20022 messages (same as CAMT/pacs)
- CIPS uses ISO 20022 + China-specific extensions
- PAPSS uses ISO 20022 + African currency settlement
- mBridge uses DLT-based CBDC transfer (not ISO 20022)

**Promotion gates:**
1. Define `PaymentCorridor` enum with protocol-specific adapters
2. Implement SPFS adapter: ISO 20022 over dedicated network
3. Implement CIPS adapter: ISO 20022 + CNY-specific fields
4. Implement PAPSS adapter: ISO 20022 + Afreximbank settlement
5. mBridge: DLT integration (separate research needed)

### 3.4 G-FI4: On-Ramp Provider Testing (P3 — Low Priority)

**Current:** 2 of 4 providers (AlchemyPay, Banxa) are labeled CON-41 stubs
with `#[allow(dead_code)]` fields.

**Gap:** The HMAC secrets for these providers are stored but never tested
against their actual webhook endpoints. The session builders construct URLs
but the full end-to-end flow is untested.

**Promotion gates:**
1. Acquire sandbox API keys for AlchemyPay and Banxa
2. Implement end-to-end integration tests with sandbox endpoints
3. Remove `#[allow(dead_code)]` annotations

---

## 4. Security Assessment

### 4.1 Webhook Security

| Provider | Verification | Status |
|----------|-------------|--------|
| Ramp | HMAC-SHA256(secret, raw_payload) | ✅ Live |
| Investec | HMAC-SHA256; fails-closed if secret empty | ✅ Live |
| AlchemyPay | HMAC-SHA256 | ✅ Stub |
| Banxa | HMAC-SHA256 | ✅ Stub |

### 4.2 X402 Payment Protection

The X402 middleware protects settlement endpoints requiring payment:
- `POST /settle` — payment required for settlement
- `POST /ingress/iso20022` — payment required for CAMT generation
- `POST /ingress/papss` — payment required for PAPSS settlement
- `POST /ingress/brics` — payment required for BRICS settlement
- `POST /erp/sync` — payment required for ERP integration

Replay protection via `InMemoryReplayGuard` on invoice challenges.

### 4.3 XML Injection Risk

The CAMT handlers use `writeln!` string formatting with user-provided fields
(account_id, amount, currency). These are NOT sanitized for XML injection.

**Risk:** If account_id contains `<` or `&`, the generated XML will be
malformed or could enable XML injection in downstream bank systems.

**Mitigation:** Add XML entity escaping for all user-provided fields.
`quick-xml` crate supports safe element construction.

---

## 5. Decision Gates Summary

| Gate | Status | Blocking |
|------|--------|----------|
| On-ramp session creation (4 providers) | ✅ Live | — |
| HMAC webhook verification | ✅ Live | — |
| CAMT.053 bank statement | ✅ Live | — |
| CAMT.054 notification | ✅ Live | — |
| X402 payment gating | ✅ Live | — |
| XML schema validation | ❌ G-FI1 | Institutional readiness |
| pacs.008 credit transfer | ❌ G-FI2 | Payment initiation |
| BRICS protocol integration | ❌ G-FI3 | Operator demand |
| On-ramp provider testing (2/4) | ❌ G-FI4 | Sandbox access |

---

## 6. Cross-References

- **ADAPTER_FAMILY_STRATEGY.md:** Fiat at T1 Production (776 lines)
- **BRICS_FINANCIAL_SYSTEMS_RESEARCH.md:** Full SPFS/PAPSS/CIPS/mBridge analysis
- **LIGHTNING_SETTLEMENT_RAIL_RESEARCH.md:** X402 middleware integration
- **SBTC_SETTLEMENT_RAIL_RESEARCH.md:** Treasury/SYI fiat FX integration
- **CON-41:** Industry Enhancement (AlchemyPay + Banxa)
- **API surface:** `POST /fiat/onramp/session`, `POST /fiat/webhook`,
  `POST /ingress/iso20022`, `POST /ingress/iso20022/camt054`

---

## 7. Recommendations

1. **Fix XML injection immediately.** Add entity escaping (`&` → `&amp;`,
   `<` → `&lt;`, `>` → `&gt;`) for all user-provided fields in CAMT XML
   generation. This is a security concern for institutional banking.

2. **Prioritize G-FI1 (XSD validation).** Schema validation is the
   difference between "generates XML" and "generates ISO 20022-compliant
   XML." Banks reject non-compliant messages silently.

3. **Defer G-FI2 (pacs.008) per operator demand.** Credit transfer
   initiation requires banking partner integration, not just XML generation.

4. **Use quick-xml for CAMT generation.** Replace `writeln!` formatting
   with a proper XML builder to eliminate injection risk and ensure
   well-formed output.
