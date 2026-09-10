# Conxian System Architecture: Client Journey, Installation & Unified Installer Blueprint

This document defines the end-to-end client onboarding lifecycle, system installation process, multi-component deployment architecture, external rail connectivity, and the strategic blueprint for a **Unified Conxian CLI / Installer (`conxian-cli`)**.

---

## 1. Executive Overview & Client Value Proposition

Conxian provides an institutional-grade, non-custodial cross-border settlement and compliance infrastructure bridging traditional finance (SWIFT ISO 20022 `pacs.008`/`camt.053`), sovereign digital currencies (BRICS mBridge, PAPSS, SPFS), enterprise smart contract platforms (Canton Network Daml eUTXO), DePIN/Machine Economy (peaq DLT), and permissionless Bitcoin layer-1/layer-2 settlement rails (sBTC, DLCs, Lightning, Babylon, Fedimint, Liquid, BitVM).

When an institutional client (bank, central bank, enterprise treasury, or DePIN fleet operator) purchases or licenses Conxian, they receive access to the **Conxian Sovereign Settlement Suite**.

---

## 2. Client Onboarding & Purchase Flow

```
+-----------------------------------------------------------------------------------+
| 1. License & Enterprise Provisioning                                              |
|    - License agreement execution & Organization ID issuance in Conxian Portal      |
|    - Generation of deployment credentials, API tokens, and trust tier (T1/T2) keys  |
+-----------------------------------------------------------------------------------+
                                         │
                                         ▼
+-----------------------------------------------------------------------------------+
| 2. Unified Deployment (`conxian-cli init`)                                        |
|    - Single-command CLI or Docker Compose bundle execution                        |
|    - Automated environment validation, HSM/Enclave pairing, and node RPC checks    |
+-----------------------------------------------------------------------------------+
                                         │
                                         ▼
+-----------------------------------------------------------------------------------+
| 3. Local Node & Gateway Service Initialization                                     |
|    - Conxian Gateway (`cmd/gateway`) starts Axum HTTP/REST API                    |
|    - Conxian Nexus (`conxian-nexus`) initiates chain state listeners & indexers   |
|    - Conxius Enclave SDK (`conxius-enclave-sdk`) pairs with client HSM/KMS        |
|    - Control Plane UI (`apps/control-plane`) serves management dashboard           |
+-----------------------------------------------------------------------------------+
                                         │
                                         ▼
+-----------------------------------------------------------------------------------+
| 4. Multi-Rail Settlement & ERP Integration                                        |
|    - ISO 20022 XML parsing, BRICS mBridge DLT state proofs, Canton ACS state      |
|    - OData v4 ERP webhook callbacks (SAP/Oracle) & Zero-Trust Wasm UCV-1           |
+-----------------------------------------------------------------------------------+
```

### 2.1 What Clients License & Receive
1. **Conxian Gateway (`conxian-gateway`)**: High-throughput, SLA-grade async Rust service handling compliance normalization, ISO 20022 payment generation, zero-trust proof verifications, and REST API routes.
2. **Conxian Nexus (`conxian-nexus`)**: Protocol coordination engine managing cross-chain state synchronization and L1/L2 header verification.
3. **Conxius Enclave SDK (`conxius-enclave-sdk`)**: Hardware Security Module (HSM) and KMS non-custodial key abstraction layer ensuring private keys never touch Conxian servers.
4. **Conxian Control Plane UI (`apps/control-plane`)**: Next.js management web application for real-time transaction monitoring, trust policy configuration, and audit logs.
5. **@conxian/client-sdk & @conxian/schemas**: TypeScript client libraries for embedding zero-trust verification and payment generation directly into client enterprise backends.

---

## 3. Client Environment Inputs & Configuration Requirements

To achieve production readiness, the client configures the following inputs during setup:

| Category | Input Parameter | Description & Requirement |
|---|---|---|
| **Authentication** | `CONXIAN_GATEWAY_AUTH_TOKEN` | Bearer token for API protection (must not use `sentinel_` prefix in prod) |
| **Trust Policy** | `x-conxian-trust-metadata` | Ingress header specifying system (`IBC`), trust tier (`T1`), policy context, and evidence |
| **Blockchain Nodes** | `BITCOIN_RPC_URL`, `STACKS_RPC_URL` | Client or dedicated Bitcoin L1 and Stacks node RPC endpoints |
| **Signing & Enclave** | `ENCLAVE_KMS_KEY_ID`, `AWS_KMS_ARN` | Hardware key references for non-custodial transaction authorization |
| **Database & Persistence** | `DATABASE_URL` / SQLite | Local or PostgreSQL persistence for state tracking and offline queue |
| **ERP Webhooks** | `ODATA_V4_WEBHOOK_URL` | Enterprise ERP (SAP/Oracle) callback URL for real-time `camt.053` updates |

---

## 4. Multi-Rail System Connectivity Architecture

```
                                  +---------------------------------------+
                                  |     Client Enterprise System (ERP)    |
                                  |    (SAP, Oracle, SWIFT Alliance)      |
                                  +---------------------------------------+
                                                      │
                                                      │ ISO 20022 / OData v4
                                                      ▼
+---------------------------------------------------------------------------------------------------+
|                                     CONXIAN SOVEREIGN GATEWAY                                     |
|                                                                                                   |
|  +-------------------------+   +--------------------------+   +--------------------------------+  |
|  |  ISO 20022 / pacs.008   |   |   BRICS mBridge Adapter  |   |   Canton Daml ACS / CCIP Gate  |  |
|  |  Compliance Normalizer  |   |   HotStuff DLT Proofs    |   |   eUTXO UCR State Translator   |  |
|  +-------------------------+   +--------------------------+   +--------------------------------+  |
|                                                                                                   |
|  +-------------------------+   +--------------------------+   +--------------------------------+  |
|  |   sBTC L1 Tx & Header   |   |   Fedimint Blind Sig     |   |   peaq DePIN / M2M Settlement  |  |
|  |   Double-SHA256 & PoW   |   |   Schnorr Verification   |   |   Lightning X402 Micro-payments|  |
|  +-------------------------+   +--------------------------+   +--------------------------------+  |
+---------------------------------------------------------------------------------------------------+
                                                      │
                                                      │ Zero-Trust State Proofs & UTXOs
                                                      ▼
+---------------------------------------------------------------------------------------------------+
|                                    EXTERNAL SETTLEMENT RAILS                                      |
|                                                                                                   |
|  +-------------------+  +-------------------+  +-------------------+  +----------------------------+  |
|  | Bitcoin L1 / DLCs |  | Stacks L2 / sBTC  |  | Canton Network    |  | BRICS CBDC DLT / peaq      |  |
|  +-------------------+  +-------------------+  +-------------------+  +----------------------------+  |
+---------------------------------------------------------------------------------------------------+
```

---

## 5. Architectural Recommendation: Unified Installer (`conxian-cli`)

### 5.1 Current Setup vs. Target Unified Setup
- **Current State**: Client manually builds or runs Docker containers for `conxian-gateway`, `conxian-nexus`, and `apps/control-plane`, managing multiple `.env` files and configuration scripts across repositories.
- **Target Architecture (`conxian-cli`)**: A single binary CLI tool (`conxian-cli init`) that automates setup, environment validation, database migration, HSM pairing, and container orchestration.

### 5.2 `conxian-cli` Command Blueprint

```bash
# Initialize interactive setup wizard
conxian-cli init --tier T1 --mode production

# Perform automated preflight node and network checks
conxian-cli doctor

# Start unified stack (Gateway, Nexus, Control Plane)
conxian-cli start --detach

# Monitor health and multi-rail sync status
conxian-cli status
```

---

## 6. Implementation Roadmap for Unified Installer

1. **Phase 1 (Q4 2026)**: Create Docker Compose production bundle incorporating `conxian-gateway`, `conxian-nexus`, `apps/control-plane`, and PostgreSQL.
2. **Phase 2 (Q4 2026)**: Develop `conxian-cli` Rust binary wrapping environment validation, `verify_contamination_guard.py` preflight, and container orchestration.
3. **Phase 3 (Q1 2027)**: Integrate automated HSM/KMS zero-knowledge key pairing wizard in `conxian-cli init`.
