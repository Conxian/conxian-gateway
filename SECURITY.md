# Conxian Gateway Security Policy

This document defines the security posture, vulnerability disclosure procedures, supported releases, secret incident protocols, and threat boundaries for **Conxian Gateway**.

---

## 1. Supported Versions

Security updates and vulnerability patches are actively produced for the following release branches:

| Version | Supported | Security Patch Policy |
| :--- | :--- | :--- |
| **`v0.1.5`** (or `main`) |  **Active** | Full security maintenance, high-priority patch releases. |
| **`v0.1.4`** and earlier | ❌ **End of Life** | Unsupported. Operators must upgrade to `v0.1.5` or later. |

---

## 2. Reporting a Vulnerability

If you discover a potential security vulnerability, zero-day exploit, or secret exposure in Conxian Gateway, **do not open a public GitHub issue, pull request, or discussion**.

### Reporting Channels

1. **GitHub Private Vulnerability Reporting (Preferred)**:
   Submit a private advisory directly through GitHub:
   👉 **[Report a vulnerability privately](https://github.com/Conxian/conxian-gateway/security/advisories/new)**

2. **Security Contact Routing**:
   If Private Vulnerability Reporting is unavailable, send an encrypted notification to:
   - **Email**: `security@conxian.io`
   - **Governance Contacts**: `@botshelomokoka` / `@admin-conxian-labs`

### What to Include in Your Report

To help us triage and remediate the issue efficiently, please include:
- **Type of issue**: (e.g., authentication bypass, cryptographic defect, state manipulation, unexpected secret exposure, denial of service).
- **Affected component**: (e.g., `cmd/gateway`, `internal/api`, `internal/engine`, `internal/compliance`, `pkg/conxian-core`).
- **Steps to reproduce**: Minimal proof of concept (PoC) code or requests demonstrating the issue.
- **Potential impact**: Assessment of risk to node operators, institutional integration rails, or cross-chain state proofs.

---

## 3. Vulnerability Triage & Response SLAs

All reports are acknowledged and triaged according to severity:

| Severity | Initial Response SLA | Target Patch Window | Description |
| :--- | :--- | :--- | :--- |
| **Critical** | `< 24 hours` | `< 48 hours` | Remote code execution, unauthenticated admin bypass, or private key / key material exposure. |
| **High** | `< 48 hours` | `< 7 days` | Authentication bypass on non-admin endpoints, state corruption, or high-impact DoS. |
| **Medium** | `< 72 hours` | `< 14 days` | Authenticated policy bypass, missing validation under non-default features, or low-impact DoS. |
| **Low** | `< 7 days` | `< 30 days` | Informational security hygiene, minor hardening, or low-risk configuration edge cases. |

### Coordinated Disclosure Policy

- **Embargo**: We request that security researchers maintain strict confidentiality until a official patch is released and communicated.
- **Credit**: Valid reports receive public attribution in release notes and security advisories (unless anonymity is explicitly requested).

---

## 4. Secret & Credential Disclosure Protocol

Conxian Gateway strictly enforces zero secret exposure across all public and private branches.

### Accidental Exposure Incident Protocol
If a real production key, API token, certificate, or database secret is inadvertently committed or logged:
1. **Immediate Revocation**: The exposed secret must be revoked immediately at the issuer/provider level (do not wait for Git history purging).
2. **Containment**: Open a private advisory or notify maintainers immediately.
3. **Purge & History Cleanup**: The commit containing the secret must be removed or rewritten prior to merging to public branches.
4. **Mandatory Rotation**: A fresh secret must be provisioned; old compromised values must never be reused.

### Non-Production Sentinel Policy
- Standard environment templates (`.env.example`) and test fixtures use non-secret placeholders matching the `sentinel_<NAME>` convention (e.g., `sentinel_API_TOKEN`).
- Production runtime (`Config::from_env`) fails closed and panics at startup if any mandatory secret is set to empty or its `sentinel_*` placeholder.

---

## 5. Gateway Security Architecture & Threat Boundaries

Conxian Gateway operates as a non-custodial, high-assurance inter-ledger bridge. Security controls are built around the following boundaries:

- **Non-Custodial Isolation**: The gateway verifies cryptographic state proofs (BIP-340 Schnorr, Groth16 ZK, MuSig2, DLC attestations) without taking custody of private keys or funds.
- **Trust Policy Ingress Gate**: Ingress endpoints require structured trust metadata (`x-conxian-trust-metadata`) specifying IBC context, trust tier (`T1`), policy evidence, and freshness timestamps.
- **Sentinel Token Enforcement**: Admin and sensitive governance endpoints (`/admin/v1/*`) require valid Bearer token authentication and reject sentinel string defaults.
- **Persistence Single-Writer Boundary**: File-based persistence enforces exclusive single-writer locks (`EXCLUSIVE_LOCAL_WRITER_MODE`) and rejects shared network filesystems (NFS/SMB/CIFS) to prevent state file corruption.

---

## 6. Continuous Security Controls & CI Verification

Every pull request and commit is checked against automated security tooling:

- **Gitleaks Secret Scanning**: Scans every commit for secret patterns (`.gitleaks.toml` & `secret-scan.yml`).
- **Contamination Guard**: Scans production source paths (`cmd/`, `internal/`, `pkg/`, `apps/`, `packages/`) for prohibited stubs, placeholders, or weak defaults (`scripts/verify_contamination_guard.py`).
- **Tracked Artifact Guard**: Verifies zero environment files (`.env*` except `.env.example`), private keys (`*.key`, `*.pem`), cert stores (`*.pfx`, `*.p12`), secrets (`*.secret`), or database state files are tracked (`scripts/verify_tracked_artifacts.py`).
- **Dependency Vulnerability Scanning**: Audits Rust crate dependencies for known CVEs via `cargo-audit` (`cargo-audit.yml`).
- **Immutable Action Pinning**: Enforces exact SHA-256 commit hashes on all GitHub Action references (`scripts/verify_github_action_pins.py`).

---

🛡️ **Governance Disclaimer**: Conxian Gateway operates under Sovereign Autonomous Business (SAB) governance. Legal review is required before embedding specific SLA commitments into formal contractual agreements.
