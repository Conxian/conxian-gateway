# Release Runbook (v1.9.2 Standards)

This document outlines the release process for the Conxian Gateway, ensuring alignment with institutional standards and the Unified Vault SDK Pivot.

## 1. Versioning and Tag Format

- Follow Semantic Versioning: `MAJOR.MINOR.PATCH`.
- Create release tags as `vMAJOR.MINOR.PATCH` (example: `v1.9.2`).

## 2. Release Branches & Promotion

Release promotion follows the authoritative **[Governance & Mainnet Readiness](README.md#governance--mainnet-readiness)** policy.

- **`main`**: Target for production releases.
- **`staged`**: Mandatory validation path for all production code.
- **`dev`**: Integration and testnet validation.

## 3. Pre-Release Checklist

1. **Changelog**: Update `CHANGELOG.md` with notable changes and the release date.
2. **Security Scan**: Run `cargo audit` and `cargo deny` to verify dependencies.
3. **Hygiene Check**: Run `python3 scripts/verify_contamination_guard.py` to ensure no non-production keywords exist in source paths.
4. **Integration Tests**: Verify all workspace tests pass: `cargo test --workspace`.
5. **Metrics Check**: Confirm `/metrics` and `/health` endpoints are functional.

## 4. Release Steps

1. **Update Version**: Ensure `Cargo.toml` reflect the target version.
2. **Commit & Push**: Commit changes to `staged` and push for final validation.
3. **Tagging**:
   - `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
   - `git push origin vX.Y.Z`
4. **Promotion**: Merge `staged` into `main` after successful tag verification.
5. **GitHub Release**: GitHub Actions will automatically generate a draft release. Review and publish.

## 5. Control Sign-offs (Mandatory)

- [ ] **Security sign-off**: Confirms security-sensitive changes and incident follow-ups are addressed.
- [ ] **Treasury sign-off**: Verifies that chain-sync health and treasury metrics are accurate.
- [ ] **Authority boundary check**: Confirms release does not introduce custody behavior and does not redefine protocol source-of-truth.
- [ ] **Contamination Guard**: Verified clean by automated script.

## 6. Verification checklist

- [ ] Tag `vX.Y.Z` exists on GitHub.
- [ ] CI workflow succeeded for the tag.
- [ ] A GitHub release was created with an accurate changelog link.
