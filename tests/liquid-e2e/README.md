# Local Liquid peg E2E harness

This directory contains the isolated Bitcoin + Elements regtest harness for
issue #218. It is deliberately separate from the repository's application
`docker-compose.yml` and has no dependency on Liquid testnet, public Bitcoin
RPCs, or public Liquid infrastructure.

The harness uses pinned multi-architecture images:

- `bitcoin/bitcoin:31.1`
- `blockstream/elementsd:23.3.3`

Both images are pinned by digest in
[`docker-compose.yml`](./docker-compose.yml). The daemons run on regtest with
P2P listening disabled, and an internal-only Docker network. No host ports are
published; all RPC calls use `docker compose exec` inside the Compose network.

## Prerequisites

- Docker Engine with Docker Compose v2
- `jq`
- Bash 4+

Docker is required because the harness validates real daemon behavior. No
public Liquid service is contacted.

To validate amounts and other configuration without Docker:

```bash
PEGIN_AMOUNT=1 PEGOUT_AMOUNT=0.25 ./scripts/liquid-e2e.sh --validate-config
```

`PEGIN_AMOUNT` and `PEGOUT_AMOUNT` must use canonical non-negative decimal
text with no exponent notation, sign, surrounding whitespace, or more than
eight fractional digits. Both amounts must be greater than zero, and the
peg-out amount must be smaller than the peg-in amount.

## One-command local run

From the repository root:

```bash
./scripts/liquid-e2e.sh
```

The script validates its prerequisites, starts Bitcoin before Elements, polls
both RPCs with a bounded timeout, creates fresh wallets, runs the peg-in and
representative peg-out flows, writes fixtures/logs, and removes its named
Compose volumes in an exit trap.

The default run uses `PEGIN_CONFIRMATION_DEPTH=10`. The value is passed to
Elements at chain initialization and must be an integer from `2` through
`1000`; the lower bound keeps the intentionally immature claim test valid.
The successful claim waits for `PEGIN_CONFIRMATION_DEPTH + 2` parent-chain
confirmations. For example:

```bash
PEGIN_CONFIRMATION_DEPTH=3 OUTPUT_DIR=target/liquid-e2e-3 \
  ./scripts/liquid-e2e.sh
```

Other useful configuration variables are `PEGIN_AMOUNT`, `PEGOUT_AMOUNT`,
`RPC_TIMEOUT_SECONDS`, `OUTPUT_DIR`, `COMPOSE_PROJECT_NAME`, and
`COMPOSE_FILE`. The default Compose project name includes a timestamp and
process ID, so default runs get fresh named volumes. If a project name is
provided explicitly, it is treated as owned by this harness and is cleaned up
by the script.

## Generated artifacts

Artifacts are written under `target/liquid-e2e/` by default and are not
tracked. On both success and failure the harness preserves as much as is
available before tearing down the daemons:

| Path | Description |
| --- | --- |
| `pegin.json` | Reusable peg-in proof, raw transaction, claim script, claim transaction, confirmation counts, and balance assertions. |
| `pegout.json` | Reusable representative peg-out request, decoded nulldata metadata, destination script, and confirmation result. |
| `bitcoin-chain-info.json` | Bitcoin `getblockchaininfo` response. |
| `elements-chain-info.json` | Elements `getblockchaininfo` response. |
| `elements-sidechain-info.json` | Elements `getsidechaininfo` response, including effective peg-in depth and parent genesis. |
| `early-claim-rejection.txt` | Daemon error returned by the intentionally immature `claimpegin` attempt. |
| `bitcoin.log`, `elements.log`, `compose.log` | Captured daemon/Compose logs. |
| `compose-ps.txt`, `cleanup.log` | Container service state and cleanup output (without full process command lines). |

### `pegin.json` schema

The top-level object contains `schema_version`, `network`, and
`configuration`. `peg_in` contains:

- `mainchain_address` and `claim_script` from `getpeginaddress`;
- `bitcoin_txid`, `bitcoin_block_hash`, `bitcoin_raw_transaction`, and
  `bitcoin_txout_proof` for adapter/fixture consumers;
- `bitcoin_confirmations` and
  `configuration.required_parent_confirmations`;
- `early_claim_rejected`, `early_claim_expected_error`,
  `early_claim_observed_error`, `claim_txid`, `claim_confirmations`, and the
  decoded `claim_transaction`;
- `sidechain_balance_before` and `sidechain_balance_after`.

### `pegout.json` schema

The top-level object contains `schema_version` and `network`. `peg_out`
contains the Bitcoin destination address and script, the Elements transaction
ID/raw transaction, decoded `nulldata` metadata, and the confirmed Elements
transaction. The decoded metadata is checked for:

- `pegout_chain` equal to the Bitcoin regtest genesis hash;
- `pegout_address` or `pegout_hex` matching the generated Bitcoin destination;
- decoded `scriptPubKey.type` equal to `nulldata`;
- a non-empty `pegout_type`.

## Determinism and fixture reuse

The protocol path is deterministic and isolated, but raw proof bytes and
transaction IDs are expected to vary between fresh runs. New wallets generate
new keys, transaction fees/change outputs depend on the wallet state, and
regtest block timestamps/nonces are produced by the daemon. Consumers should
use the schema and semantic fields rather than compare full transaction bytes
or IDs as constants. A generated `pegin.json` is still reusable as a complete
fixture for a test run that consumes its own recorded raw transaction/proof
pair.

## Peg-out limitation

The two-node harness has no Watchmen/functionary set. It therefore cannot
perform or claim a real Bitcoin-side release. The peg-out test stops at a
confirmed Elements `sendtomainchain` transaction and verifies that its decoded
metadata names the Bitcoin destination and parent genesis. `pegout.json`
explicitly records `watchmen_release_observed: false`.

This is representative workflow coverage, not a production Watchmen release
test and not a substitute for Conxian's production proof-verification
boundary.
