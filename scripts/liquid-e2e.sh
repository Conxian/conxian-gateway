#!/usr/bin/env bash

set -Eeuo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/liquid-e2e.sh

Run the local Bitcoin + Elements regtest peg-in/peg-out harness.

Configuration is supplied through environment variables:
  PEGIN_CONFIRMATION_DEPTH  Elements confirmation depth (default: 10; minimum: 2)
  PEGIN_AMOUNT               Bitcoin amount to peg in (default: 1)
  PEGOUT_AMOUNT              Representative peg-out amount (default: 0.25)
  OUTPUT_DIR                 Fixture/log directory (default: target/liquid-e2e)
  COMPOSE_PROJECT_NAME       Compose project name (default: unique per run)
  COMPOSE_FILE               Compose file override

The harness requires Docker, Docker Compose v2, and jq. It uses only local
regtest daemons and does not connect to public Bitcoin or Liquid services.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if (( $# > 0 )); then
  echo "Unexpected argument: $1" >&2
  usage >&2
  exit 2
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
COMPOSE_FILE="${COMPOSE_FILE:-$REPO_ROOT/tests/liquid-e2e/docker-compose.yml}"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/target/liquid-e2e}"
PEGIN_CONFIRMATION_DEPTH="${PEGIN_CONFIRMATION_DEPTH:-10}"
PEGIN_AMOUNT="${PEGIN_AMOUNT:-1}"
PEGOUT_AMOUNT="${PEGOUT_AMOUNT:-0.25}"
RPC_TIMEOUT_SECONDS="${RPC_TIMEOUT_SECONDS:-90}"
RUN_ID="${LIQUID_E2E_RUN_ID:-$(date -u +%Y%m%d%H%M%S)-$$}"
PROJECT_NAME="${COMPOSE_PROJECT_NAME:-liquid-e2e-$RUN_ID}"

BITCOIN_GENESIS="0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
FEDPEG_SCRIPT="512103dff4923d778550cc13ce0d887d737553b4b58f4e8e886507fc39f5e447b2186451ae"
# These credentials are static and test-only; never reuse them outside regtest.
BITCOIN_RPC_USER="regtest"
BITCOIN_RPC_PASSWORD="bitcoin-regtest-only-password"
ELEMENTS_RPC_USER="elements"
ELEMENTS_RPC_PASSWORD="elements-regtest-only-password"
BITCOIN_WALLET="liquid-e2e-bitcoin"
ELEMENTS_WALLET="liquid-e2e-elements"

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command not found: $command_name" >&2
    exit 127
  fi
}

require_command docker
require_command jq

COMPOSE_VERSION="$(docker compose version 2>/dev/null || true)"
if [[ -z "$COMPOSE_VERSION" || ! "$COMPOSE_VERSION" =~ v2([.[:space:]-]|$) ]]; then
  echo "Docker Compose v2 is required; found: ${COMPOSE_VERSION:-unavailable}" >&2
  exit 2
fi

if [[ ! -f "$COMPOSE_FILE" ]]; then
  echo "Compose file not found: $COMPOSE_FILE" >&2
  exit 2
fi

if [[ ! "$PEGIN_CONFIRMATION_DEPTH" =~ ^[0-9]+$ ]]; then
  echo "PEGIN_CONFIRMATION_DEPTH must be an integer between 2 and 1000" >&2
  exit 2
fi
PEGIN_CONFIRMATION_DEPTH=$((10#$PEGIN_CONFIRMATION_DEPTH))
if (( PEGIN_CONFIRMATION_DEPTH < 2 || PEGIN_CONFIRMATION_DEPTH > 1000 )); then
  echo "PEGIN_CONFIRMATION_DEPTH must be an integer between 2 and 1000" >&2
  exit 2
fi

if ! jq -n --arg amount "$PEGIN_AMOUNT" '($amount | tonumber) > 0' >/dev/null 2>&1; then
  echo "PEGIN_AMOUNT must be a positive decimal amount" >&2
  exit 2
fi

if ! jq -n --arg pegin "$PEGIN_AMOUNT" --arg pegout "$PEGOUT_AMOUNT" \
  '($pegin | tonumber) > 0 and ($pegout | tonumber) > 0 and ($pegout | tonumber) < ($pegin | tonumber)' \
  >/dev/null 2>&1; then
  echo "PEGOUT_AMOUNT must be positive and smaller than PEGIN_AMOUNT" >&2
  exit 2
fi

if [[ ! "$RPC_TIMEOUT_SECONDS" =~ ^[1-9][0-9]*$ ]]; then
  echo "RPC_TIMEOUT_SECONDS must be a positive integer" >&2
  exit 2
fi

mkdir -p "$OUTPUT_DIR"

compose() {
  docker compose \
    --project-name "$PROJECT_NAME" \
    --file "$COMPOSE_FILE" \
    "$@"
}

log() {
  printf '[liquid-e2e] %s\n' "$*"
}

die() {
  echo "[liquid-e2e] ERROR: $*" >&2
  exit 1
}

COMPOSE_STARTED=0

cleanup() {
  local status=$?

  trap - EXIT
  set +e

  if (( COMPOSE_STARTED == 1 )); then
    compose ps >"$OUTPUT_DIR/compose-ps.txt" 2>&1
    compose logs --no-color bitcoin >"$OUTPUT_DIR/bitcoin.log" 2>&1
    compose logs --no-color elements >"$OUTPUT_DIR/elements.log" 2>&1
    compose logs --no-color >"$OUTPUT_DIR/compose.log" 2>&1
    compose down --volumes --remove-orphans >"$OUTPUT_DIR/cleanup.log" 2>&1
  fi

  if (( status == 0 )); then
    log "Completed successfully; fixtures and daemon logs are in $OUTPUT_DIR"
  else
    printf '[liquid-e2e] failed with exit status %s; artifacts preserved in %s\n' "$status" "$OUTPUT_DIR" >&2
  fi

  exit "$status"
}

trap cleanup EXIT

bitcoin_cli() {
  compose exec -T bitcoin bitcoin-cli \
    -regtest \
    -rpcconnect=127.0.0.1 \
    -rpcport=18443 \
    "-rpcuser=$BITCOIN_RPC_USER" \
    "-rpcpassword=$BITCOIN_RPC_PASSWORD" \
    "$@"
}

bitcoin_wallet_cli() {
  bitcoin_cli "-rpcwallet=$BITCOIN_WALLET" "$@"
}

elements_cli() {
  compose exec -T elements elements-cli \
    -chain=elementsregtest \
    -rpcconnect=127.0.0.1 \
    -rpcport=18884 \
    "-rpcuser=$ELEMENTS_RPC_USER" \
    "-rpcpassword=$ELEMENTS_RPC_PASSWORD" \
    "$@"
}

elements_wallet_cli() {
  elements_cli "-rpcwallet=$ELEMENTS_WALLET" "$@"
}

wait_for_rpc() {
  local label="$1"
  local cli_function="$2"
  local deadline=$((SECONDS + RPC_TIMEOUT_SECONDS))

  log "Waiting for $label RPC readiness (timeout ${RPC_TIMEOUT_SECONDS}s)"
  while ! "$cli_function" getblockchaininfo >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then
      die "$label RPC did not become ready before the timeout"
    fi
    sleep 1
  done
  log "$label RPC is ready"
}

ensure_wallet() {
  local cli_function="$1"
  local wallet_name="$2"

  if "$cli_function" listwallets | jq -e --arg wallet "$wallet_name" 'any(.[]; . == $wallet)' >/dev/null; then
    return
  fi

  if "$cli_function" createwallet "$wallet_name" >/dev/null 2>&1; then
    return
  fi

  "$cli_function" loadwallet "$wallet_name" >/dev/null 2>&1 \
    || die "Unable to create or load wallet $wallet_name"
}

assert_json() {
  local description="$1"
  shift

  local jq_args=()
  while [[ "${1:-}" == "--arg" || "${1:-}" == "--argjson" ]]; do
    jq_args+=("$1" "$2" "$3")
    shift 3
  done

  local expression="$1"
  local document="$2"

  if ! jq -e "${jq_args[@]}" "$expression" <<<"$document" >/dev/null; then
    die "Assertion failed: $description"
  fi
  log "PASS: $description"
}

elements_wallet_balance() {
  elements_wallet_cli getwalletinfo | jq -r '
    def as_number:
      if type == "number" then .
      elif type == "string" then tonumber
      else 0
      end;
    if (.balance | type) == "object" then
      (.balance.bitcoin // 0) | as_number
    elif (.balance | type) == "number" or (.balance | type) == "string" then
      .balance | as_number
    else
      0
    end
  '
}

log "Using Compose project $PROJECT_NAME"
log "Using PEGIN_CONFIRMATION_DEPTH=$PEGIN_CONFIRMATION_DEPTH"
log "Validating Compose configuration"
export PEGIN_CONFIRMATION_DEPTH
compose config --quiet

# A generated project name makes every default invocation use fresh named
# volumes. The pre-clean only removes volumes owned by this harness project.
compose down --volumes --remove-orphans >/dev/null 2>&1 || true

log "Starting Bitcoin regtest"
COMPOSE_STARTED=1
compose up --detach bitcoin
wait_for_rpc "Bitcoin" bitcoin_cli

log "Starting Elements regtest after Bitcoin readiness"
compose up --detach elements
wait_for_rpc "Elements" elements_cli

BITCOIN_CHAIN_INFO="$(bitcoin_cli getblockchaininfo)"
BITCOIN_GENESIS_ACTUAL="$(bitcoin_cli getblockhash 0)"
assert_json "Bitcoin is on regtest" '.chain == "regtest"' "$BITCOIN_CHAIN_INFO"
if [[ "$BITCOIN_GENESIS_ACTUAL" != "$BITCOIN_GENESIS" ]]; then
  die "Unexpected Bitcoin regtest genesis: $BITCOIN_GENESIS_ACTUAL"
fi
log "PASS: Bitcoin regtest genesis is $BITCOIN_GENESIS"

ELEMENTS_CHAIN_INFO="$(elements_cli getblockchaininfo)"
ELEMENTS_SIDECHAIN_INFO="$(elements_cli getsidechaininfo)"
assert_json "Elements sidechain reports the configured confirmation depth" \
  --argjson depth "$PEGIN_CONFIRMATION_DEPTH" \
  '.pegin_confirmation_depth == $depth' "$ELEMENTS_SIDECHAIN_INFO"
assert_json "Elements references the Bitcoin regtest genesis" \
  --arg genesis "$BITCOIN_GENESIS" \
  '.parent_blockhash == $genesis' "$ELEMENTS_SIDECHAIN_INFO"
assert_json "Elements uses the expected fedpegscript" \
  --arg fedpeg "$FEDPEG_SCRIPT" \
  '.fedpegscript == $fedpeg' "$ELEMENTS_SIDECHAIN_INFO"

printf '%s\n' "$BITCOIN_CHAIN_INFO" >"$OUTPUT_DIR/bitcoin-chain-info.json"
printf '%s\n' "$ELEMENTS_CHAIN_INFO" >"$OUTPUT_DIR/elements-chain-info.json"
printf '%s\n' "$ELEMENTS_SIDECHAIN_INFO" >"$OUTPUT_DIR/elements-sidechain-info.json"

log "Creating idempotent test wallets"
ensure_wallet bitcoin_cli "$BITCOIN_WALLET"
ensure_wallet elements_cli "$ELEMENTS_WALLET"

BITCOIN_MINING_ADDRESS="$(bitcoin_wallet_cli getnewaddress)"
ELEMENTS_MINING_ADDRESS="$(elements_wallet_cli getnewaddress)"

log "Mining mature funding blocks on both regtest chains"
bitcoin_wallet_cli generatetoaddress 101 "$BITCOIN_MINING_ADDRESS" >/dev/null
elements_wallet_cli generatetoaddress 101 "$ELEMENTS_MINING_ADDRESS" >/dev/null

SIDECHAIN_BALANCE_BEFORE="$(elements_wallet_balance)"

log "Generating a fresh peg-in address"
PEGIN_ADDRESS_INFO="$(elements_wallet_cli getpeginaddress)"
PEGIN_MAINCHAIN_ADDRESS="$(jq -r '.mainchain_address // empty' <<<"$PEGIN_ADDRESS_INFO")"
CLAIM_SCRIPT="$(jq -r '.claim_script // empty' <<<"$PEGIN_ADDRESS_INFO")"
[[ -n "$PEGIN_MAINCHAIN_ADDRESS" ]] || die "Elements did not return a mainchain peg-in address"
[[ -n "$CLAIM_SCRIPT" ]] || die "Elements did not return a peg-in claim script"

log "Sending $PEGIN_AMOUNT BTC to the peg-in address"
BITCOIN_PEGIN_TXID="$(bitcoin_wallet_cli sendtoaddress "$PEGIN_MAINCHAIN_ADDRESS" "$PEGIN_AMOUNT")"
bitcoin_wallet_cli generatetoaddress 1 "$BITCOIN_MINING_ADDRESS" >/dev/null

BITCOIN_PEGIN_TX_INFO="$(bitcoin_wallet_cli gettransaction "$BITCOIN_PEGIN_TXID")"
BITCOIN_PEGIN_CONFIRMATIONS="$(jq -r '.confirmations // 0' <<<"$BITCOIN_PEGIN_TX_INFO")"
[[ "$BITCOIN_PEGIN_CONFIRMATIONS" =~ ^[0-9]+$ ]] || die "Could not read peg-in transaction confirmations"
(( BITCOIN_PEGIN_CONFIRMATIONS >= 1 )) || die "Peg-in transaction was not mined"
BITCOIN_PEGIN_RAW_TX="$(bitcoin_cli getrawtransaction "$BITCOIN_PEGIN_TXID")"
BITCOIN_PEGIN_TXOUT_PROOF="$(bitcoin_cli gettxoutproof "[\"$BITCOIN_PEGIN_TXID\"]")"
BITCOIN_PEGIN_BLOCK_HASH="$(jq -r '.blockhash // empty' <<<"$BITCOIN_PEGIN_TX_INFO")"
[[ -n "$BITCOIN_PEGIN_BLOCK_HASH" ]] || die "Peg-in transaction has no containing block"

EARLY_CLAIM_REJECTED=false
if EARLY_CLAIM_OUTPUT="$(elements_wallet_cli claimpegin "$BITCOIN_PEGIN_RAW_TX" "$BITCOIN_PEGIN_TXOUT_PROOF" "$CLAIM_SCRIPT" 2>&1)"; then
  die "Immature claimpegin unexpectedly succeeded: $EARLY_CLAIM_OUTPUT"
else
  EARLY_CLAIM_REJECTED=true
  printf '%s\n' "$EARLY_CLAIM_OUTPUT" >"$OUTPUT_DIR/early-claim-rejection.txt"
  log "PASS: immature claimpegin was rejected"
fi

REQUIRED_PARENT_CONFIRMATIONS=$((PEGIN_CONFIRMATION_DEPTH + 2))
PARENT_BLOCKS_TO_MINE=$((REQUIRED_PARENT_CONFIRMATIONS - BITCOIN_PEGIN_CONFIRMATIONS))
if (( PARENT_BLOCKS_TO_MINE > 0 )); then
  log "Mining $PARENT_BLOCKS_TO_MINE additional Bitcoin blocks for $REQUIRED_PARENT_CONFIRMATIONS total confirmations"
  bitcoin_wallet_cli generatetoaddress "$PARENT_BLOCKS_TO_MINE" "$BITCOIN_MINING_ADDRESS" >/dev/null
fi

BITCOIN_PEGIN_TX_INFO="$(bitcoin_wallet_cli gettransaction "$BITCOIN_PEGIN_TXID")"
BITCOIN_PEGIN_CONFIRMATIONS="$(jq -r '.confirmations // 0' <<<"$BITCOIN_PEGIN_TX_INFO")"
if (( BITCOIN_PEGIN_CONFIRMATIONS < REQUIRED_PARENT_CONFIRMATIONS )); then
  die "Peg-in transaction has $BITCOIN_PEGIN_CONFIRMATIONS confirmations; expected at least $REQUIRED_PARENT_CONFIRMATIONS"
fi
log "PASS: peg-in transaction reached $BITCOIN_PEGIN_CONFIRMATIONS parent confirmations"

log "Claiming the mature peg-in"
CLAIM_TXID="$(elements_wallet_cli claimpegin "$BITCOIN_PEGIN_RAW_TX" "$BITCOIN_PEGIN_TXOUT_PROOF" "$CLAIM_SCRIPT")"
[[ "$CLAIM_TXID" =~ ^[0-9a-fA-F]{64}$ ]] || die "Elements returned an invalid peg-in claim txid"
elements_wallet_cli generatetoaddress 1 "$ELEMENTS_MINING_ADDRESS" >/dev/null

CLAIM_TX_INFO="$(elements_cli getrawtransaction "$CLAIM_TXID" true)"
CLAIM_CONFIRMATIONS="$(jq -r '.confirmations // 0' <<<"$CLAIM_TX_INFO")"
if (( CLAIM_CONFIRMATIONS < 1 )); then
  die "Peg-in claim transaction was not confirmed"
fi
assert_json "confirmed claim contains a peg-in input" \
  '[.vin[]? | select(.is_pegin == true)] | length >= 1' "$CLAIM_TX_INFO"

SIDECHAIN_BALANCE_AFTER_CLAIM="$(elements_wallet_balance)"
assert_json "confirmed peg-in increases the sidechain wallet balance" \
  --arg before "$SIDECHAIN_BALANCE_BEFORE" \
  --arg after "$SIDECHAIN_BALANCE_AFTER_CLAIM" \
  '($after | tonumber) > 0 and ($after | tonumber) > ($before | tonumber)' '{}'

jq -n \
  --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg bitcoin_genesis "$BITCOIN_GENESIS" \
  --arg fedpeg_script "$FEDPEG_SCRIPT" \
  --argjson pegin_confirmation_depth "$PEGIN_CONFIRMATION_DEPTH" \
  --argjson required_parent_confirmations "$REQUIRED_PARENT_CONFIRMATIONS" \
  --arg mainchain_address "$PEGIN_MAINCHAIN_ADDRESS" \
  --arg claim_script "$CLAIM_SCRIPT" \
  --arg bitcoin_txid "$BITCOIN_PEGIN_TXID" \
  --arg bitcoin_block_hash "$BITCOIN_PEGIN_BLOCK_HASH" \
  --arg bitcoin_raw_tx "$BITCOIN_PEGIN_RAW_TX" \
  --arg bitcoin_txout_proof "$BITCOIN_PEGIN_TXOUT_PROOF" \
  --arg claim_txid "$CLAIM_TXID" \
  --argjson claim_tx "$CLAIM_TX_INFO" \
  --argjson bitcoin_confirmations "$BITCOIN_PEGIN_CONFIRMATIONS" \
  --argjson claim_confirmations "$CLAIM_CONFIRMATIONS" \
  --argjson early_claim_rejected "$EARLY_CLAIM_REJECTED" \
  --argjson sidechain_balance_before "$SIDECHAIN_BALANCE_BEFORE" \
  --argjson sidechain_balance_after "$SIDECHAIN_BALANCE_AFTER_CLAIM" \
  '{
    schema_version: 1,
    generated_at: $generated_at,
    network: {
      parent: "bitcoin-regtest",
      sidechain: "elementsregtest",
      parent_genesis: $bitcoin_genesis,
      fedpeg_script: $fedpeg_script
    },
    configuration: {
      pegin_confirmation_depth: $pegin_confirmation_depth,
      required_parent_confirmations: $required_parent_confirmations
    },
    peg_in: {
      mainchain_address: $mainchain_address,
      claim_script: $claim_script,
      bitcoin_txid: $bitcoin_txid,
      bitcoin_block_hash: $bitcoin_block_hash,
      bitcoin_confirmations: $bitcoin_confirmations,
      bitcoin_raw_transaction: $bitcoin_raw_tx,
      bitcoin_txout_proof: $bitcoin_txout_proof,
      early_claim_rejected: $early_claim_rejected,
      claim_txid: $claim_txid,
      claim_confirmations: $claim_confirmations,
      claim_transaction: $claim_tx,
      sidechain_balance_before: $sidechain_balance_before,
      sidechain_balance_after: $sidechain_balance_after
    }
  }' >"$OUTPUT_DIR/pegin.json"

log "Submitting representative peg-out request"
BITCOIN_DESTINATION="$(bitcoin_wallet_cli getnewaddress "liquid-e2e-pegout")"
BITCOIN_DESTINATION_INFO="$(bitcoin_wallet_cli getaddressinfo "$BITCOIN_DESTINATION")"
BITCOIN_DESTINATION_SCRIPT="$(jq -r '.scriptPubKey // empty' <<<"$BITCOIN_DESTINATION_INFO")"
[[ -n "$BITCOIN_DESTINATION_SCRIPT" ]] || die "Could not resolve the Bitcoin peg-out destination script"

PEGOUT_TXID="$(elements_wallet_cli sendtomainchain "$BITCOIN_DESTINATION" "$PEGOUT_AMOUNT")"
[[ "$PEGOUT_TXID" =~ ^[0-9a-fA-F]{64}$ ]] || die "Elements returned an invalid peg-out txid"
PEGOUT_RAW_TX="$(elements_cli getrawtransaction "$PEGOUT_TXID")"
PEGOUT_TX_INFO="$(elements_cli decoderawtransaction "$PEGOUT_RAW_TX")"
PEGOUT_OUTPUTS="$(jq '[.vout[]? | select(.scriptPubKey?.pegout_chain != null)]' <<<"$PEGOUT_TX_INFO")"
PEGOUT_OUTPUT_COUNT="$(jq 'length' <<<"$PEGOUT_OUTPUTS")"
[[ "$PEGOUT_OUTPUT_COUNT" == "1" ]] || die "Expected exactly one decoded peg-out nulldata output; found $PEGOUT_OUTPUT_COUNT"
PEGOUT_METADATA="$(jq -c '.[0].scriptPubKey' <<<"$PEGOUT_OUTPUTS")"
PEGOUT_CHAIN="$(jq -r '.pegout_chain // empty' <<<"$PEGOUT_METADATA")"
PEGOUT_ADDRESS="$(jq -r '.pegout_address // empty' <<<"$PEGOUT_METADATA")"
PEGOUT_HEX="$(jq -r '.pegout_hex // empty' <<<"$PEGOUT_METADATA")"
PEGOUT_TYPE="$(jq -r '.pegout_type // empty' <<<"$PEGOUT_METADATA")"

[[ "$PEGOUT_CHAIN" == "$BITCOIN_GENESIS" ]] || die "Peg-out metadata references $PEGOUT_CHAIN instead of Bitcoin genesis $BITCOIN_GENESIS"
if [[ "$PEGOUT_ADDRESS" != "$BITCOIN_DESTINATION" && "$PEGOUT_HEX" != "$BITCOIN_DESTINATION_SCRIPT" ]]; then
  die "Peg-out metadata does not match the Bitcoin destination address or script"
fi
[[ -n "$PEGOUT_TYPE" ]] || die "Peg-out metadata did not include a script type"
log "PASS: peg-out metadata matches the Bitcoin destination and parent genesis"

elements_wallet_cli generatetoaddress 1 "$ELEMENTS_MINING_ADDRESS" >/dev/null
PEGOUT_CONFIRMED_INFO="$(elements_cli getrawtransaction "$PEGOUT_TXID" true)"
PEGOUT_CONFIRMATIONS="$(jq -r '.confirmations // 0' <<<"$PEGOUT_CONFIRMED_INFO")"
if (( PEGOUT_CONFIRMATIONS < 1 )); then
  die "Representative peg-out transaction was not confirmed on Elements"
fi
log "PASS: representative peg-out transaction confirmed on Elements"

jq -n \
  --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg bitcoin_genesis "$BITCOIN_GENESIS" \
  --arg destination "$BITCOIN_DESTINATION" \
  --arg destination_script "$BITCOIN_DESTINATION_SCRIPT" \
  --arg pegout_txid "$PEGOUT_TXID" \
  --arg pegout_raw_tx "$PEGOUT_RAW_TX" \
  --argjson pegout_transaction "$PEGOUT_CONFIRMED_INFO" \
  --argjson pegout_metadata "$PEGOUT_METADATA" \
  --argjson confirmations "$PEGOUT_CONFIRMATIONS" \
  '{
    schema_version: 1,
    generated_at: $generated_at,
    network: {
      parent: "bitcoin-regtest",
      sidechain: "elementsregtest",
      parent_genesis: $bitcoin_genesis
    },
    peg_out: {
      destination_address: $destination,
      destination_script: $destination_script,
      elements_txid: $pegout_txid,
      elements_raw_transaction: $pegout_raw_tx,
      elements_confirmations: $confirmations,
      decoded_nulldata: $pegout_metadata,
      confirmed_transaction: $pegout_transaction,
      watchmen_release_observed: false,
      limitation: "The local two-node harness has no Watchmen/functionary set; this fixture validates the confirmed sendtomainchain request and its parent-chain metadata, not a Bitcoin-side release."
    }
  }' >"$OUTPUT_DIR/pegout.json"

log "Wrote reusable fixtures: $OUTPUT_DIR/pegin.json and $OUTPUT_DIR/pegout.json"
