#!/usr/bin/env bash
set -euo pipefail

# Opt-in integration harness for a local Bitcoin regtest + Elements
# elementsregtest pair.  This intentionally exercises only the local daemon
# protocol surface; it is not a production federation test.

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
readonly REQUIRED_PARENT_CONFIRMATIONS=102
readonly RPC_USER="${LIQUID_RPC_USER:-liquid_e2e}"
readonly RPC_PASSWORD="${LIQUID_RPC_PASSWORD:-$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')}"
readonly BTC_WALLET="liquid-e2e-bitcoin"
readonly ELEMENTS_WALLET="liquid-e2e-elements"

fail() {
    printf 'Liquid E2E failure: %s\n' "$*" >&2
    exit 1
}

command -v mktemp >/dev/null 2>&1 || fail "required command not found: mktemp"
command -v realpath >/dev/null 2>&1 || fail "required command not found: realpath"

mkdir -p -- "${REPO_ROOT}/target"
readonly TARGET_ROOT="$(cd -- "${REPO_ROOT}/target" && pwd -P)"

assert_no_symlink_components() {
    local raw_path="$1"
    local current="/"
    local component
    local -a components=()

    IFS='/' read -r -a components <<<"${raw_path#/}"
    for component in "${components[@]}"; do
        [[ -z "$component" || "$component" == "." ]] && continue
        current="${current%/}/${component}"
        [[ -L "$current" ]] && fail "unsafe symlink component in path: ${raw_path}"
    done
}

require_owned_directory_or_parent() {
    local path="$1"
    local label="$2"
    local probe="$path"

    if [[ -e "$path" || -L "$path" ]]; then
        [[ -d "$path" && ! -L "$path" && -O "$path" ]] || \
            fail "${label} must be an owned, non-symlink directory: ${path}"
        return 0
    fi

    while [[ ! -e "$probe" && ! -L "$probe" && "$probe" != "/" ]]; do
        probe="$(dirname -- "$probe")"
    done
    [[ -d "$probe" && ! -L "$probe" && -O "$probe" ]] || \
        fail "nearest existing ${label} parent must be an owned, non-symlink directory: ${probe}"
}

resolve_artifact_parent() {
    local configured_path="$1"
    local absolute_path="$configured_path"
    local resolved_path

    if [[ "$absolute_path" != /* ]]; then
        absolute_path="${REPO_ROOT}/${absolute_path}"
    fi
    assert_no_symlink_components "$absolute_path"
    resolved_path="$(realpath -m -- "$absolute_path")" || \
        fail "could not resolve LIQUID_E2E_ARTIFACT_DIR: ${configured_path}"

    [[ "$resolved_path" != "/" && "$resolved_path" != "$HOME" && \
        "$resolved_path" != "$REPO_ROOT" && "$resolved_path" != "$TARGET_ROOT" ]] || \
        fail "LIQUID_E2E_ARTIFACT_DIR must be a subdirectory inside repo target/: ${configured_path}"
    case "$resolved_path" in
        "${TARGET_ROOT}"/*) ;;
        *) fail "LIQUID_E2E_ARTIFACT_DIR must resolve inside repo target/: ${configured_path}" ;;
    esac

    require_owned_directory_or_parent "$resolved_path" "artifact directory"
    printf '%s\n' "$resolved_path"
}

validate_bounded_decimal() {
    local name="$1"
    local value="$2"
    local minimum="$3"
    local maximum="$4"
    local max_digits="$5"

    [[ "$value" =~ ^[0-9]+$ ]] || \
        fail "${name} must be a strict decimal integer in the range ${minimum}..${maximum}"
    (( ${#value} <= max_digits )) || \
        fail "${name} must be a strict decimal integer in the range ${minimum}..${maximum}"

    local normalized=$((10#$value))
    (( normalized >= minimum && normalized <= maximum )) || \
        fail "${name} must be a strict decimal integer in the range ${minimum}..${maximum}"
    printf '%d\n' "$normalized"
}

LIQUID_PEGIN_CONFIRMATION_DEPTH="${LIQUID_PEGIN_CONFIRMATION_DEPTH:-100}"
LIQUID_BTC_RPC_PORT="${LIQUID_BTC_RPC_PORT:-18888}"
LIQUID_ELEMENTS_RPC_PORT="${LIQUID_ELEMENTS_RPC_PORT:-18884}"
LIQUID_BTC_P2P_PORT="${LIQUID_BTC_P2P_PORT:-18889}"
LIQUID_ELEMENTS_P2P_PORT="${LIQUID_ELEMENTS_P2P_PORT:-18885}"

LIQUID_PEGIN_CONFIRMATION_DEPTH="$(validate_bounded_decimal \
    LIQUID_PEGIN_CONFIRMATION_DEPTH "$LIQUID_PEGIN_CONFIRMATION_DEPTH" 2 1000 4)"
LIQUID_BTC_RPC_PORT="$(validate_bounded_decimal LIQUID_BTC_RPC_PORT "$LIQUID_BTC_RPC_PORT" 1 65535 5)"
LIQUID_ELEMENTS_RPC_PORT="$(validate_bounded_decimal LIQUID_ELEMENTS_RPC_PORT "$LIQUID_ELEMENTS_RPC_PORT" 1 65535 5)"
LIQUID_BTC_P2P_PORT="$(validate_bounded_decimal LIQUID_BTC_P2P_PORT "$LIQUID_BTC_P2P_PORT" 1 65535 5)"
LIQUID_ELEMENTS_P2P_PORT="$(validate_bounded_decimal LIQUID_ELEMENTS_P2P_PORT "$LIQUID_ELEMENTS_P2P_PORT" 1 65535 5)"

if (( LIQUID_BTC_RPC_PORT == LIQUID_ELEMENTS_RPC_PORT ||
    LIQUID_BTC_RPC_PORT == LIQUID_BTC_P2P_PORT ||
    LIQUID_BTC_RPC_PORT == LIQUID_ELEMENTS_P2P_PORT ||
    LIQUID_ELEMENTS_RPC_PORT == LIQUID_BTC_P2P_PORT ||
    LIQUID_ELEMENTS_RPC_PORT == LIQUID_ELEMENTS_P2P_PORT ||
    LIQUID_BTC_P2P_PORT == LIQUID_ELEMENTS_P2P_PORT )); then
    fail "all Liquid E2E RPC and P2P ports must be pairwise distinct"
fi

readonly LIQUID_PEGIN_CONFIRMATION_DEPTH
readonly LIQUID_BTC_RPC_PORT LIQUID_ELEMENTS_RPC_PORT
readonly LIQUID_BTC_P2P_PORT LIQUID_ELEMENTS_P2P_PORT
readonly ARTIFACT_PARENT="$(resolve_artifact_parent \
    "${LIQUID_E2E_ARTIFACT_DIR:-${TARGET_ROOT}/liquid-e2e-artifacts}")"
mkdir -p -- "$ARTIFACT_PARENT"
require_owned_directory_or_parent "$ARTIFACT_PARENT" "artifact directory"
readonly ARTIFACT_DIR="$(mktemp -d -- "${ARTIFACT_PARENT}/run.XXXXXX")"
readonly ARTIFACT_OWNER_MARKER="${ARTIFACT_DIR}/.conxian-liquid-e2e-artifact-owner"
printf 'conxian-liquid-e2e-artifact-v1\nrepo=%s\n' "$REPO_ROOT" >"$ARTIFACT_OWNER_MARKER"

readonly RUN_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/conxian-liquid-e2e.XXXXXX")"
readonly BTC_DATA="${RUN_ROOT}/bitcoin"
readonly ELEMENTS_DATA="${RUN_ROOT}/elements"

BITCOIND_PID=""
ELEMENTSD_PID=""
BITCOIN_LOG="${ARTIFACT_DIR}/bitcoin-stdout.log"
ELEMENTS_LOG="${ARTIFACT_DIR}/elements-stdout.log"

mkdir -p "$BTC_DATA" "$ELEMENTS_DATA"

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    set +e

    if [[ -n "$ELEMENTSD_PID" ]] && kill -0 "$ELEMENTSD_PID" 2>/dev/null; then
        kill "$ELEMENTSD_PID" 2>/dev/null
        wait "$ELEMENTSD_PID" 2>/dev/null
    fi
    if [[ -n "$BITCOIND_PID" ]] && kill -0 "$BITCOIND_PID" 2>/dev/null; then
        kill "$BITCOIND_PID" 2>/dev/null
        wait "$BITCOIND_PID" 2>/dev/null
    fi

    if [[ -f "${BTC_DATA}/regtest/debug.log" ]]; then
        cp -- "${BTC_DATA}/regtest/debug.log" "${ARTIFACT_DIR}/bitcoin-debug.log"
    fi
    if [[ -f "${ELEMENTS_DATA}/elementsregtest/debug.log" ]]; then
        cp -- "${ELEMENTS_DATA}/elementsregtest/debug.log" "${ARTIFACT_DIR}/elements-debug.log"
    fi
    printf '%s\n' "$status" >"${ARTIFACT_DIR}/exit-status"
    rm -rf -- "$RUN_ROOT"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

need_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

for command_name in jq curl od tr awk; do
    need_command "$command_name"
done

DAEMON_ROOT="$(bash "${SCRIPT_DIR}/install_daemons.sh")"
readonly BITCOIND="${DAEMON_ROOT}/bitcoin-31.1/bin/bitcoind"
readonly BITCOIN_CLI="${DAEMON_ROOT}/bitcoin-31.1/bin/bitcoin-cli"
readonly ELEMENTSD="${DAEMON_ROOT}/elements-23.3.3/bin/elementsd"
readonly ELEMENTS_CLI="${DAEMON_ROOT}/elements-23.3.3/bin/elements-cli"

btc() {
    "$BITCOIN_CLI" \
        -regtest \
        -datadir="$BTC_DATA" \
        -rpcconnect=127.0.0.1 \
        -rpcport="$BTC_RPC_PORT" \
        -rpcuser="$RPC_USER" \
        -rpcpassword="$RPC_PASSWORD" \
        "$@"
}

btc_wallet() {
    btc -rpcwallet="$BTC_WALLET" "$@"
}

elm() {
    "$ELEMENTS_CLI" \
        -chain=elementsregtest \
        -datadir="$ELEMENTS_DATA" \
        -rpcconnect=127.0.0.1 \
        -rpcport="$ELEMENTS_RPC_PORT" \
        -rpcuser="$RPC_USER" \
        -rpcpassword="$RPC_PASSWORD" \
        "$@"
}

elm_wallet() {
    elm -rpcwallet="$ELEMENTS_WALLET" "$@"
}

wait_for_rpc() {
    local name="$1"
    local command_name="$2"
    local attempts=0
    while (( attempts < 120 )); do
        if "$command_name" getblockchaininfo >/dev/null 2>&1; then
            return 0
        fi
        attempts=$((attempts + 1))
        sleep 0.25
    done
    printf '%s RPC did not become ready\n' "$name" >&2
    return 1
}

wait_for_elements_sync() {
    local attempts=0
    while (( attempts < 120 )); do
        if elm getblockchaininfo | jq -e '.initialblockdownload == false and .blocks >= 1' >/dev/null 2>&1; then
            return 0
        fi
        attempts=$((attempts + 1))
        sleep 0.25
    done
    printf 'Elements chain did not leave initial block download\n' >&2
    return 1
}

assert_jq() {
    local description="$1"
    local expression="$2"
    local input="$3"
    if ! jq -e "$expression" <<<"$input" >/dev/null; then
        printf 'assertion failed: %s\n' "$description" >&2
        return 1
    fi
}

assert_jq_with_args() {
    local description="$1"
    local input="$2"
    shift 2
    if ! jq -e "$@" <<<"$input" >/dev/null; then
        printf 'assertion failed: %s\n' "$description" >&2
        return 1
    fi
}

decimal_to_sats() {
    local amount="$1"
    local whole="$amount"
    local fraction=""

    if [[ "$amount" == *.* ]]; then
        whole="${amount%%.*}"
        fraction="${amount#*.}"
    fi

    if [[ ! "$whole" =~ ^[0-9]+$ || ! "$fraction" =~ ^[0-9]*$ || ${#fraction} -gt 8 ]]; then
        fail "decoded Elements amount '${amount}' is not a base-10 value with at most 8 decimals"
    fi

    while ((${#fraction} < 8)); do
        fraction+="0"
    done

    printf '%d\n' "$((10#$whole * 100000000 + 10#${fraction:-0}))"
}

printf 'Starting pinned Bitcoin Core %s and Elements Core %s\n' "31.1" "23.3.3"
"$BITCOIND" \
    -regtest \
    -datadir="$BTC_DATA" \
    -server=1 \
    -daemon=0 \
    -listen=0 \
    -discover=0 \
    -dnsseed=0 \
    -txindex=1 \
    -fallbackfee=0.0002 \
    -port="$BTC_P2P_PORT" \
    -rpcbind=127.0.0.1 \
    -rpcallowip=127.0.0.1 \
    -rpcport="$BTC_RPC_PORT" \
    -rpcuser="$RPC_USER" \
    -rpcpassword="$RPC_PASSWORD" \
    >"$BITCOIN_LOG" 2>&1 &
BITCOIND_PID=$!

"$ELEMENTSD" \
    -chain=elementsregtest \
    -datadir="$ELEMENTS_DATA" \
    -server=1 \
    -daemon=0 \
    -listen=0 \
    -discover=0 \
    -dnsseed=0 \
    -txindex=1 \
    -fallbackfee=0.0002 \
    -anyonecanspendaremine=1 \
    -initialfreecoins=0 \
    -validatepegin=1 \
    -peginconfirmationdepth="$LIQUID_PEGIN_CONFIRMATION_DEPTH" \
    -fedpegscript=51 \
    -mainchainrpchost=127.0.0.1 \
    -mainchainrpcport="$BTC_RPC_PORT" \
    -mainchainrpcuser="$RPC_USER" \
    -mainchainrpcpassword="$RPC_PASSWORD" \
    -port="$ELEMENTS_P2P_PORT" \
    -rpcbind=127.0.0.1 \
    -rpcallowip=127.0.0.1 \
    -rpcport="$ELEMENTS_RPC_PORT" \
    -rpcuser="$RPC_USER" \
    -rpcpassword="$RPC_PASSWORD" \
    >"$ELEMENTS_LOG" 2>&1 &
ELEMENTSD_PID=$!

wait_for_rpc Bitcoin btc || fail "Bitcoin daemon startup; see ${BITCOIN_LOG}"
wait_for_rpc Elements elm || fail "Elements daemon startup; see ${ELEMENTS_LOG}"

btc createwallet "$BTC_WALLET" >/dev/null
elm createwallet "$ELEMENTS_WALLET" >/dev/null

BTC_MINING_ADDRESS="$(btc_wallet getnewaddress "" bech32)"
ELEMENTS_MINING_ADDRESS="$(elm_wallet getnewaddress)"
btc generatetoaddress 101 "$BTC_MINING_ADDRESS" >/dev/null

# The custom Elements chain needs one local block before it can finish its
# initial state transition and expose getpeginaddress.
elm generatetoaddress 1 "$ELEMENTS_MINING_ADDRESS" >/dev/null
wait_for_elements_sync || fail "Elements initial synchronization; see ${ELEMENTS_LOG}"

readonly SIDECHAIN_INFO="$(elm getsidechaininfo)"
printf '%s\n' "$SIDECHAIN_INFO" >"${ARTIFACT_DIR}/sidechain-info.json"
readonly SIDECHAIN_PARENT_GENESIS="$(jq -er '.parent_blockhash' <<<"$SIDECHAIN_INFO")"
readonly PEGGED_ASSET="$(jq -er '.pegged_asset' <<<"$SIDECHAIN_INFO")"
readonly PEGIN_CONFIRMATION_DEPTH="$(jq -er '.pegin_confirmation_depth' <<<"$SIDECHAIN_INFO")"
readonly BTC_GENESIS="$(btc getblockhash 0)"

[[ "$BTC_GENESIS" == "$SIDECHAIN_PARENT_GENESIS" ]] || fail \
    "Elements parent genesis ${SIDECHAIN_PARENT_GENESIS} does not match Bitcoin genesis ${BTC_GENESIS}"
[[ "$PEGIN_CONFIRMATION_DEPTH" == "$LIQUID_PEGIN_CONFIRMATION_DEPTH" ]] || fail \
    "Elements reported peg-in confirmation depth ${PEGIN_CONFIRMATION_DEPTH}, expected configured ${LIQUID_PEGIN_CONFIRMATION_DEPTH}"
[[ "$PEGGED_ASSET" =~ ^[0-9a-fA-F]{64}$ ]] || fail "Elements returned an invalid pegged asset: ${PEGGED_ASSET}"

# Elements' claimpegin path requires the live policy depth plus two parent
# blocks in this daemon version. Keep the previously verified 102-confirmation
# target while deriving the actual claim-readiness floor from the node.
readonly CLAIM_MIN_PARENT_CONFIRMATIONS=$((PEGIN_CONFIRMATION_DEPTH + 2))
readonly TARGET_PARENT_CONFIRMATIONS=$((
    REQUIRED_PARENT_CONFIRMATIONS > CLAIM_MIN_PARENT_CONFIRMATIONS
        ? REQUIRED_PARENT_CONFIRMATIONS
        : CLAIM_MIN_PARENT_CONFIRMATIONS
))

readonly PEG_IN_INFO="$(elm_wallet getpeginaddress)"
readonly MAINCHAIN_PEG_IN_ADDRESS="$(jq -er '.mainchain_address' <<<"$PEG_IN_INFO")"
readonly CLAIM_SCRIPT="$(jq -er '.claim_script' <<<"$PEG_IN_INFO")"

readonly PARENT_TXID="$(btc_wallet sendtoaddress "$MAINCHAIN_PEG_IN_ADDRESS" 1.0)"
btc generatetoaddress "$TARGET_PARENT_CONFIRMATIONS" "$BTC_MINING_ADDRESS" >/dev/null

readonly PARENT_TX_INFO="$(btc gettransaction "$PARENT_TXID")"
readonly PARENT_CONFIRMATIONS="$(jq -er '.confirmations' <<<"$PARENT_TX_INFO")"
readonly PARENT_BLOCK_HASH="$(jq -er '.blockhash' <<<"$PARENT_TX_INFO")"
readonly PARENT_RAW_TX="$(btc getrawtransaction "$PARENT_TXID" false)"
readonly PARENT_TXOUT_PROOF="$(btc gettxoutproof "[\"${PARENT_TXID}\"]" "$PARENT_BLOCK_HASH")"

(( PARENT_CONFIRMATIONS >= REQUIRED_PARENT_CONFIRMATIONS )) || fail \
    "parent transaction has only ${PARENT_CONFIRMATIONS} confirmations; expected at least ${REQUIRED_PARENT_CONFIRMATIONS}"
(( PARENT_CONFIRMATIONS >= CLAIM_MIN_PARENT_CONFIRMATIONS )) || fail \
    "parent transaction has only ${PARENT_CONFIRMATIONS} confirmations; live claim policy requires ${CLAIM_MIN_PARENT_CONFIRMATIONS}"
[[ -n "$PARENT_RAW_TX" && -n "$PARENT_TXOUT_PROOF" && -n "$CLAIM_SCRIPT" ]] || fail "peg-in proof components are incomplete"

if ! CLAIM_TXID="$(elm_wallet claimpegin "$PARENT_RAW_TX" "$PARENT_TXOUT_PROOF" "$CLAIM_SCRIPT")"; then
    fail "Elements claimpegin rejected the parent proof; see ${ELEMENTS_LOG}"
fi
readonly CLAIM_TXID
elm generatetoaddress 1 "$ELEMENTS_MINING_ADDRESS" >/dev/null
readonly CLAIM_TX_INFO="$(elm gettransaction "$CLAIM_TXID")"
readonly CLAIM_CONFIRMATIONS="$(jq -er '.confirmations' <<<"$CLAIM_TX_INFO")"
readonly CLAIM_BLOCK_HASH="$(jq -er '.blockhash' <<<"$CLAIM_TX_INFO")"
readonly CLAIM_DECODED="$(elm getrawtransaction "$CLAIM_TXID" true)"
printf '%s\n' "$CLAIM_DECODED" >"${ARTIFACT_DIR}/claim-decoded.json"
{
    printf 'pegged_asset=%s\n' "$PEGGED_ASSET"
    jq -c --arg asset "$PEGGED_ASSET" '{
        claim_outputs: [
            .vout[]
            | select(.asset == $asset and .value != null and (.scriptPubKey.type // "") != "fee")
            | {n, asset, value, script_type: .scriptPubKey.type}
        ],
        fee_outputs: [
            .vout[]
            | select(.asset == $asset and .value != null and .scriptPubKey.type == "fee")
            | {n, asset, value, script_type: .scriptPubKey.type}
        ]
    }' <<<"$CLAIM_DECODED"
} >"${ARTIFACT_DIR}/claim-asset-diagnostics.txt"
readonly CLAIM_WITNESS="$(jq -ec '.vin[0].pegin_witness' <<<"$CLAIM_DECODED")"
CLAIM_OUTPUT_ROWS="$(jq -r --arg asset "$PEGGED_ASSET" '
    [
        .vout[]
        | select(.asset == $asset and .value != null and (.scriptPubKey.type // "") != "fee")
        | [(.n | tostring), (.value | tostring)]
        | @tsv
    ] | .[]
' <<<"$CLAIM_DECODED")" || fail "could not decode explicit pegged-asset claim outputs"
CLAIM_FEE_ROWS="$(jq -r --arg asset "$PEGGED_ASSET" '
    [
        .vout[]
        | select(.asset == $asset and .value != null and .scriptPubKey.type == "fee")
        | [(.n | tostring), (.value | tostring)]
        | @tsv
    ] | .[]
' <<<"$CLAIM_DECODED")" || fail "could not decode explicit Elements fee outputs"

[[ -n "$CLAIM_OUTPUT_ROWS" ]] || fail \
    "decoded claim transaction has no explicit output for live pegged asset ${PEGGED_ASSET}"

CLAIM_OUTPUTS='[]'
CLAIM_OUTPUT_COUNT=0
CLAIM_AMOUNT_SATS=0
while IFS=$'\t' read -r output_index output_value; do
    [[ -n "$output_value" ]] || continue
    output_sats="$(decimal_to_sats "$output_value")"
    CLAIM_OUTPUT_COUNT=$((CLAIM_OUTPUT_COUNT + 1))
    CLAIM_AMOUNT_SATS=$((CLAIM_AMOUNT_SATS + output_sats))
    CLAIM_OUTPUTS="$(jq -c \
        --arg asset "$PEGGED_ASSET" \
        --arg output_index "$output_index" \
        --arg raw_value "$output_value" \
        --argjson value_sats "$output_sats" \
        '. + [{
            visibility: "explicit",
            output_index: ($output_index | tonumber),
            raw_value: $raw_value,
            value_sats: $value_sats,
            asset: $asset
        }]' <<<"$CLAIM_OUTPUTS")"
done <<<"$CLAIM_OUTPUT_ROWS"

CLAIM_FEE_OUTPUT_COUNT=0
CLAIM_FEE_SATS=0
while IFS=$'\t' read -r _fee_output_index fee_value; do
    [[ -n "$fee_value" ]] || continue
    fee_sats="$(decimal_to_sats "$fee_value")"
    CLAIM_FEE_OUTPUT_COUNT=$((CLAIM_FEE_OUTPUT_COUNT + 1))
    CLAIM_FEE_SATS=$((CLAIM_FEE_SATS + fee_sats))
done <<<"$CLAIM_FEE_ROWS"

CLAIM_RECONCILED_SATS="$CLAIM_AMOUNT_SATS"
if (( CLAIM_FEE_OUTPUT_COUNT > 0 )); then
    CLAIM_RECONCILED_SATS=$((CLAIM_AMOUNT_SATS + CLAIM_FEE_SATS))
fi

if (( CLAIM_RECONCILED_SATS != 100000000 )); then
    printf 'peg-in conservation assertion failed:\n' >&2
    printf '  parent_deposit_sats=100000000\n' >&2
    printf '  claim_output_count=%s\n' "$CLAIM_OUTPUT_COUNT" >&2
    printf '  claim_outputs_sats=%s\n' "$CLAIM_AMOUNT_SATS" >&2
    printf '  fee_output_count=%s\n' "$CLAIM_FEE_OUTPUT_COUNT" >&2
    printf '  fee_outputs_sats=%s\n' "$CLAIM_FEE_SATS" >&2
    printf '  reconciled_sats=%s\n' "$CLAIM_RECONCILED_SATS" >&2
    printf '  pegged_asset=%s\n' "$PEGGED_ASSET" >&2
    printf '  decoded_transaction=%s\n' "${ARTIFACT_DIR}/claim-decoded.json" >&2
    fail "decoded claim outputs and Elements fee do not reconcile the 1 BTC parent deposit"
fi

readonly CLAIM_OUTPUTS CLAIM_AMOUNT_SATS CLAIM_FEE_OUTPUT_COUNT CLAIM_FEE_SATS CLAIM_RECONCILED_SATS

(( CLAIM_CONFIRMATIONS >= 1 )) || fail "claim transaction is not confirmed"
assert_jq "claim has a peg-in marker" \
    'any(.vin[]; .is_pegin == true)' "$CLAIM_DECODED"
assert_jq_with_args "claim references the parent transaction" "$CLAIM_DECODED" \
    --arg parent_txid "$PARENT_TXID" \
    'any(.vin[]; .is_pegin == true and .txid == $parent_txid and (.pegin_witness | length >= 5))'
assert_jq_with_args "claim has an explicit pegged-asset output" "$CLAIM_DECODED" \
    --arg asset "$PEGGED_ASSET" \
    '[.vout[] | select(.asset == $asset and .value != null and (.scriptPubKey.type // "") != "fee")] | length >= 1'
(( CLAIM_AMOUNT_SATS > 0 )) || fail "decoded claim output amount is zero sats"

if duplicate_error="$(elm_wallet claimpegin "$PARENT_RAW_TX" "$PARENT_TXOUT_PROOF" "$CLAIM_SCRIPT" 2>&1)"; then
    fail "duplicate claim unexpectedly succeeded"
fi
grep -Eiq 'already.?claimed|pegin-already-claimed' <<<"$duplicate_error" || {
    printf 'duplicate claim failed with an unexpected error: %s\n' "$duplicate_error" >&2
    exit 1
}

# A normal wallet transfer creates a blinded Elements output.  The harness
# checks the cryptographic representation, not the wallet's unblinded amount.
readonly CT_DESTINATION="$(elm_wallet getnewaddress)"
readonly CT_TXID="$(elm_wallet sendtoaddress "$CT_DESTINATION" 0.2)"
elm generatetoaddress 1 "$ELEMENTS_MINING_ADDRESS" >/dev/null
readonly CT_DECODED="$(elm getrawtransaction "$CT_TXID" true)"
printf '%s\n' "$CT_DECODED" >"${ARTIFACT_DIR}/confidential-transfer-decoded.json"
assert_jq "blinded transfer carries commitments and proofs" '
    [.vout[] | select(
        (.valuecommitment // "") != "" and
        (.assetcommitment // "") != "" and
        (.surjectionproof // "") != "" and
        (
            (.rangeproof // "") != "" or
            (."value-minimum" != null and
             ."value-maximum" != null and
             ."ct-exponent" != null and
             ."ct-bits" != null)
        )
    )] | length >= 1
' "$CT_DECODED"
assert_jq "explicit outputs do not claim blinded proof fields" '
    all(.vout[];
        ((.value != null and .asset != null) | not) or
        ((.valuecommitment // "") == "" and
         (.assetcommitment // "") == "" and
         (.rangeproof // "") == "" and
         (.surjectionproof // "") == "")
    )
' "$CT_DECODED"

readonly PEGOUT_DESTINATION="$(btc_wallet getnewaddress "" bech32)"
readonly PEGOUT_DESTINATION_SCRIPT="$(btc_wallet getaddressinfo "$PEGOUT_DESTINATION" | jq -er '.scriptPubKey')"
readonly PEGOUT_TXID="$(elm_wallet sendtomainchain "$PEGOUT_DESTINATION" 0.1 false)"
elm generatetoaddress 1 "$ELEMENTS_MINING_ADDRESS" >/dev/null
readonly PEGOUT_TX_INFO="$(elm gettransaction "$PEGOUT_TXID")"
readonly PEGOUT_CONFIRMATIONS="$(jq -er '.confirmations' <<<"$PEGOUT_TX_INFO")"
readonly PEGOUT_BLOCK_HASH="$(jq -er '.blockhash' <<<"$PEGOUT_TX_INFO")"
readonly PEGOUT_RAW_TX="$(elm getrawtransaction "$PEGOUT_TXID" false)"
readonly PEGOUT_DECODED="$(elm getrawtransaction "$PEGOUT_TXID" true)"
printf '%s\n' "$PEGOUT_RAW_TX" >"${ARTIFACT_DIR}/pegout-raw.hex"
printf '%s\n' "$PEGOUT_DECODED" >"${ARTIFACT_DIR}/pegout-decoded.json"

(( PEGOUT_CONFIRMATIONS >= 1 )) || fail "peg-out request is not confirmed"
assert_jq_with_args "peg-out has one OP_RETURN burn output bound to parent genesis" "$PEGOUT_DECODED" \
    --arg genesis "$SIDECHAIN_PARENT_GENESIS" \
    '[.vout[] | select(.scriptPubKey.type == "nulldata" and .scriptPubKey.pegout_chain == $genesis)] | length == 1'
assert_jq_with_args "peg-out destination address and script are encoded" "$PEGOUT_DECODED" \
    --arg genesis "$SIDECHAIN_PARENT_GENESIS" \
    --arg destination "$PEGOUT_DESTINATION" \
    --arg destination_script "$PEGOUT_DESTINATION_SCRIPT" \
    'any(.vout[]; .scriptPubKey.type == "nulldata" and
        .scriptPubKey.pegout_chain == $genesis and
        .scriptPubKey.pegout_address == $destination and
        .scriptPubKey.pegout_hex == $destination_script)'
assert_jq_with_args "peg-out burn output has the requested amount and asset" "$PEGOUT_DECODED" \
    --arg genesis "$SIDECHAIN_PARENT_GENESIS" \
    --arg asset "$PEGGED_ASSET" \
    '[.vout[] | select(.scriptPubKey.type == "nulldata" and .scriptPubKey.pegout_chain == $genesis and .asset == $asset and .value != null) |
        select(((.value * 100000000) | round) == 10000000)] | length == 1'
assert_jq_with_args "peg-out transaction records a non-zero fee" "$PEGOUT_DECODED" \
    --arg asset "$PEGGED_ASSET" \
    '((.fee[$asset] // .fees[$asset] // 0) * 100000000) > 0'

readonly PEGOUT_BURN_OUTPUT="$(jq -ce --arg genesis "$SIDECHAIN_PARENT_GENESIS" '
    first(.vout[] | select(.scriptPubKey.type == "nulldata" and .scriptPubKey.pegout_chain == $genesis))
' <<<"$PEGOUT_DECODED")"
readonly PEGOUT_OUTPUT_AMOUNT_SATS="$(jq -er '((.value * 100000000) | round)' <<<"$PEGOUT_BURN_OUTPUT")"
readonly PEGOUT_OUTPUT_ASSET="$(jq -er '.asset' <<<"$PEGOUT_BURN_OUTPUT")"
readonly PEGOUT_OUTPUT_DESTINATION="$(jq -er '.scriptPubKey.pegout_address' <<<"$PEGOUT_BURN_OUTPUT")"
readonly PEGOUT_OUTPUT_DESTINATION_SCRIPT="$(jq -er '.scriptPubKey.pegout_hex' <<<"$PEGOUT_BURN_OUTPUT")"
[[ "$PEGOUT_OUTPUT_AMOUNT_SATS" == "10000000" ]] || fail \
    "decoded peg-out burn amount is ${PEGOUT_OUTPUT_AMOUNT_SATS} sats, expected 10000000"
[[ "$PEGOUT_OUTPUT_ASSET" == "$PEGGED_ASSET" ]] || fail \
    "decoded peg-out asset ${PEGOUT_OUTPUT_ASSET} does not match live pegged asset ${PEGGED_ASSET}"
[[ "$PEGOUT_OUTPUT_DESTINATION" == "$PEGOUT_DESTINATION" ]] || fail \
    "decoded peg-out destination does not match requested address"
[[ "$PEGOUT_OUTPUT_DESTINATION_SCRIPT" == "$PEGOUT_DESTINATION_SCRIPT" ]] || fail \
    "decoded peg-out script does not match requested destination script"

readonly PEGOUT_FEE_SATS="$(jq -er --arg asset "$PEGGED_ASSET" '((.fee[$asset] // .fees[$asset] // 0) * 100000000 | round)' <<<"$PEGOUT_DECODED")"
readonly PEGOUT_OUTPUT_INDEX="$(jq -er --arg genesis "$SIDECHAIN_PARENT_GENESIS" 'first(.vout | to_entries[] | select(.value.scriptPubKey.type == "nulldata" and .value.scriptPubKey.pegout_chain == $genesis) | .key)' <<<"$PEGOUT_DECODED")"

jq -n \
    --arg operation peg_in \
    --arg network elementsregtest \
    --arg genesis "$SIDECHAIN_PARENT_GENESIS" \
    --arg parent_txid "$PARENT_TXID" \
    --arg parent_block_hash "$PARENT_BLOCK_HASH" \
    --arg parent_raw_tx "$PARENT_RAW_TX" \
    --arg parent_txoutproof "$PARENT_TXOUT_PROOF" \
    --argjson parent_confirmations "$PARENT_CONFIRMATIONS" \
    --arg claim_txid "$CLAIM_TXID" \
    --arg claim_block_hash "$CLAIM_BLOCK_HASH" \
    --argjson claim_confirmations "$CLAIM_CONFIRMATIONS" \
    --arg claim_script "$CLAIM_SCRIPT" \
    --argjson pegin_witness "$CLAIM_WITNESS" \
    --arg asset "$PEGGED_ASSET" \
    --argjson claim_amount_sats "$CLAIM_AMOUNT_SATS" \
    --argjson claim_fee_sats "$CLAIM_FEE_SATS" \
    --argjson claim_reconciled_sats "$CLAIM_RECONCILED_SATS" \
    --argjson outputs "$CLAIM_OUTPUTS" \
    '{
        operation: $operation,
        network: $network,
        parent: {
            genesis_hash: $genesis,
            txid: $parent_txid,
            block_hash: $parent_block_hash,
            raw_tx: $parent_raw_tx,
            txoutproof: $parent_txoutproof,
            confirmations: $parent_confirmations
        },
        peg_in: {
            txid: $claim_txid,
            block_hash: $claim_block_hash,
            confirmations: $claim_confirmations,
            parent_txid: $parent_txid,
            parent_genesis_hash: $genesis,
            is_pegin: true,
            peg_in_marker: true,
            claim_script: $claim_script,
            pegin_witness: $pegin_witness,
            pegged_asset: $asset,
            amount_sats: $claim_amount_sats,
            fee_sats: $claim_fee_sats,
            reconciled_sats: $claim_reconciled_sats,
            output_amount_sats: ($outputs[0].value_sats // null),
            proof_components: {
                raw_parent_tx: true,
                txoutproof: true,
                claim_script: true,
                parent_genesis: true,
                pegin_witness: true
            }
        },
        outputs: $outputs
    }' >"${ARTIFACT_DIR}/peg-in-proof.json"

jq -n \
    --arg operation peg_out \
    --arg network elementsregtest \
    --arg genesis "$SIDECHAIN_PARENT_GENESIS" \
    --arg txid "$PEGOUT_TXID" \
    --arg block_hash "$PEGOUT_BLOCK_HASH" \
    --argjson confirmations "$PEGOUT_CONFIRMATIONS" \
    --arg destination_address "$PEGOUT_DESTINATION" \
    --arg destination_script "$PEGOUT_DESTINATION_SCRIPT" \
    --arg asset "$PEGGED_ASSET" \
    --argjson amount_sats "$PEGOUT_OUTPUT_AMOUNT_SATS" \
    --argjson fee_sats "$PEGOUT_FEE_SATS" \
    --argjson burn_output true \
    --argjson output_index "$PEGOUT_OUTPUT_INDEX" \
    '{
        operation: $operation,
        network: $network,
        parent: {genesis_hash: $genesis},
        peg_out: {
            txid: $txid,
            block_hash: $block_hash,
            confirmations: $confirmations,
            confirmed: true,
            parent_genesis_hash: $genesis,
            destination_address: $destination_address,
            destination_script: $destination_script,
            amount_sats: $amount_sats,
            asset: $asset,
            fee_sats: $fee_sats,
            burn_output: $burn_output,
            output_index: $output_index
        },
        outputs: [{visibility: "explicit", value_sats: $amount_sats, asset: $asset}]
    }' >"${ARTIFACT_DIR}/peg-out-proof.json"

jq -n \
    --arg claim_txid "$CLAIM_TXID" \
    --arg ct_txid "$CT_TXID" \
    --arg pegout_txid "$PEGOUT_TXID" \
    --arg parent_genesis "$SIDECHAIN_PARENT_GENESIS" \
    --arg pegged_asset "$PEGGED_ASSET" \
    --argjson parent_confirmations "$PARENT_CONFIRMATIONS" \
    --argjson claim_confirmations "$CLAIM_CONFIRMATIONS" \
    --argjson pegout_confirmations "$PEGOUT_CONFIRMATIONS" \
    --argjson pegout_fee_sats "$PEGOUT_FEE_SATS" \
    --argjson pegin_confirmation_depth "$PEGIN_CONFIRMATION_DEPTH" \
    '{
        status: "passed",
        parent_genesis_hash: $parent_genesis,
        pegged_asset: $pegged_asset,
        transactions: {
            parent_peg_in: $claim_txid,
            confidential_transfer: $ct_txid,
            peg_out: $pegout_txid
        },
        confirmations: {
            parent_peg_in: $parent_confirmations,
            liquid_claim: $claim_confirmations,
            liquid_peg_out: $pegout_confirmations
        },
        peg_out_fee_sats: $pegout_fee_sats,
        pegin_confirmation_depth: $pegin_confirmation_depth,
        assertions: [
            "Elements getsidechaininfo matched the live parent genesis and pegged asset",
            "Elements getsidechaininfo exposed the live peg-in policy and the parent proof reached the 102-confirmation target",
            "Bitcoin parent coinbase was matured before the peg-in",
            "Elements claimpegin accepted the parent proof and claim under Elements consensus",
            "Elements claim includes the peg-in marker, parent txid, raw tx, txoutproof, claim script, and witness",
            "Decoded pegged-asset claim output(s) plus the Elements fee output reconciled to exactly 100000000 sats",
            "Duplicate claim was rejected as already claimed",
            "Blinded transfer exposed commitments, rangeproof, and surjection proof fields",
            "Decoded peg-out output encoded parent genesis, destination address/script, amount, asset, and fee"
        ],
        non_claims: [
            "This harness does not enable or test production LiquidAdapter state-proof verification",
            "This harness does not claim CT cryptographic proof validation beyond real daemon transaction acceptance and decoded commitments",
            "This harness does not test automatic Bitcoin release",
            "This harness does not test Watchmen batching",
            "This harness does not test federation quorum",
            "This harness does not test PAK policy",
            "This harness does not test production timing or production federation coverage"
        ]
    }' >"${ARTIFACT_DIR}/e2e-summary.json"

printf 'Liquid E2E passed; artifacts: %s\n' "$ARTIFACT_DIR"
