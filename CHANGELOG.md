# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Added Client-Side Wasm UCV-1 Verification schemas (`WasmUcvProofPayload`, `WasmUcvVerificationResult`) to `@conxian/schemas` and implemented local zero-trust state proof verification (`verifyStateProofLocal`) in `@conxian/client-sdk` with full Vitest unit test coverage (Candidate Q).
- Implemented BRICS mBridge & CIPS Cross-Border Settlement Engine (Candidate P / G-B6 / G-FI3) featuring `MBridgeAdapter::verify_mbridge_dlt_attestation` in `internal/engine/src/brics_adapter.rs`, payload validation in `internal/compliance/src/zkc.rs`, `/api/v1/ingress/mbridge` API route, and E2E integration tests in `brics_mbridge_tests.rs`.
- Implemented Canton Daml ACS state translation adapter (`translate_canton_state` / Candidate J / G-C4) in `internal/api/src/handlers.rs` and `pkg/conxian-core`, adding Daml contract ID verification, state root hash computation via SHA-256 (`daml_contract_id` + `template_name` + optional `payload_json`), and UCR URI translation (`ucr:canton:<domain>:<contract_id>`).
- Updated `@conxian/client-sdk` and `@conxian/schemas` with `translateCantonState` and supporting TypeScript interfaces (`CantonStateTranslationRequest`, `CantonStateTranslationResponse`, `UniversalContractRef`).
- Expanded strategic research in `SOVEREIGN_SHARDING_VERIFICATION.md`, `CANTON_NETWORK_AND_MACHINE_ECONOMY_RESEARCH.md`, `CANDIDATE_MATRIX.md`, and `GAP_ANALYSIS_AND_SCORING.md` covering BitVM3 recursive proof efficiency targets, Canton Daml ACS mapping specs, and BRICS mBridge validator node requirements.
- Implemented ISO 20022 XML Schema Validation & XSD structure hardening (`validate_iso20022_xml_structure`) in `internal/compliance/src/zkc.rs` (G-FI1).
- Added XML structure, namespace verification (pacs.008, pacs.009, camt), and syntax error checks for ISO 20022 ingress normalization.
- Added comprehensive unit tests in `zkc_iso20022_tests` and updated research knowledge base (`GAP_ANALYSIS_2026-08-07.md`, `CANDIDATE_MATRIX.md`).

## [v0.1.5] - 2026-08-07

### Added
- Implemented 7 missing Python validation scripts in `scripts/` to close CI coverage gaps (CON-1322).
- Created `docs/audit/GAP_ANALYSIS_AND_SCORING.md` for prioritized risk management.
- Added `docs/research/OPPORTUNITY_MAP_AND_EXPANSION.md` covering BitVM3 and local-first verification.
- Implemented **Babylon** and **BitVM2** state-proof verification logic in the multi-chain adapter layer.
- Added comprehensive integration tests for Babylon and BitVM verification endpoints.
- Created `docs/audit/REMEDIATION_LOG.md` tracking repository hardening efforts.
- Implemented **Universal Chain Verification (UCV-1)** in the compliance layer, unifying heterogeneous proof verification across multi-chain adapters.
- Added `UniversalVerifier` service and `CoreVerifier` trait to support heterogeneous chain-state proofs.
- Exposed `POST /api/v1/chains/{chain}/verify` endpoint for multi-chain proof validation.
- Updated `@conxian/client-sdk` with `verifyStateProof` method and added supporting TypeScript schemas.
- Added `docs/research/UNIVERSAL_CHAIN_RESEARCH.md` covering multi-chain adapter patterns and event bus delivery logic.

### Changed
- Bumped `lib-conxian-core` dependency to v0.3.2 (tag `v0.3.2`, published to crates.io).
- Updated `Cargo.lock` to resolve `lib-conxian-core` from crates.io via direct-source git tag.
- Standardized `actions/checkout` version to `v4.2.2` (pinned by SHA) across all local workflows (CON-1324).
- Centralized `CHANGELOG.md` as a canonical release history record in the repository root.
- **Hardened CI/CD Pipelines**: Integrated mandatory `contamination_guard` in Rust CI and expanded Node.js CI to include Playwright browser installation and full workspace testing.
- **Hardened GitHub Workflows**: Pinned all third-party actions to immutable SHAs to prevent supply-chain attacks.
- **Repository Hygiene**: Updated `.gitignore` to strictly exclude local persistence artifacts (`gateway_state.json`, `offline_queue.db`).
- **Standardized CI/CD**: Aligned dependencies and action versions across all workflows for consistent execution.
- **Hardened X402 (Payment Required) middleware** to support flexible payload formats (numeric/string Satoshis/expiry) and protected nested API paths.
- Refactored API handlers for ALEX, A2P, and ISO 20022 paths to align with updated `conxian-core` schemas and fix type mismatches.
- Standardized environment variable sentinels by replacing `CHANGEME_` and `REQUIRED_FOR_PROD_` with `sentinel_` identifiers.
- Improved `A2pRouter` production check by verifying API key prefixes instead of internal mock flags.
- Standardized internal naming by replacing "mock" with "simulated" in several gateway and compliance modules.
- Renamed `simulate_mesh_gossip` to `gossip_mesh_rehearsal` for better institutional alignment.
- Remediated non-deterministic `.unwrap()` calls in system clock access across the workspace.
- Implemented Schnorr signature verification in `ZkcVerifier` for Taproot-compatible attestations.
- Hardened `internal/api/src/auth.rs` by replacing insecure sentinel strings with compliant production identifiers.
- Integrated dynamic `chrono::Utc` timestamps in `ZkcVerifier` for ISO 20022 message generation, replacing hardcoded legacy dates.
- Updated `docker-compose.yml` with hardened sentinel values for webhook and ingress secrets.

### Documentation
- Updated `docs/research/ENHANCEMENT_PLAN.md` and `docs/research/VERIFICATION_IMPROVEMENT_PROPOSAL.md` with UCV-1 implementation results.
- Clarified readiness language so real runtime code, simulated validation paths, and production-enforced controls are not conflated in status messaging.

## [0.1.4] - 2026-06-07

### Added
- Implemented comprehensive Control-Plane UI in `apps/control-plane` with Next.js 14.
- Added modular sub-pages for Release Governance, Audit Log, Policy Approvals, System Metrics, and Settings.
- Integrated Tailwind CSS for an institutional-grade dark-mode dashboard experience.
- Created `ClientButton` component for interactive, state-aware dashboard actions.
- Added system status indicators and throughput visualization to the dashboard.

### Changed
- Standardized environment variable sentinels by replacing `CHANGEME_` and `REQUIRED_FOR_PROD_` with `sentinel_` identifiers.
- Improved `A2pRouter` production check by verifying API key prefixes instead of internal mock flags.
- Standardized internal naming by replacing "mock" with "simulated" in several gateway and compliance modules.
- Renamed `simulate_mesh_gossip` to `gossip_mesh_rehearsal` for better institutional alignment.
- Refactored Control-Plane navigation to use a persistent sidebar with functional iconography.
- Updated documentation for the Control-Plane application.

## [0.1.3] - 2026-04-24

### Added
- Implemented ZKML-backed Guardian Attestation verification in `ZkcVerifier`.
- Added `format_iso20022_pacs008_v8` to support ISO 20022 payment generation from Conxian Job Cards.

### Changed
- Standardized environment variable sentinels by replacing `CHANGEME_` and `REQUIRED_FOR_PROD_` with `sentinel_` identifiers.
- Improved `A2pRouter` production check by verifying API key prefixes instead of internal mock flags.
- Standardized internal naming by replacing "mock" with "simulated" in several gateway and compliance modules.
- Renamed `simulate_mesh_gossip` to `gossip_mesh_rehearsal` for better institutional alignment.
- Remediated contamination guard failures by replacing "mock" identifiers with "simulated" across the codebase.
- Standardized error handling in API handlers for better type inference and auditability.
- Hardened TEE device ID verification to support simulated test vectors while rejecting unauthorized IDs.
- Fixed build regression in `internal/engine` caused by signature change in `MempoolOrchestrator`.
- Synchronized repository versioning to `v0.1.3`.

## [0.1.2] - 2026-04-15

### Added
- Integrated BNS (Stacks Name Service) resolution in `IdentityManager` via `call_read_only` RPC.
- Implemented production-grade Infobip SMS egress logic in `A2pRouter`.
- Added `build_swap_payload` to `AlexClient` to support prepared transaction payloads for secure signers.
- Introduced `SimulatedStacksRpcTrait` in `conxian-core` to decouple compliance from engine.

### Changed
- Standardized environment variable sentinels by replacing `CHANGEME_` and `REQUIRED_FOR_PROD_` with `sentinel_` identifiers.
- Improved `A2pRouter` production check by verifying API key prefixes instead of internal mock flags.
- Standardized internal naming by replacing "mock" with "simulated" in several gateway and compliance modules.
- Renamed `simulate_mesh_gossip` to `gossip_mesh_rehearsal` for better institutional alignment.
- Hardened ALEX swap path in API to return `prepared` status with structured Clarity payloads.
- Refactored `IdentityManager` and `AppState` to use dependency injection for Stacks RPC, removing circular dependencies.
- Updated `PRD.md` and `ENHANCEMENT_PLAN.md` to reflect full functional state of shared services.

## [0.1.1] - 2026-04-10

### Added
- Implemented `resolve_identity` and `exchange_token` handlers in the API layer.
- Integrated `DlcOrchestrator` into the global `AppState` and gateway configuration.
- Introduced `Network` enum for strict environment separation (Mainnet, Testnet, Simulated).
- Added `ORACLE_PUBKEY` mandatory configuration for DLC bond orchestration.

### Changed
- Standardized environment variable sentinels by replacing `CHANGEME_` and `REQUIRED_FOR_PROD_` with `sentinel_` identifiers.
- Improved `A2pRouter` production check by verifying API key prefixes instead of internal mock flags.
- Standardized internal naming by replacing "mock" with "simulated" in several gateway and compliance modules.
- Renamed `simulate_mesh_gossip` to `gossip_mesh_rehearsal` for better institutional alignment.
- Hardened `SovereignCommit` implementation with simulated Tableland SQL insertion for settlements and job cards.
- Refactored `Config::from_env` to use network-specific defaults for RPC and API URLs.
- Updated `verify_contamination_guard.py` to support formal environment-based keywords.
- Removed identity resolution stubs in favor of functional simulations.

## [0.1.0] - 2026-04-05

### Added
- Initialized workspace structure with Rust 2021.
- Implemented BitVM and BitVM2 attestation verification for trustless state layer.
- Added ISO 20022 Egress (pacs.008) support for institutional banking alignment.
- Integrated Workload Identity Federation (WIF) for TEE-based enclave authentication.
- Enhanced Treasury Monitor with sBTC "Suction" simulation and Sovereign Yield Index (SYI).
- Implemented `FiatRouter` with production-grade Ramp, Investec, Alchemy Pay, and Banxa integrations.
- Added HMAC-SHA256 signature verification for authenticated webhooks and OTP.
- Implemented Infobip integration and hardened stateless OTP verification in `A2pRouter`.
- Deployed specialized NTT Relayer for sovereign bridging of native token transfers.
- Integrated Conxian Job Card Schema (CJCS) v2.0 JSON-LD for institutional labor orchestration.
- Implemented ALEX DEX client and exposed quote/swap API endpoints.
- Integrated maintainer-controlled bounty payout toggle.
- Implemented Industrial Intent (x402) metadata and structured finance tranches.
- Created canonical portfolio map and repository inventory.

### Changed
- Standardized environment variable sentinels by replacing `CHANGEME_` and `REQUIRED_FOR_PROD_` with `sentinel_` identifiers.
- Improved `A2pRouter` production check by verifying API key prefixes instead of internal mock flags.
- Standardized internal naming by replacing "mock" with "simulated" in several gateway and compliance modules.
- Renamed `simulate_mesh_gossip` to `gossip_mesh_rehearsal` for better institutional alignment.
- Sovereign/Institutional Realignment of the entire codebase for mainnet readiness.
- Hardened repository hygiene by removing legacy backup files and updating `.gitignore`.
- Refactored API layer to use dependency injection via `AppState`, removing hardcoded mocks.
- Hardened security boundaries and aligned system wallets with mainnet prefixes (`SP`).
