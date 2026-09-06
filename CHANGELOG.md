# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Implemented SWIFT `camt.053` OData v4 ERP Webhook Callback Synchronization (Candidate T / G-TR1) in `internal/api/src/camt.rs`, providing `ODataV4CallbackPayload` serialization, `dispatch_odata_v4_webhook` HTTP callback delivery using `minreq` inside `spawn_blocking`, and full test coverage.
- Comprehensive audit and research expansion across all gaps, candidates (Candidates Q, R, S, T), and knowledge base documents (`docs/research/CANDIDATE_MATRIX.md`, `docs/research/KNOWLEDGE_MAP.md`, `docs/research/OPPORTUNITY_MAP_AND_EXPANSION.md`, and `docs/research/GAP_ANALYSIS_2026-09-06.md`).
- Added Machine Economy & DePIN peaq DLT TypeScript interfaces (`MachineIdentityPayload`, `MachineRwaAttestation`, `DePinSettlementRequest`, `DePinSettlementResponse`) and SWIFT ISO 20022 `camt.053` Bank Treasury Reporting interfaces (`Camt053StatementRequest`, `Camt053StatementResponse`) to `@conxian/schemas` (Candidates R & T).
- Expanded `@conxian/client-sdk` (`ConxianClient`) with typed API methods (`resolveMachineIdentity`, `verifyMachineRwaAttestation`, `settleDePinMachinePayment`, `generateCamt053Statement`) and added unit test coverage in `index.test.ts`.
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
