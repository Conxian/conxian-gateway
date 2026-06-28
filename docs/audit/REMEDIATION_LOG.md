# Repository Hardening Remediation Log (CON-1251 / CON-1245)

## 1. Action Pinning (Security Hardening)
- All GitHub Actions in `.github/workflows/` have been pinned to immutable SHAs to prevent supply-chain attacks via tag floating.
- Verified actions include: `actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `taiki-e/install-action`, `actions/upload-artifact`, and `softprops/action-gh-release`.

## 2. Artifact and Hygiene (Repository Hygiene)
- `.gitignore` hardened to ensure `offline_queue.db` and `gateway_state.json` are never tracked.
- Confirmed `node_modules`, `target`, and `.next` are correctly ignored.

## 3. Sentinel and Placeholder Sanitization
- Verified that all remaining `sentinel_` strings are documented and enforced via the `Config` loader in `cmd/gateway/src/config.rs`.
- `A2pRouter` and `AuthStore` correctly reject these sentinels in production environments.

## 4. Documentation Alignment
- `README.md` aligned with mandatory Purpose, Status, and Audience sections.
- `AGENTS.md` consolidated to root directory for unified agent guidance.

## 5. Fail-Closed Admin Hardening (CON-1279)
- Secured all `/admin/v1` routes with `auth_middleware` to ensure authenticated decision making.
- Hardened `sentinel_API_TOKEN` rejection in the authentication layer to prevent misconfiguration leaks.
- Replaced misleading "partial" BitVM attestation status with an explicit `action_required` error in `handlers.rs`, enforcing context-aware verification.

## 6. UCV-1 and Multi-Chain Alignment (CON-810 / CON-789)
- Updated `packages/schemas` and `internal/api` to support dynamic trust-tier metadata in chain discovery.
- Aligned Liquid and Rootstock adapters with the Pilot Lane (Tier 2) research patterns, including Elements-based UTXO and Powpeg anchor verification.
- Documented Phase 7 Sovereign Labor and Sharding Verification (SSV-1) for future BitVM2-backed labor proofs.

## 7. Unified CI Validation and Standardization (2026-06-28)
- Implemented 7 missing Python validation scripts in `scripts/` to close coverage gaps in the unified CI workflow (CON-1322).
- Standardized `actions/checkout` version to `v4.2.2` (pinned by SHA) across all local workflows to ensure consistent checkout behavior (CON-1324).
- Created `docs/audit/GAP_ANALYSIS_AND_SCORING.md` to track and prioritize future hardening work.
- Expanded research in `docs/research/OPPORTUNITY_MAP_AND_EXPANSION.md` covering BitVM3 and local-first verification.
- Initialized `docs/governance/CHANGELOG.md` as a canonical record for release history.
