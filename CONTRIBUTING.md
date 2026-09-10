# Contributing to Conxian Gateway

Thank you for your interest in contributing to Conxian! We welcome contributions that align with our mission of bridging Bitcoin/Stacks with institutional compliance and the Unified Vault SDK Pivot.

## Development Principles
- **Rust First**: All core logic is implemented in Rust (edition 2021).
- **Institutional Quality**: Code must be high-performance, asynchronous, and well-documented.
- **Sovereignty**: Prioritize non-custodial and sovereign alignment in all features.
- **Testing**: New features must include unit and/or integration tests.

## Getting Started
1. Fork the repository and clone it to your local machine.
2. Install the latest stable Rust toolchain: `rustup update stable`.
3. Install Node.js (v20+) and PNPM: `npm install -g pnpm`.
4. Install and build workspace dependencies cleanly using frozen lockfiles:
   ```bash
   pnpm install --frozen-lockfile
   pnpm build
   ```
5. Install Playwright browser dependencies:
   ```bash
   pnpm exec playwright install --with-deps chromium
   ```
6. Build and run tests across the workspace:
   - To build the Rust gateway: `cargo build`
   - To run Rust tests: `cargo test --workspace`
   - To run TypeScript tests (must set `NEXTAUTH_SECRET` for control plane smoke tests):
     ```bash
     NEXTAUTH_SECRET=sentinel_nextauth_secret pnpm test
     ```
7. Verify quality-gating and hygiene checks pass before submitting:
   ```bash
   python3 scripts/verify_contamination_guard.py
   python3 scripts/verify_tracked_artifacts.py
   ```

## Submission Process
1. Create a new branch for your changes: `git checkout -b my-feature`.
2. Ensure Rust code passes quality checks:
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   - `cargo fmt --all -- --check`
   - `cargo test --workspace`
3. Ensure TypeScript code passes quality checks:
   - `pnpm lint`
   - `pnpm test`
4. Commit your changes with a descriptive message.
6. Submit a Pull Request (PR) with a clear description of the problem solved or feature added.

## Coding Standards
- Use structured tracing for logging.
- Expose metrics for new features if applicable.
- Avoid hardcoding secrets; use environment variables via the `Config` struct in `cmd/gateway/src/config.rs`.
- All public API endpoints must be documented in the `README.md`.

## Code of Conduct
We are committed to a welcoming and inclusive community. Please be respectful and professional in all interactions.

## Mainnet Readiness & Branch Policy
For institutional safety, this repository enforces a strict branch promotion policy (`main`, `staged`, `dev`). Please refer to the **[Governance & Mainnet Readiness](README.md#governance--mainnet-readiness)** section in the README for the authoritative policy.

## Governance-Sensitive Changes

If your PR changes governance or security-control files, complete the PR security checklist and request CODEOWNERS review before merge.

Sensitive files include:

- `CODEOWNERS`
- `SECURITY.md`
- `SUPPORT.md`
- `.github/ISSUE_TEMPLATE/**`
- `.github/PULL_REQUEST_TEMPLATE*`
- `.github/workflows/**`
- `.github/release.yml`

## Security and dependency hygiene

- Never commit `.env*` files, private keys (`*.key`, `*.pem`), certificate stores (`*.pfx`, `*.p12`), secrets (`*.secret`), or database state files (`gateway_state.json`, `offline_queue.db`, `*.db`, `*.sqlite`).
- Use `.env.example` only as a non-secret template (the only allowlisted `.env*` file in Git).
- Run `python3 scripts/verify_tracked_artifacts.py` and `python3 tests/test_verify_tracked_artifacts.py` locally to verify zero prohibited files or secrets are tracked.
- Pull requests and protected branches are scanned with `gitleaks` and checked in CI via `rust-ci.yml`.
- Dependency changes are reviewed through dependency review and Dependabot updates.

## Support and Security Routing

- For support and governance-routing guidance, refer to [SUPPORT.md](SUPPORT.md).
- For private vulnerability reporting requirements, refer to [SECURITY.md](SECURITY.md).
