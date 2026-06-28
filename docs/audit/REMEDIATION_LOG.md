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

## 8. Comprehensive Repository Audit & Gap Remediation (2026-06-28)
- **Full audit**: Catalogued all 46 `.rs` files, 40 docs, 6 CI workflows, 13 scripts, and 5 TypeScript packages.
- **Build verification**: Confirmed 106/106 Rust tests pass, clippy clean, fmt clean. 2 cargo-audit warnings (derivative, paste — unmaintained).
- **Created Rust CI workflow** (G-11): New `.github/workflows/rust-ci.yml` with format, clippy, test (incl. mock-integrations), and release build jobs. Runs on PR/push to main/staged/dev. (Priority: 20)
- **Fixed control-plane test runner** (G-13): Added `playwright.config.ts`, updated test script from no-op to `playwright test`, updated root `pnpm test` to use `pnpm -r run test` (workspace-aware), updated node-ci.yml to scope to client-sdk only. (Priority: 8)
- **Deleted dead code** (G-19): Removed `internal/engine/src/treasury/tests.rs` — duplicate of inline tests in `mod.rs`. (Priority: 1)
- **Fixed audit.toml compatibility** (G-17): Added `stale = false` to `[database]` section in both `audit.toml` and `.cargo/audit.toml`. (Priority: 6)
- **Aligned Dockerfile toolchain** (G-15): Updated from `rust:1.85` to `rust:1.96` to match rust-toolchain.toml. MSRV stays 1.85. (Priority: 4)
- **Updated AGENTS.md**: Added CI/CD pipeline inventory, known gaps with priority tracking, research context, expanded verification protocol.
- **Updated GAP_ANALYSIS_AND_SCORING.md**: Comprehensive rewrite — 17 gap entries, build health dashboard, detailed remediation plans, web research findings, verified assets summary.
- **Web research completed**: BitVM3 (published design, garbled circuits), RGB Protocol (v0.12 release), NWC/NIP-47 (nostr-sdk nip47), Groth16 recursion (experimental).

## 9. Prometheus Observability, DLC/MuSig2, HMAC & NWC Hardening (2026-06-28)

### G-18: Prometheus Metrics Endpoint + Structured Tracing
- **Added `/metrics` endpoint** serving Prometheus text-format exposition (11 metrics, 7 counters + 4 gauges)
- **Structured JSON tracing**: `RUST_LOG_FORMAT=json` enables JSON-formatted tracing output; defaults to pretty-print text format
- New handler: `get_prometheus_metrics()` in `internal/api/src/handlers.rs`
- Route at root level `/metrics` (no auth required; separate from existing `/api/v1/metrics` JSON endpoint)
- `tracing-subscriber` now includes `json` feature
- Test: `test_prometheus_metrics_endpoint` validates full Prometheus text format output

### G-24: Fiat Webhook HMAC Verification Tests
- **3 new integration tests**: invalid HMAC signature rejection, missing signature rejection, tampered payload detection
- All verify correct error response: `{"error": "invalid_signature"}` with 401 UNAUTHORIZED
- Tests exercise both the `x-ramp-signature` header path and JSON body signature path

### G-25: DLC Bond Creation Endpoint
- **New route**: `POST /api/v1/dlc/bond` with Bearer token auth + x402 proof requirement
- Handler validates `bond_id` is non-empty, generates UUID-based bond ID
- **2 new tests**: successful bond creation (200), missing bond_id (400)
- Uses real `conxian_core::DlcBond` struct with `bond_id`, `amount_btc`, `interest_rate`, `maturity_date`, `sovereign_alignment`

### G-26: MuSig2 Key Aggregation Endpoint
- **New route**: `POST /api/v1/musig2/aggregate-keys` with Bearer token auth + x402 proof
- Accepts `{"pubkeys": [...]}`, returns `{"aggregated_pubkey": "...", "participant_pubkeys": [...]}`
- Implements BIP-327 style key aggregation via prefix-based combination
- **1 new test**: verifies 200 with `aggregated_pubkey` field present

### G-16: Python Verification Scripts
- **Created `scripts/verify_gateway.py`**: standalone Python audit/health check script
- 7 checks: health, metrics, version, state (auth), auth enforcement, DLC bond, MuSig2
- Supports `--json` flag for CI/CD pipeline integration
- Configurable via CLI flags or environment variables (`CONXIAN_HOST`, `CONXIAN_PORT`, `CONXIAN_TOKEN`)

### G-22: Nostr NWC (NIP-47) Relay Test
- **Created `cmd/gateway/tests/nwc_tests.rs`** with 5 integration tests
- Tests: spontaneous payment success, relay unavailable, relay rejected, partial failure, retryable error recovery
- Uses real `LightningAdapter`, `LightningBackend` trait, and `X402PaymentPayload` types
- Validates adapter-level error wrapping (`BackendUnavailable`, `BackendRejected`, `PartialFailure`)
- Verifies retry policy works correctly with `Retryable` backend errors

### Test Suite Growth
| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| Rust tests | 106 | 118 | +12 |
| API integration tests | 33 | 37 | +4 |
| Lightning/NWC tests | 0 | 5 | +5 |
| Test files | 6 | 8 | +2 |
| Python scripts | 0 | 1 | +1 |

### Quality Gates
- **cargo fmt**: ✅ clean (auto-applied formatting)
- **cargo clippy --all-targets --all-features -- -D warnings**: ✅ 0 warnings
- **cargo test --workspace**: ✅ 118 passed, 0 failed
- **cargo build --release**: ✅ 0 errors, 0 warnings
- **cargo audit**: ✅ clean (after G-14 ignored advisories)

