# Conxian Gateway

### API token configuration migration

New installer output uses `API_TOKEN`. Existing `CONXIAN_GATEWAY_AUTH_TOKEN`
configurations remain accepted when `API_TOKEN` is absent. If both are set,
`API_TOKEN` takes precedence; an explicitly empty or invalid canonical value
fails instead of falling back. Empty, sentinel and installer-placeholder tokens
are not configured secrets. Set real values only through the approved secret
boundary; the local startup preflight reports names/status, never token values.
Other startup requirements and deployment/release gates remain applicable.

Current [capability discovery](docs/CAPABILITY_DISCOVERY.md) reports process registrations with separate health/evidence status. It does not infer live support from SDK inventory counts. See [architecture](ARCHITECTURE.md) for public protocol and managed-service ownership.

The client SDK also provides an [offline ERP journal preview](docs/ERP_JOURNAL_PREVIEW.md) with exact debit/credit balancing and explicit source/accounting units. Its output remains unverified and grants no ERP posting authority.

## Public protocol and managed service boundary

Gateway owns tokenless routing and financial-message translation. The reusable routing schemas and bounded ISO 20022 translation profile are public protocol components associated with `conxian.org`. Conxian-Labs operates the commercial `conxian-labs.com` tier through authenticated tenant APIs, distributed quotas, compliance integrations, durable audit export, deployment operations, and contracted support.

Conxian has no native token, staking asset, or speculative tokenomics. Gateway may observe or route assets issued by external networks, but those adapters do not create Conxian protocol assets. Bitcoin L1 is the verification root for promoted production claims.

The current managed-auth middleware provides strict JWT/API-key validation, a single-process rate limiter, and structured audit hooks. Multi-replica commercial service remains gated on a shared atomic rate-limit backend and durable append-only audit sink.

Institutional-grade middleware bridging Bitcoin and Stacks state logic with enterprise compliance.

[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/Status-v0.1.5-orange.svg)](#status)

## Purpose
The Conxian Gateway provides a single API layer for indexing, verifying, and orchestrating settlement workflows across multiple Bitcoin layers. It simplifies the integration path for institutions and fintechs by providing mathematically verifiable state proofs and non-custodial signing interfaces.

## Status
**Runtime implementation and deployment qualification are separate.** This repository contains Bitcoin and Stacks state-tracking runtime code (v0.1.5). Hosted deployment, provider connectivity and operation-specific production acceptance require current receipts under [#466](https://github.com/Conxian/conxian-gateway/issues/466); source, CI and a historical secret inventory do not establish those states. See [runtime initialization](docs/RUNTIME_INITIALIZATION.md) for the current minimum lane and its remaining deployment gates.

<!-- STARTUP-SECRET-CONTRACT:BEGIN -->
### Startup secret requirements

The minimum runtime lane requires **3 logical startup secrets**:
`API_TOKEN`, `SETTLEMENT_INGRESS_SECRET`, `OFFLINE_QUEUE_SECRET`.

| Enabled provider mode | Additional startup credential |
| --- | --- |
| `RAMP_MODE` | `RAMP_API_KEY` |
| `ALCHEMY_PAY_MODE` | `ALCHEMY_PAY_APP_ID` |
| `INFOBIP_MODE` | `INFOBIP_API_KEY` |
| `RAMP_MODE` or `STITCH_MODE` or `OZOW_MODE` or `PAPSS_MODE` or `ALCHEMY_PAY_MODE` or `BANXA_MODE` | `FIAT_WEBHOOK_SECRET` |
| `INFOBIP_MODE` | `HMAC_SECRET` |

A provider credential is required only for mode values `1`, `active`, `shadow`, `true`.
The legacy token alias is one logical requirement, not an extra secret.
Optional managed-auth/provider credentials and deployment, persistence, network,
signer and release acceptance remain separate operation-specific requirements.

Describe this source contract without inspecting the environment:
`python3 scripts/startup_secret_preflight.py --describe`.
Check injected secret presence only inside the approved operator boundary:
`python3 scripts/startup_secret_preflight.py`.
Neither command proves provisioning, secret strength or runtime readiness.
<!-- STARTUP-SECRET-CONTRACT:END -->

## Audience
- **Institutions & Fintechs**: Seeking a secure, non-custodial entry point into Bitcoin-native liquidity.
- **Infrastructure Engineers**: Deploying and operating Conxian nodes in sovereign or enterprise environments.
- **Integrators**: Building wallets, dapps, or settlement systems on the Conxian stack.

## Workflow & Consumption
1. **Discovery**: Review the [PRD.md](PRD.md) and [ADAPTER_FAMILY_STRATEGY.md](docs/research/ADAPTER_FAMILY_STRATEGY.md).
2. **Pilot**: Run the [developer sandbox](examples/developer-sandbox/README.md), which uses the workspace [`@conxian/client-sdk`](packages/client-sdk) for the narrow health, supported-chain, and BitVM rehearsal path.
3. **Sovereign Installation**: Use the unified installer CLI `conxian-cli` (`cmd/conxian-cli`) to generate configuration, perform environment preflight diagnostic checks, and manage service lifecycles.
4. **Production**: Deploy the gateway behind your own institutional security perimeter using `conxian-cli start` or the provided [Docker Compose](docker-compose.yml) baseline.
5. **Expansion**: Add custom chain adapters or compliance rules using the [`ChainAdapter`](pkg/conxian-core/src/lib.rs) trait.

Before production deployment, review the supported single-writer topology,
crash recovery, backup, mempool reconciliation, and shutdown procedures in
[`docs/PERSISTENCE_TOPOLOGY.md`](docs/PERSISTENCE_TOPOLOGY.md) as well as the
[Client Installation & Deployment Blueprint](docs/research/CLIENT_INSTALLATION_AND_DEPLOYMENT_BLUEPRINT.md).

## Core Capabilities
- **Universal Verification (UCV-1)**: Unified interface for heterogeneous proofs (BitVM, ZKC, TEE).
- **Institutional Egress**: ISO 20022 (pacs.008) banking-standard messaging.
- **Mempool Orchestration**: Automated RBF/CPFP fee-bumping for high-priority settlements.
- **Identity Bridge**: Unified resolution for BNS, ENS, and World ID.

## Sovereign Multi-Rail Installer (`conxian-cli`)

The workspace includes a sovereign CLI installer (`cmd/conxian-cli`) to streamline client onboarding, environment diagnostics, and multi-rail lifecycle orchestration.

### Key Subcommands
- **`conxian-cli init`**: Writes the default `.env.production` and `docker-compose.yml` installer templates. They require operator completion against the current startup/persistence contract before use.
- **`conxian-cli doctor`**: Inspects default installer token configuration, tier and RPC URL syntax. It does not inspect injected production configuration, measure secret strength or test network/port connectivity.
- **`conxian-cli start`**: Prints startup instructions; it does not launch a process or qualify a deployment.
- **`conxian-cli status`**: Probes Gateway `/api/v1/health` for HTTP liveness only. It does not probe `/metrics`, RPC connectivity, provider qualification or deployment readiness.

For full architectural blueprints, see [`docs/research/CLIENT_INSTALLATION_AND_DEPLOYMENT_BLUEPRINT.md`](docs/research/CLIENT_INSTALLATION_AND_DEPLOYMENT_BLUEPRINT.md).

## Architecture
- `/apps/control-plane`: Next.js management dashboard.
- `/cmd/gateway`: Core Axum Gateway server runtime binary.
- `/cmd/conxian-cli`: Sovereign multi-rail deployment CLI installer.
- `/internal/engine`: High-performance chain listeners and RPC clients.
- `/internal/api`: Axum-based REST interface and X402 payment middleware.
- `/internal/compliance`: Zero-Knowledge Compliance (ZKC) and attestation logic.
- `/packages`: TypeScript client SDK (`@conxian/client-sdk`) and shared domain schemas (`@conxian/schemas`).
- `/pkg/conxian-core`: Shared Rust primitives and resilience models.

## Development

Follow these steps to set up, build, and test the entire multi-language workspace.

### 1. Rust Gateway & CLI Setup
The core Gateway services and sovereign CLI installer are implemented in asynchronous Rust.
```bash
# Build production release binaries (gateway and conxian-cli)
cargo build --release

# Run conxian-cli diagnostic doctor
cargo run -p conxian-cli -- doctor

# Run entire Rust workspace test suite
cargo test --workspace
```

### 2. TypeScript SDK & Dashboard Setup
The monorepo contains multiple TypeScript packages/applications managed via `pnpm` workspaces (e.g., `@conxian/schemas`, `@conxian/client-sdk`, and the `control-plane` dashboard).

To ensure deterministic dependency resolution in CI/CD and local environments, always use `--frozen-lockfile`:
```bash
# Install workspace dependencies cleanly
pnpm install --frozen-lockfile

# Build TypeScript workspaces in proper topological order
pnpm build
```

### 3. Running Workspace Tests
The full test suite includes Playwright browser testing for the Next.js control-plane dashboard.

Before executing tests, you must download the required headless browser binaries:
```bash
# Install Playwright browser engines and system dependencies
pnpm exec playwright install --with-deps chromium
```

To run all TypeScript workspace tests (including the dashboard smoke tests), you must define the `NEXTAUTH_SECRET` environment variable to initialize NextAuth:
```bash
# Run all vitest and playwright tests across the workspace
NEXTAUTH_SECRET=sentinel_nextauth_secret pnpm test
```

### 4. Running Python Quality Checks
We maintain several quality-gating and audit scripts in the `scripts/` directory to prevent stubs, unpinned actions, or accidental leak of generated artifacts:
```bash
# Run the strict contamination guard (scans Rust and TypeScript production sources in cmd/, internal/, pkg/, apps/, and packages/ for stubs/placeholders)
python3 scripts/verify_contamination_guard.py

# Check for accidentally tracked runtime artifacts or python caches
python3 scripts/verify_tracked_artifacts.py
```

## Governance & Mainnet Readiness

The Conxian Gateway enforces a strict branch promotion policy to ensure institutional-grade stability and mainnet safety.

- **`main`**: Strictly **Mainnet-only** production code. No stubs, simulations, or placeholders. All code in this branch is audited for production execution.
- **`staged`**: Mainnet production validation. All promotion to `main` must pass through `staged` with full mainnet-acceptance evidence.
- **`dev`**: Integration branch for new features and testnet-only logic. This branch may contain functional simulations for non-production validation.

For a detailed map of repository readiness, refer to [docs/READINESS_GATES.md](docs/READINESS_GATES.md).

## Policy Registry

As an institutional middleware provider, we maintain a comprehensive set of governance and security policies:

- **[LICENSE](LICENSE)**: MIT License.
- **[SECURITY.md](SECURITY.md)**: Vulnerability reporting and incident handling procedures.
- **[CONTRIBUTING.md](CONTRIBUTING.md)**: Technical standards and submission process.
- **[RELEASE.md](RELEASE.md)**: Release runbook and control sign-off checklist.
- **[SUPPORT.md](SUPPORT.md)**: Support channels and governance routing.
- **[SLA Disclaimer](README_SLA_SNIPPET.md)**: Org-wide SLA policy (No SLA on public repos; paid SLAs strictly under B2B commercial contract).
- **[PRIVACY.md](PRIVACY.md)**: Data handling and PII pass-through policy.
- **[TERMS.md](TERMS.md)**: Usage terms for the Conxian Gateway.
- **[CHANGELOG.md](CHANGELOG.md)**: Canonical release history.
- **[Repository Baseline Review Template](.github/ISSUE_TEMPLATE/repo_baseline_improvement.yml)**: Issue template for repository baseline reviews, priority-based quality assessments, and high-value repo improvements.

## Contact
- Support: [support@conxian-labs.com](mailto:support@conxian-labs.com)
- Security: [security@conxian-labs.com](mailto:security@conxian-labs.com)
