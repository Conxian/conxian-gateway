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
3. Build the project: `cargo build`.
4. Run tests: `cargo test`.

## Submission Process
1. Create a new branch for your changes: `git checkout -b my-feature`.
2. Ensure your code passes linting: `cargo clippy --all-targets --all-features -- -D warnings`.
3. Ensure your code is formatted: `cargo fmt --all -- --check`.
4. Verify all tests pass: `cargo test`.
5. Commit your changes with a descriptive message.
6. Submit a Pull Request (PR) with a clear description of the problem solved or feature added.

## Coding Standards
- Use structured tracing for logging.
- Expose metrics for new features if applicable.
- Avoid hardcoding secrets; use environment variables via the `Config` struct in `cmd/gateway/src/config.rs`.
- All public API endpoints must be documented in the `README.md`.

## Code of Conduct
We are committed to a welcoming and inclusive community. Please be respectful and professional in all interactions.

## Mainnet Readiness & Branch Policy
- **main**: Strictly Mainnet-only production code. No stubs, mocks, or placeholders.
- **staged**: Mainnet production validation. All promotion to `main` must pass through `staged` with full mainnet-acceptance evidence.
- **dev**: Testnet-only logic and non-production validation.

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

- Never commit `.env*` files, private keys, or API tokens.
- Use `.env.example` only as a non-secret template.
- Pull requests and protected branches are scanned with `gitleaks`.
- Dependency changes are reviewed through dependency review and Dependabot updates.

## Support and Security Routing

- For support and governance-routing guidance, refer to [SUPPORT.md](SUPPORT.md).
- For private vulnerability reporting requirements, refer to [SECURITY.md](SECURITY.md).
