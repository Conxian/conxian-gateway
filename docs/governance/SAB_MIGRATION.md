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

## 2. Migration Waves (CON-332)

### Wave 1: Data Decoupling (Q2 2026 - M1)
- **Goal**: Move critical settlement logs and job cards from Supabase to Tableland.
- **Dependency**: None.
- **Readiness Gate**: `SovereignCommit` hooks verified in ZKC layer.
- **Execution**: Deploy Tableland validator and migrate existing rows via encrypted stream.
- **Rollback**: Maintain read-only replica in Supabase for 30 days.

### Wave 2: Identity Sovereignty (Q2 2026 - M2)
- **Goal**: Implement Workload Identity Federation (WIF) to remove reliance on external OIDC providers.
- **Dependency**: Wave 1 (for enclave identity metadata storage in Tableland).
- **Readiness Gate**: Successful Enclave-to-GCP token exchange simulation.
- **Execution**: Update `IdentityManager` to verify Enclave-signed OIDC tokens directly.
- **Rollback**: Fallback to traditional Google/GitHub OIDC if verification latency > 2s.

### Wave 3: Execution Autonomy (Q3 2026 - M1)
- **Goal**: Host the Conxian Gateway and Nexus engines on SAB-owned bare metal or sovereign nodes.
- **Dependency**: Wave 2 (for decentralized auth of node clusters).
- **Readiness Gate**: Docker Swarm cluster verified with >99.9% uptime in staged environment.
- **Execution**: Cut over DNS from Render to Sovereign Node cluster.
- **Rollback**: Keep Render secondary cluster active; redirect traffic via Cloudflare LB if health degraded.

### Wave 4: Full Institutional Cutover (Q4 2026)
- **Goal**: Decommission all third-party cloud accounts.
- **Dependency**: Waves 1, 2, and 3 verified as stable for 90 days.
- **Readiness Gate**: External security audit of Sovereign Node architecture.
- **Execution**: Delete Neon/Supabase/Render projects after final data snapshot verification.
- **Rollback**: Final snapshots stored in cold-storage (Arweave/Filecoin) for multi-year compliance.

## 3. Readiness Gates for Cutover
- [x] **Data Integrity**: Verified parity between Neon and local Postgres snapshots.
- [ ] **Secret Hygiene**: All `CHANGEME` sentinels replaced with production secrets in SAB vault.
- [x] **Connectivity**: Sovereign RPC endpoints for Bitcoin and Stacks verified as stable.

## 4. Rollback Summary
| Surface | Primary Trigger | Recovery Action |
| :--- | :--- | :--- |
| **Database** | Migration mismatch / Data loss | Restore from point-in-time snapshot. |
| **Identity** | High latency / Attestation failure | Reactivate traditional OAuth paths. |
| **API** | Node cluster instability | DNS reroute to secondary Render cluster. |

## 5. Go/No-Go Decision Rules
1. **Latency**: Any sovereign service exceeding 150% of Web2 baseline latency triggers a wave-specific rollback.
2. **Integrity**: Any ZKC verification mismatch between Web2 and Sovereign logs triggers an immediate Wave 1 halt.
3. **Availability**: Sovereign Node cluster must maintain 99.9% availability over 7 consecutive days before DNS cutover.
