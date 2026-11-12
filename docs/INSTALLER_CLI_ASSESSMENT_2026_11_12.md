# Installer CLI — client-lifecycle and standards assessment

**Date:** 2026-11-12 · **Subject:** `cmd/conxian-cli` (the Conxian unified installer
CLI) · **Method:** ran the built binary against every command, compared its
conventions to docker / kubectl / gh, and mapped it to the client lifecycle in
`conxian-business/docs/CLIENT_ONBOARDING_AND_UNIFIED_INSTALLER_SPEC.md`.

## 1. Client lifecycle coverage

| Lifecycle stage | Client need | CLI today | Gap |
|---|---|---|---|
| **Setup** | generate config + compose | `init` ✅ | no interactive prompts (spec §5 wants them) |
| **Preflight** | validate inputs, reachability | `doctor`, `doctor --probe` ✅ | DB/Redis/enclave/webhook checks absent (spec §5) |
| **Start** | run the stack | `start` prints instructions only | does not launch (by design; documented) |
| **Provision** | create a tenant | `onboard`, `deploy` ✅ | — |
| **Monitor** | health/status | `status` ✅ | no `logs` |
| **Meter / entitle** | usage, limits | `usage`, `limits` ✅ | — |
| **Billing** | quote/pay/verify/balance | `quote`, `subscribe`, `verify`, `balance` ✅ | — |
| **Keys** | rotate/revoke | `keys rotate\|revoke` ✅ | — |
| **Configure** | persistent profile | `config get\|set\|show` ✅ | — |
| **Maintenance** | DB migrations | ❌ | spec §5 wants `migrate` |
| **Repair** | recover / reset | ❌ | no `reset`/`recover` |
| **Upgrade** | update the CLI/stack | ❌ | no `self-update`/`upgrade` |
| **Uninstall** | remove generated files | `uninstall [--force]` ✅ (this change) | — |

**Summary:** setup, provision, monitor, billing, keys and **uninstall** are covered.
The **maintenance / repair / upgrade** stages are not — and the spec explicitly asks
for database migrations and broader diagnostics.

## 2. Standards comparison (vs docker / kubectl / gh)

| Convention | docker/kubectl/gh | conxian-cli before | after this change |
|---|---|---|---|
| `--help` overview | ✅ | ✅ | ✅ |
| **per-subcommand `cmd --help`** | ✅ | ❌ (showed global usage) | ✅ |
| `help <topic>` | ✅ | ❌ | ✅ |
| **unknown subcommand → non-zero exit** | ✅ | ❌ (exit 0) | ✅ |
| `--version` / `-V` | ✅ | ✅ | ✅ |
| machine-readable output (`--json`/`-o`) | ✅ | ✅ (`--json`) | ✅ |
| shell completion | ✅ | ❌ | open |
| dry-run | ✅ | ❌ | open |
| exit codes 0/1 | ✅ | ✅ | ✅ |

## 3. Fixed in this change

1. **Unknown subcommand returned exit 0.** It printed the usage and succeeded —
   so a typo (`conxian-cli onbaord`) looked like success to a script. Now exits 1.
2. **No per-subcommand help.** `doctor --help` printed the whole global usage.
   Added `print_command_help()`; `conxian-cli <cmd> --help` and `help <cmd>` now
   show that command's flags and behaviour.
3. **`help` was not a command** — it fell through to the generic usage and ignored
   its argument. Now routes to per-command help.
4. Added **`uninstall [--force]`** (reverses `init`; removes only generated
   templates, never a user-authored file, and never the data volume/DB).
5. Added **`version`** as an explicit subcommand (in addition to `--version`).

## 4. Remaining gaps (owner/roadmap)

- **Maintenance:** a `migrate` command to apply Gateway/Nexus DB schema migrations
  (spec §5). This touches a live database, so it needs its own design + review.
- **Repair/upgrade:** `reset`/`recover` and `upgrade`/`self-update`.
- **Diagnostics breadth:** `doctor` covers RPC syntax+reachability; the spec also
  wants Database, Redis, enclave attestation and webhook checks.
- **Shell completion** (`completion bash|zsh|fish`) and **`--dry-run`**.
- **Interactive `init`** (spec §5 "interactively prompts for client inputs").

## 5. Verification (this change)

Rust 1.99.1: `cargo test -p conxian-cli` → **25 passed** (15 unit + 10 integration);
`cargo fmt --check` clean; `cargo clippy -D warnings` clean. Binary exercised:
`--help`, `<cmd> --help`, `help <topic>`, `version`, `doctor[ --json][ --probe]`,
`init`, `uninstall [--force]` (removes generated, preserves user files), `config`,
`status`, unknown subcommand (now non-zero).
