# Contributing to Conxian Gateway

Thank you for your interest in contributing to Conxian! We welcome contributions that align with our mission of bridging Bitcoin/Stacks with institutional compliance.

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
