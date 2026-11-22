# Installer CLI — client-lifecycle and standards assessment

**Date:** 2026-11-12 · **Subject:** `cmd/conxian-cli` (the Conxian unified installer
CLI) · **Method:** ran the built binary against every command, compared its
conventions to docker / kubectl / gh, and mapped it to the client lifecycle in
`conxian-business/docs/CLIENT_ONBOARDING_AND_UNIFIED_INSTALLER_SPEC.md`.

## 1. Client lifecycle coverage

| Lifecycle stage | Client need | CLI today | Gap |
|---|---|---|---|
| **Setup** | generate config + compose | `init` ✅ | no interactive prompts (spec §5 wants them) |
| **Preflight** | validate inputs, reachability | `doctor` ✅ (reads `.env.production`), `doctor --probe` ✅ | DB/Redis/enclave/webhook checks absent (spec §5) |
| **Start** | run the stack | `start` ✅ (launches `docker compose up`, `--dry-run` prints the command) | — |
| **Provision** | create a tenant | `onboard`, `deploy` ✅ | — |
| **Monitor** | health/status | `status` ✅ (with `--json`), `logs` ✅ (streams container logs) | — |
| **Meter / entitle** | usage, limits | `usage`, `limits` ✅ | — |
| **Billing** | quote/pay/verify/balance | `quote`, `subscribe`, `verify`, `balance` ✅ | — |
| **Keys** | rotate/revoke | `keys rotate\|revoke` ✅ | — |
| **Configure** | persistent profile | `config get\|set\|show` ✅ | — |
| **Maintenance** | DB migrations | `migrate` ✅ | — |
| **Repair** | recover / reset | ❌ | no `reset`/`recover` |
| **Upgrade** | update the CLI/stack | ❌ | no `self-update`/`upgrade` |
| **Uninstall** | remove generated files | `uninstall [--force]` ✅ | — |

**Summary:** setup, preflight, **start (real launch)**, provision, monitor, billing, keys,
maintenance and uninstall are covered. Repair/upgrade and broader diagnostics (DB/Redis/enclave/
webhook) remain open.

## 1a. Why preflight was inert (fixed)

`doctor` previously validated `InstallerConfig::default()` — a compile-time template whose
token is the literal `CONXIAN_PROD_AUTH_TOKEN_REQUIRED`. It never read the `.env.production`
that `init` writes, so a freshly initialised, fully completed install could never pass its own
preflight, and the failure said nothing about which keys were actually missing. `doctor` now
loads the install file (or `--env-file`), reports problems by **key only** (never values), and
passes once the placeholders are replaced. `start` was the same shape: it printed instructions.
It now runs `docker compose up` and fails closed with the missing keys when the install is not
ready or Docker is unavailable.


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

## 3a. Fixed in the AI-native pass (2026-11-22)

The earlier pass hardened conventions (help, exit codes, uninstall) but left the CLI
*checking* rather than *doing*. This pass closes the do-vs-check gap on the install lane:

1. **`doctor` validates the real install, not a compile-time default.** It loads
   `.env.production` (or `--env-file`), detects placeholder/missing secret keys and
   invalid RPC URLs, and reports them **by key only** (values never printed). A filled
   install now passes its own preflight — previously impossible.
2. **`start` launches the stack** (`docker compose --env-file … -f … up [-d]`) instead of
   printing instructions. It fails closed, naming the missing keys, when the install is
   not ready or Docker is unavailable. `--dry-run` prints the exact command only;
   `--foreground` runs attached.
3. **`status --json`** emits one machine-readable object (agent-consumable) instead of a
   prose line, and still never reads/echoes the health response body.
4. **`logs [--follow] [-n <lines>]`** streams the stack's container logs via
   `docker compose logs` — the monitor stage had `status` but no way to see output.
5. **`completion bash|zsh|fish`** emits a shell completion script.

## 4. Remaining gaps (owner/roadmap)

- **Repair/upgrade:** `reset`/`recover` and `upgrade`/`self-update`.
- **Diagnostics breadth:** `doctor` covers install keys + RPC syntax/reachability; the
  spec also wants Database, Redis, enclave attestation and webhook checks.
- **Interactive `init`** (spec §5 "interactively prompts for client inputs"). `init`
  is currently non-interactive and writes placeholders the client must fill.

## 5. Verification (this change)

Rust 1.100.0: `cargo test -p conxian-cli` → **43 passed** (20 unit + 23 integration);
`cargo fmt --check` clean; `cargo clippy --all-targets -D warnings` clean. Binary
exercised against a live Docker daemon: `init` → fill → `doctor` (fails on template,
passes when filled) → `start --dry-run` → `start` (actually created and started a
container) → `status --json` against a live 200 health server → `completion bash|zsh|fish`.
The seven new integration tests fail against the previous `main.rs` and pass against this
one (regression-verified).
