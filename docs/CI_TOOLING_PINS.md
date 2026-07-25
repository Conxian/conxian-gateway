# CI Tooling Pins

**Verified:** 2026-07-25

The repository-owned release baseline installs the same immutable tool versions
used by the scheduled security and coverage workflows. The release workflow
does not query prior check runs: each baseline job checks out the exact release
commit and runs its tool directly.

## Pinned tools

| Tool | Version | Installation/verification | Canonical source |
|---|---|---|---|
| Rust toolchain | `1.96.0` | `dtolnay/rust-toolchain` action pinned to `e97e2d8cc328f1b50210efc529dca0028893a2d9` | [`rust-toolchain.toml`](../rust-toolchain.toml) |
| `cargo-audit` | `0.22.2` | `cargo install cargo-audit --version 0.22.2 --locked --force`, followed by an exact version check | [`rustsec/rustsec` release `cargo-audit/v0.22.2`](https://github.com/rustsec/rustsec/releases/tag/cargo-audit/v0.22.2) |
| `cargo-llvm-cov` | `0.8.7` | `cargo install cargo-llvm-cov --version 0.8.7 --locked --force`, followed by an exact version check | [`taiki-e/cargo-llvm-cov` release `v0.8.7`](https://github.com/taiki-e/cargo-llvm-cov/releases/tag/v0.8.7) |
| Gitleaks archive | `8.30.1` | Linux x86_64 archive and the official checksum manifest are downloaded over TLS; both the manifest digest and archive digest are checked before execution | [`gitleaks/gitleaks` release `v8.30.1`](https://github.com/gitleaks/gitleaks/releases/tag/v8.30.1) |

The pinned Gitleaks digests are:

```text
checksums manifest: 061476c21adaf5441516f96f185c1a4706a83cd6329b9b38762271b3d4a52fae
gitleaks linux x86_64: 551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb
```

The manifest is the authoritative upstream release record for the archive.
Pinning the manifest's own digest prevents a mutable or substituted manifest
from changing the expected archive digest. The release does not publish a
separate signature file for this archive, so CI uses the official upstream
release record plus the two-step SHA-256 verification; this workflow does not
independently verify a release signature.

Both scans pass `--config .gitleaks.toml` explicitly. That checked-in
configuration retains only the repository's existing narrow test-fixture
exceptions; generated build directories are absent from the fresh CI checkout
and are not allowlisted. The synthetic Groth16 fixture exception is restricted
to its exact path and deterministic public identifier fields.

## GitHub Action pins

Repository workflows use full commit pins, with the selected upstream tag
recorded in a nearby comment for reviewability:

| Action | Commit | Upstream tag |
|---|---|---|
| `actions/checkout` | `3d3c42e5aac5ba805825da76410c181273ba90b1` | `v7.0.1` |
| `actions/setup-node` | `820762786026740c76f36085b0efc47a31fe5020` | `v7.0.0` |
| `pnpm/action-setup` | `0ebf47130e4866e96fce0953f49152a61190b271` | `v6.0.9` |
| `actions/cache` | `55cc8345863c7cc4c66a329aec7e433d2d1c52a9` | `v6.1.0` |
| `taiki-e/install-action` | `c44f6b046f1c29ae5918b1e0bfdbb2f1813836fd` | `v2.84.1` |
| `actions/upload-artifact` | `043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` | `v7.0.1` |
| `actions/download-artifact` | `3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c` | `v8.0.1` |
| `actions/attest` | `f7c74d28b9d84cb8768d0b8ca14a4bac6ef463e6` | `v4.2.0` |
| `softprops/action-gh-release` | `3d0d9888cb7fd7b750713d6e236d1fcb99157228` | `v3.0.2` |
| `tj-actions/branch-names` | `dde14ac574a8b9b1cedc59a1cf312788af43d8d8` | `v8` |
| `neondatabase/create-branch-action` | `fb620d43d4c565abaf088b848a4e28e5c4ea4d9c` | `v6` |
| `neondatabase/delete-branch-action` | `4468d825d5a88ef4012f1705a82f02ec3072f776` | `v3` |

The release-baseline action commits were resolved from their upstream Git tag
refs on 2026-07-22. The three Neon workflow tags were resolved on 2026-07-25;
all three were lightweight tags pointing directly to the commits shown above.
A future refresh must re-resolve the tag, verify the commit and the action's
documented inputs/outputs, then update every workflow that shares the pin
rather than changing only one workflow.

The Neon workflow declares `permissions: {}`. The pinned branch-name action
reads the pull-request event context, and the pinned create action uses that
same event context only as annotations on requests authenticated directly to
the Neon API. The delete action also authenticates directly to Neon. None of
the three actions checks out repository contents or calls the GitHub API, so no
`GITHUB_TOKEN` repository or pull-request permission is required.

## Refresh procedure

1. Select a specific upstream release from the canonical project release page.
2. Resolve its tag to a full commit with `gh api` and record the tag/commit pair.
3. For Gitleaks, download the official `*_checksums.txt`, calculate its SHA-256,
   calculate the platform archive SHA-256, and update both constants in
   `.github/workflows/secret-scan.yml` and `.github/workflows/release.yml`.
4. For Cargo tools, verify the selected version builds with Rust `1.96.0` and
   keep `--locked` plus the exact post-install version assertion.
5. Run `actionlint`, the verifier regression tests, the targeted security
   workflows' shell/static checks, and the full repository verification suite.
6. Record the refresh date and source evidence in this file and in the review
   description. Do not replace a full commit pin with a moving tag or an
   unversioned `cargo install` command.

## Workflow action pin policy

`scripts/verify_github_action_pins.py` scans every `.yml` and `.yaml` file
under `.github/workflows/`. Remote GitHub Actions and reusable workflows must
use a full 40-character commit SHA. Local actions referenced with `./...` are
allowed. Any `docker://` action must use a full `sha256` image digest; image
tags and other mutable references fail closed.

The guard and its regression tests run in the always-on Rust CI format job.
Quoted and unquoted `uses:` values and trailing inline comments are supported.
Any exception requires changing the checked-in verifier and its tests rather
than bypassing the policy in an individual workflow.
