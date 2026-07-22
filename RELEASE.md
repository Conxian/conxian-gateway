# Conxian Gateway Release Runbook

This runbook describes the tag-driven release flow for the Gateway binary. The
workflow is deliberately fail-closed: GitHub Release publication is downstream
of exact-commit repository baselines, metadata validation, the production build,
checksum/SBOM verification, and GitHub artifact attestation. The release
workflow reruns repository-controlled gates directly on the exact tag commit;
it does not trust potentially stale check-run results from another event.

The workflow currently publishes one supported release target:
`x86_64-unknown-linux-gnu`.

## 1. Preconditions and sign-offs

Before creating a release tag:

1. Merge the intended change to the release branch after the repository's CI,
   security, and dependency checks pass. The workflow cannot configure or prove
   GitHub branch-protection/ruleset requirements; repository administrators must
   make those checks required separately.
2. Update the workspace version in `Cargo.toml` and every release-facing
   changelog/status surface. All workspace packages must carry the same
   `MAJOR.MINOR.PATCH` version as the tag.
3. Add release notes to `CHANGELOG.md` and confirm the file is present on the
   tagged commit.
4. Run the local verification suite from the repository root:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace
   cargo test --workspace --features mock-integrations
   cargo audit # cargo-audit 0.22.2
   ./scripts/lightning_coverage_gate.sh 90 # cargo-llvm-cov 0.8.7
   python3 -m unittest discover -s tests -p 'test_verify_release_artifacts.py'
   pnpm install
   pnpm build
   pnpm test
   python3 scripts/verify_contamination_guard.py
   ```

5. Confirm the protected `release` environment has the required reviewers and
   deployment restrictions. Do not treat the environment name alone as proof
   that protection is configured.
6. If crates.io publication is intended, configure `CARGO_REGISTRY_TOKEN` only
   as a secret in the protected `release` environment. Never place it in a
   repository, tag message, workflow input, or untrusted pull-request context.

## 2. Create the release tag

Create an annotated tag on the exact commit that passed preflight, then push the
tag:

```bash
VERSION=0.1.5
git checkout main
git pull --ff-only origin main
git tag -a "v${VERSION}" -m "Release v${VERSION}"
git push origin "v${VERSION}"
```

The tag must match `vMAJOR.MINOR.PATCH`. A manual `workflow_dispatch` run must
also select that tag and repeat the exact version in `release_version`; a branch
dispatch is rejected before any release job can create or publish a release.

## 3. Automated workflow order

`.github/workflows/release.yml` runs these jobs in order. All baseline jobs have
only `contents: read`; no environment secret is available before validation and
packaging succeed.

### `release-identity`

This job checks out the tag with full history, verifies the `vMAJOR.MINOR.PATCH`
format and manual version confirmation, checks that the tag resolves to the
workflow's exact `GITHUB_SHA`, fetches `origin/main`, and rejects the release
unless the tag commit is reachable from that reviewed main history. A tag on an
arbitrary unreviewed commit is therefore not an allowed release source.

### Exact-commit baseline jobs

The following jobs each check out the commit emitted by `release-identity`, not
the moving branch name and not a prior check-run result:

- `baseline-rust` runs formatting, workspace Clippy with warnings denied, both
  workspace test modes, the contamination guard, and the release-artifact
  verifier regression suite;
- `baseline-node` mirrors the Node workflow's compiler split, typecheck, lint,
  build, Playwright installation, and tests;
- `baseline-cargo-audit` installs and asserts `cargo-audit 0.22.2` with
  `--locked` before running `cargo audit`;
- `baseline-secret-scan` verifies the pinned Gitleaks `8.30.1` archive against
  the pinned SHA-256 of the official release checksum manifest and the archive
  digest recorded in that manifest before scanning; and
- `baseline-lightning` installs and asserts `cargo-llvm-cov 0.8.7` with
  `--locked` before enforcing the 90% scoped Lightning gate.

`package-release` requires all five baseline jobs and `release-identity`.
Attestation and publication cannot bypass any of these dependencies.

### `package-release`

This job has only `contents: read` and:

- verifies workspace package versions and `CHANGELOG.md` after all baseline
  jobs have passed;
- builds the actual `gateway` production binary with the repository's pinned
  Rust toolchain for `x86_64-unknown-linux-gnu`;
- packages the binary and deterministic metadata as
  `conxian-gateway-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz`;
- generates a pinned CycloneDX 1.5 SBOM from the Gateway workspace dependency
  graph and normalizes timestamps/workspace paths;
- writes `conxian-gateway-X.Y.Z.sha256` for the archive and SBOM; and
- verifies the archive, ELF target, full commit metadata binding, checksum
  manifest, SBOM, tar member types/paths, and exact artifact-directory shape
  with `scripts/verify_release_artifacts.py` before uploading an immutable
  Actions artifact set. The artifact directory is recreated cleanly before
  packaging.

No GitHub Release action runs in this job.

### `attest-release`

This job downloads the exact artifact set into a clean directory and reruns the
same verifier against the expected full commit. `actions/attest` then creates SLSA build provenance for the shipped
archive, checksum manifest, and CycloneDX SBOM—not merely Cargo metadata. The
resulting signed attestation bundle is retained as a release asset and the
workflow summary receives the GitHub attestation URL.

The job has only the permissions required for attestation:
`contents: read`, `id-token: write`, `attestations: write`, and
`artifact-metadata: write`.

### `create-release`

This job requires every baseline, packaging, and attestation job. It runs in the protected
`release` environment, downloads the exact immutable artifacts, rechecks their
checksums and SBOM identity, then publishes one GitHub Release with:

```text
conxian-gateway-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz
conxian-gateway-X.Y.Z.sha256
conxian-gateway-X.Y.Z.cdx.json
conxian-gateway-X.Y.Z.provenance.json
```

The publication job checks the exact commit and reruns the artifact verifier
before invoking the release action. The release action refuses unmatched files
and does not overwrite existing assets. A baseline, validation, attestation, or
environment failure therefore prevents release publication rather than allowing
an ad hoc rebuild.

## 4. Verify a published release

Download the archive, SBOM, checksum manifest, and provenance bundle from the
release page, then run:

```bash
VERSION=0.1.5
TARGET=x86_64-unknown-linux-gnu
mkdir -p "verify-v${VERSION}"
# Place the four downloaded assets in verify-v${VERSION}/.

(cd "verify-v${VERSION}" && \
  sha256sum -c "conxian-gateway-${VERSION}.sha256")

EXPECTED_COMMIT="$(git rev-list -n 1 "v${VERSION}")"
python3 scripts/verify_release_artifacts.py \
  --directory "verify-v${VERSION}" \
  --version "${VERSION}" \
  --target "${TARGET}" \
  --expected-commit "${EXPECTED_COMMIT}"

jq -e \
  --arg version "${VERSION}" \
  '.bomFormat == "CycloneDX" and .specVersion == "1.5" and .metadata.component.name == "gateway" and .metadata.component.version == $version' \
  "verify-v${VERSION}/conxian-gateway-${VERSION}.cdx.json" > /dev/null

gh attestation verify \
  "verify-v${VERSION}/conxian-gateway-${VERSION}-${TARGET}.tar.gz" \
  --repo Conxian/conxian-gateway \
  --signer-workflow Conxian/conxian-gateway/.github/workflows/release.yml
```

The checksum manifest covers the archive and SBOM. The provenance bundle is a
signed Sigstore/GitHub attestation result and is intentionally not included in
that manifest, because the bundle is created after the manifest's subject
digests are fixed. The SLSA attestation itself covers the archive, checksum
manifest, and SBOM. The verifier does not extract the archive; it validates
member paths and types before reading only the expected regular files.

## 5. Optional crates.io publication gate

Crates.io publication is never performed on a tag push. To request it, manually
dispatch the workflow on the same tag with `publish_to_crates_io=true`.

The `publish-crates-io` job waits for the verified GitHub Release, runs the
Cargo dry-run before reading any secret, requires the protected `release`
environment, checks `CARGO_REGISTRY_TOKEN`, and only then runs:

```bash
cargo publish --locked --package gateway
```

The current workspace still uses path-only internal dependencies for the
Gateway package, so Cargo's publish dry-run is expected to fail until those
crates have publishable version requirements and package metadata. This is a
real crates.io packaging prerequisite, not a reason to bypass the environment
gate or expose the token. The GitHub binary release does not depend on the
optional crates.io job.

## 6. Rollback, yank, and recovery

### Validation or attestation failure

- Do not create a release manually and do not rebuild assets by hand.
- Fix the tagged commit or workflow issue, create a new patch tag, and rerun the
  workflow. A failed validation job has no path to `create-release`.
- If only the workflow infrastructure changed and the tag commit is still the
  intended source, rerun the workflow from that tag after review.

### Partial or draft GitHub Release

Inspect the release and assets before retrying:

```bash
gh release view vX.Y.Z --repo Conxian/conxian-gateway
```

Because the workflow uses `overwrite_files: false`, do not blindly rerun a job
against a release that already contains assets. For an unused draft/partial
release, an authorized maintainer may either remove the incomplete release and
rerun the tag workflow, or upload only the missing files after independently
rechecking their checksums and attestation. If the release is already public,
prefer a new patch release instead of replacing a shipped asset or moving the
tag.

### Bad published binary

1. Stop promotion and crates.io publication immediately.
2. Preserve the release page, checksum, provenance, and incident evidence.
3. Publish a corrected patch release with a new immutable tag. Do not move or
   reuse the original tag.
4. If the package was published to crates.io, yank the affected version rather
   than deleting it, then publish the replacement after the same protected gate:

   ```bash
   cargo yank --vers X.Y.Z --token "$CARGO_REGISTRY_TOKEN"
   ```

5. Remove a GitHub Release/tag only when an authorized maintainer confirms that
   it has not been consumed and repository policy permits that cleanup. Deleting
   a public release is not a substitute for a corrected replacement release.

## 7. External and admin-only controls

This repository change does not claim to have configured:

- required status checks or branch protection/rulesets for `main`;
- required reviewers, tag restrictions, or other protection on the `release`
  environment;
- the crates.io package publication order, path-dependency version metadata, or
  `CARGO_REGISTRY_TOKEN`; or
- a successful live release run with assets and an attestation.

The external CodeQL, GitGuardian, and reusable dependency-review checks are not
substitutes for the direct release baseline; they remain external evidence and
must be evaluated separately. These items require repository/organization
administration or a controlled live release rehearsal. The workflow now makes
the owned baseline, build, artifact identity, checksum, SBOM, attestation, and
publication ordering explicit and fail-closed. Pin refresh sources and
procedure are recorded in [`docs/CI_TOOLING_PINS.md`](docs/CI_TOOLING_PINS.md).
