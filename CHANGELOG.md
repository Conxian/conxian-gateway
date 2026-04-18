# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.1.1] - 2026-04-18

### Added
- Implemented `resolve_identity` and `exchange_token` handlers in the API layer.
- Integrated `DlcOrchestrator` into the global `AppState` and gateway configuration.
- Introduced `Network` enum for strict environment separation (Mainnet, Testnet, Simulated).
- Added `ORACLE_PUBKEY` mandatory configuration for DLC bond orchestration.

### Changed
- Hardened `SovereignCommit` implementation with simulated Tableland SQL insertion for settlements and job cards.
- Refactored `Config::from_env` to use network-specific defaults for RPC and API URLs.
- Updated `verify_contamination_guard.py` to support formal environment-based keywords.
- Removed identity resolution stubs in favor of functional simulations.

## [0.1.0] - 2026-04-10

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
- Sovereign/Institutional Realignment of the entire codebase for mainnet readiness.
- Hardened repository hygiene by removing legacy backup files and updating `.gitignore`.
- Refactored API layer to use dependency injection via `AppState`, removing hardcoded mocks.
- Hardened security boundaries and aligned system wallets with mainnet prefixes (`SP`).
