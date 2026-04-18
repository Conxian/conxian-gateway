# SAB Infrastructure Migration Control Plane (CON-329 / CON-337)

This document tracks the migration of Web2 infrastructure dependencies to a sovereign, SAB-owned target state.

## 1. Current Dependency Inventory

| Service Surface | Provider | Responsibility | Target State |
| :--- | :--- | :--- | :--- |
| **Institutional Ledger** | Neon | Serverless Postgres (Transactional) | Sovereign-hosted Postgres on SAB Node. |
| **Identity & Auth** | Supabase | OIDC and User Metadata | TEE-backed Enclave Identity (WIF). |
| **Job Card Storage** | Supabase | JSON-LD Persistence | Tableland (Decentralized SQL). |
| **Financial Modeling** | Supabase | Real-time 3-Statement and ARR | Local Engine with TEE Verification. |
| **API Gateway** | Render | High-availability Web Services | Docker Swarm on Sovereign Node. |

## 2. Migration Waves

### Wave 1: Data Decoupling
- **Goal**: Move critical settlement logs and job cards from Supabase to Tableland.
- **Status**: Tableland SQL simulation implemented in `ZkcVerifier` via `SovereignCommit`.

### Wave 2: Identity Sovereignty
- **Goal**: Implement Workload Identity Federation (WIF) to remove reliance on external OIDC providers.
- **Status**: `IdentityManager` support for OIDC exchange implemented.

### Wave 3: Execution Autonomy
- **Goal**: Host the Conxian Gateway and Nexus engines on SAB-owned bare metal or sovereign nodes.
- **Status**: Standardizing Docker Compose reference stacks.

## 3. Readiness Gates for Cutover
- [ ] **Data Integrity**: Verified parity between Neon and local Postgres snapshots.
- [ ] **Secret Hygiene**: All `CHANGEME` sentinels replaced with production secrets in SAB vault.
- [ ] **Connectivity**: Sovereign RPC endpoints for Bitcoin and Stacks verified as stable.

## 3. Execution Timeline (CON-332 / CON-336)

| Wave | Objective | Target Milestone | Status |
| :--- | :--- | :--- | :--- |
| **Wave 1** | Data Decoupling & Tableland | Q2 2026 - M1 | In Progress |
| **Wave 2** | Identity Sovereignty (WIF) | Q2 2026 - M2 | Planned |
| **Wave 3** | Runtime Autonomy (Docker Swarm) | Q3 2026 - M1 | Planned |
| **Wave 4** | Full Institutional Cutover | Q4 2026 | Planned |

## 4. Rollback Plan
- **Data**: Maintain read-only replicas in Neon/Supabase for 30 days post-migration.
- **Identity**: Fallback to traditional OIDC if WIF attestation fails SLA thresholds.
- **Runtime**: Redirect traffic to Render secondary cluster if sovereign nodes experience >5% degraded health.
