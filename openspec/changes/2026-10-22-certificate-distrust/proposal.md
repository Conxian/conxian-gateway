# Restore certificate distrust protections

## Scope and owner

Gateway #466 and Business #1390 track this ordinary `dev` security repair.
Immutable release candidate #576 remains unchanged and unqualified.

## Evidence and choice (22 October 2026)

RustSec RUSTSEC-2026-0382 (20 October) identifies acceptance of distrusted
emSign intermediate public keys in rustls-webpki. RUSTSEC-2026-0381 and
RUSTSEC-2026-0383 (19 October) identify affected bundled trust roots.

- https://github.com/RustSec/advisory-db/blob/main/crates/rustls-webpki/RUSTSEC-2026-0382.md
- https://github.com/RustSec/advisory-db/blob/main/crates/webpki-roots/RUSTSEC-2026-0381.md
- https://github.com/RustSec/advisory-db/blob/main/crates/webpki-root-certs/RUSTSEC-2026-0383.md
- Upstream repairs: https://github.com/rustls/webpki/pull/546 and https://github.com/rustls/webpki-roots/pull/133

Choose the first patched stable releases: rustls-webpki 0.103.16 and both root
packages 1.0.10. Retaining vulnerable versions or adding audit exceptions leaves
the trust defect unresolved. A TLS stack migration or prerelease increases
compatibility risk and maintenance cost without an evidenced need. These patch
updates add no dependency edges or resource commitment; existing pins, features,
licenses, audit policy and release controls are retained.

Only the three lockfile versions and corresponding registry checksums change.
Each actual crate archive was downloaded and its SHA256 compared with the
crates.io version metadata. All other 720 package records and dependency edges
are preserved.

## Acceptance

Validate the locked dependency graph, upstream distrust regression, normal
workspace tests and strict all-target/all-feature Clippy. Retain exact results
and environment limitations in the PR; do not infer a pass from planned checks.
Hosted security/native/integration checks and independent review remain gates.
After qualified dev integration generate a fresh immutable candidate; do not
transfer #576's old-tree container evidence to the changed tree. Normal soak,
artifact/provenance and operation-specific production acceptance remain required.
No provider, custody, settlement or live-runtime acceptance is supplied here.
