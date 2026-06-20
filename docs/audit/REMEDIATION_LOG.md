# Repository Hardening Remediation Log (CON-1251 / CON-1245)

## 1. Action Pinning (Security Hardening)
- All GitHub Actions in `.github/workflows/` have been pinned to immutable SHAs to prevent supply-chain attacks via tag floating.
- Verified actions include: `actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `taiki-e/install-action`, `actions/upload-artifact`, and `softprops/action-gh-release`.

## 2. Artifact and Hygiene (Repository Hygiene)
- `.gitignore` hardened to ensure `offline_queue.db` and `gateway_state.json` are never tracked.
- Confirmed `node_modules`, `target`, and `.next` are correctly ignored.

## 3. Sentinel and Placeholder Sanitization
- Verified that all remaining `sentinel_` strings are documented and enforced via the `Config` loader in `cmd/gateway/src/config.rs`.
- `A2pRouter` and `AuthStore` correctly reject these sentinels in production environments.

## 4. Documentation Alignment
- `README.md` aligned with mandatory Purpose, Status, and Audience sections.
- `AGENTS.md` consolidated to root directory for unified agent guidance.
