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
