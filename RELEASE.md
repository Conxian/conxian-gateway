# Release Runbook

## Versioning and tag format

- Follow Semantic Versioning: `MAJOR.MINOR.PATCH`.
- Create release tags as `vMAJOR.MINOR.PATCH` (example: `v0.2.0`).

## Changelog requirement

- Update `CHANGELOG.md` before tagging.
- Add an entry for the version being released with date and notable changes.

## Release steps

1. Ensure `main` is up to date:
   - `git checkout main`
   - `git pull --ff-only origin main`
2. Confirm `CHANGELOG.md` includes the release version.
3. Create an annotated tag:
   - `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
4. Push the tag:
   - `git push origin vX.Y.Z`
5. GitHub Actions will run `.github/workflows/release.yml` and create the GitHub release automatically.

## Verification checklist

- [ ] Tag `vX.Y.Z` exists on GitHub.
- [ ] `Release` workflow run succeeded for the tag.
- [ ] A GitHub release was created for `vX.Y.Z`.
- [ ] The release body includes a link to `CHANGELOG.md`.
- [ ] The changelog entry for `vX.Y.Z` is present and accurate.

## Control sign-offs (required before tagging)

- [ ] **API lifecycle owner sign-off**: confirms versioning/deprecation policy impact is documented.
- [ ] **Security sign-off**: confirms security-sensitive changes and incident follow-ups are addressed.
- [ ] **Compliance/policy boundary sign-off**: confirms policy-engine boundary remains unchanged or is explicitly approved.
- [ ] **Observability sign-off**: confirms health/metrics endpoints and security-event logging expectations are met.
- [ ] **Authority boundary check**: confirms release does not introduce custody behavior and does not redefine protocol source-of-truth.
