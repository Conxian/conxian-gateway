# Conxian Master Reconnaissance & Architecture Review

**Version:** 0.1.5
**Date:** September 16, 2026
**Author:** Lead Systems Engineer (Jules)
**Domain Policy:** Stateless B2B Enclave Infrastructure

---

## Executive Summary

Conxian operates as a pure Deep-Tech B2B infrastructure vendor licensing stateless container images and hardware execution parameters for enterprise clients to run entirely within their own private clouds or hardware enclaves. Conxian does **NOT** operate public DeFi protocols.

This Master Reconnaissance & Architecture Review documents the organizational baseline, dependency graph, strict legal and architectural domain routing, B2B deployment lifecycle, open technical gap matrix, and execution plan for upcoming engineering cycles.

---

## 1. Domain Separation & Routing Architecture

To maintain a strict legal and architectural firewall between the open-source protocol distribution surface and the corporate business surface, the following domain routing topology is strictly enforced across all CORS policies, reverse proxies, and API contracts:

| Component Surface | Target Domain | Surface Type & Purpose |
|---|---|---|
| `conxian-nexus` | `nexus.conxian.org` | Protocol coordination & state verification |
| `conxian-gateway` | `gateway.conxian.org` | Institutional compliance & payment gateway |
| `conxius-enclave-sdk` | `sdk.conxian.org` | Public developer SDK & enclave types |
| `conxius-platform` | `platform.conxian.org` | Internal orchestration & platform coordination |
| `conxian_market` | `market.conxian.org` | Treasury transparency dashboard |
| `conxian-business` | `bos.conxian-labs.com` | Business Operations System (BOS) |
| `conxian-labs-site` | `www.conxian-labs.com` | Corporate B2B sales, legal & governance |

---

## 2. Org-Wide Dependency Graph & Primitive Review

`conxius-platform` orchestrates deployment and execution across core Rust primitives:

```
                          +-------------------------+
                          |   lib-conxian-core      |
                          | (Common Types/Persistence)
                          +-------------------------+
                                       │
                ┌──────────────────────┴──────────────────────┐
                ▼                                             ▼
  +---------------------------+                 +---------------------------+
  |    conxian-gateway        |                 |     conxian-nexus         |
  | (Axum HTTP API / ISO20022)|                 | (State Sync & Headers)    |
  +---------------------------+                 +---------------------------+
                │                                             │
                └──────────────────────┬──────────────────────┘
                                       ▼
                          +-------------------------+
                          |   conxius-enclave-sdk   |
                          | (HSM/KMS Key Hardware)  |
                          +-------------------------+
```

### Clean Architecture Findings
- **Zero Altcoin/Public Protocol Contamination**: Purged legacy non-sovereign dependencies.
- **Hardware Enclave Isolation**: KMS/HSM signing logic isolated within `conxius-enclave-sdk`.
- **Fail-Closed Verification**: Zero-trust proof verification (UCV-1, Groth16, Schnorr) fails closed without valid proofs or job card context.

---

## 3. B2B Client Deployment Simulation

### 3.1 License Pull Artifacts
Clients pull stateless container images and SDKs from `conxian.org`:
- Docker Containers: `registry.conxian.org/gateway:v0.1.5`, `registry.conxian.org/nexus:v0.1.5`
- Client SDK: `@conxian/client-sdk` & `@conxian/schemas` via NPM / GitHub Packages

### 3.2 Required Client Environment Inputs
- `CONXIAN_GATEWAY_AUTH_TOKEN`: Ingress Bearer authentication token
- `x-conxian-trust-metadata`: Trust tier (T1/T2), IBC context, and evidence
- `BITCOIN_RPC_URL` & `STACKS_RPC_URL`: Dedicated node RPC endpoints
- `ENCLAVE_KMS_KEY_ID`: Client HSM key pointer
- `ODATA_V4_WEBHOOK_URL`: Enterprise ERP (SAP/Oracle) callback URL

### 3.3 End-to-End Payment & Settlement Flow
1. **ISO 20022 Ingress**: ERP sends `pacs.008` XML payment request to Gateway (`POST /api/v1/fiat/iso20022/pacs008`).
2. **Compliance & Sanctions**: Gateway validates XML structure, executes zero-knowledge KYC/AML compliance checks (`ZkcVerifier`), and sanitizes PII.
3. **Multi-Rail Routing**: Gateway routes payment based on target rail (Bitcoin L1, sBTC, DLC, Canton, BRICS mBridge, peaq DePIN).
4. **ERP Webhook Sync**: Gateway dispatches real-time `camt.053` OData v4 JSON callbacks (`dispatch_odata_v4_webhook`) to the client ERP.

### 3.4 Unified Installer (`conxian-cli`) Efficacy
`cmd/conxian-cli` provides an interactive preflight installer validating environment flags, secret entropy, and RPC connectivity prior to container startup.

---

## 4. Gap Analysis & Criticality Matrix

| Gap ID | Description | Layer | Risk | Impact | Effort | Priority Score | Status / Target Solution |
|---|---|---|:---:|:---:|:---:|:---:|---|
| **G-20** | BitVM3 / BitVMX Execution & Recursive Proof Verifier | Core Engine | 2 | 4 | 4 | **8** | 🟡 Candidate Q (BitVM3 proof verifier expansion) |
| **G-DL2**| DLC Bond Orchestration & On-Chain Funding | DLC Rail | 3 | 4 | 3 | **12** | ✅ Candidate G-DL2 (Funding Tx Builder, Persistence & Monitoring Engine Shipped) |
| **G-C5** | Canton CCIP Cross-Chain Chainlink Verifier | Canton Rail | 3 | 4 | 2 | **12** | 🟡 Candidate S / G-C5 (Integrate Chainlink CCIP verifier) |
| **G-TR1**| OData v4 ERP Webhook Retry & Backoff Persistence | API / ERP | 2 | 3 | 2 | **6** | ✅ Candidate T / G-TR1 (Implemented; add persistent queue) |

*Scoring: Priority Score = Risk × Impact (higher = address first).*

---

## 5. Next Immediate Code-Generation Tasks

1. **G-C5 / Canton CCIP On-Chain Verifier**: Implement Chainlink CCIP on-chain digest verifier in `internal/api/src/handlers.rs`.
2. **G-DL2 / DLC Production Funding & Monitoring**: Extend `DlcExecutionEngine` to manage initial funding transaction broadcast and UTXO state persistence.
3. **G-TR1 / OData v4 Retry Queue**: Implement persistent SQLite retry queue for failed OData v4 ERP webhook callbacks.

---

## 6. Org-Wide SLA Positioning & Governance

To reconcile open-source funding models with enterprise expectations across Conxian's expansive attack surface (`lib-conxian-core`, `conxius-enclave-sdk`, `conxius-wallet`, `conxian-nexus`, `conxian-gateway`), the organization enforces a strict **Tiered Support & Liability Matrix**:

1. **Public Repositories & Core Protocol (`conxian.org`)**: Strictly **No SLA** or uptime guarantees. Provided under standard open-source disclaimers (MIT).
2. **Enterprise / Gateway Tier (`conxian-gateway` / B2B Adapters)**: SLAs are offered **exclusively** under signed B2B commercial contracts managed via `conxian-business`.
3. **Scope Bounding**: Enterprise SLAs are strictly limited to integration support, configuration assistance, and business-hour response times—**never absolute network uptime** on underlying decentralized consensus layers or vendor hardware TEEs.
4. **Automated Guardrails**: Core protocol integrity is enforced via automated CI guardrails, contamination scans, release artifact verification, and strict staging branches (`dev` -> `staged` -> `main`).
