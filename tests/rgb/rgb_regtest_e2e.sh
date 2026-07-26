#!/usr/bin/env bash
set -euo pipefail

# Opt-in proof lane for a real RGB v0.12 state transition. Bitcoin Core funds,
# signs, accepts, broadcasts and mines the witness transaction. All mutable
# state, wallets, consignments and proof output stay under target/ or an
# ephemeral temporary directory. Bitcoin Core's cookie credential never leaves
# the isolated temporary datadir.

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
readonly RPC_PORT="${RGB_REGTEST_RPC_PORT:-18998}"
readonly P2P_PORT="${RGB_REGTEST_P2P_PORT:-18999}"
readonly WALLET="rgb-regtest"
readonly RUN_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/conxian-rgb-regtest.XXXXXX")"
readonly DATA_DIR="${RUN_ROOT}/bitcoin"
readonly COOKIE_FILE="${DATA_DIR}/regtest/.cookie"
readonly ARTIFACT_PARENT="${REPO_ROOT}/target/rgb-regtest-artifacts"

BITCOIND_PID=""
ARTIFACT_DIR=""
RPC_USER=""
RPC_PASSWORD=""
ARTIFACTS_FINALIZED=0
ARTIFACT_GUARD_FAILED=0

fail() {
    printf 'RGB regtest failure: %s\n' "$*" >&2
    exit 1
}

stop_bitcoind() {
    if [[ -n "$BITCOIND_PID" ]] && kill -0 "$BITCOIND_PID" 2>/dev/null; then
        btc stop >/dev/null 2>&1
        wait "$BITCOIND_PID" 2>/dev/null
    fi
    BITCOIND_PID=""
}

load_cookie_credentials() {
    local cookie=""
    [[ -f "$COOKIE_FILE" && ! -L "$COOKIE_FILE" ]] || return 1
    IFS= read -r cookie <"$COOKIE_FILE" || [[ -n "$cookie" ]] || return 1
    [[ "$cookie" == *:* ]] || return 1
    RPC_USER="${cookie%%:*}"
    RPC_PASSWORD="${cookie#*:}"
    [[ -n "$RPC_USER" && -n "$RPC_PASSWORD" ]]
}

guard_retained_artifacts() {
    local file=""
    local pattern_file="${RUN_ROOT}/artifact-secret.pattern"
    local unsafe=0

    if [[ -n "$RPC_PASSWORD" ]]; then
        (umask 077 && printf '%s\n' "$RPC_PASSWORD" >"$pattern_file")
        while IFS= read -r -d '' file; do
            if grep -Fq -f "$pattern_file" "$file"; then
                rm -f -- "$file"
                unsafe=1
            fi
        done < <(find "$ARTIFACT_DIR" -type f -print0)
        rm -f -- "$pattern_file"
    fi

    while IFS= read -r -d '' file; do
        rm -rf -- "$file"
        unsafe=1
    done < <(find "$ARTIFACT_DIR" -name .cookie -print0)

    if (( unsafe != 0 )); then
        printf '%s\n' 'failed: unsafe credential-bearing artifact removed' \
            >"${ARTIFACT_DIR}/credential-leak-guard.txt"
        return 1
    fi

    printf '%s\n' 'passed: cookie secret and cookie file absent' \
        >"${ARTIFACT_DIR}/credential-leak-guard.txt"
}

finalize_artifacts() {
    local status="$1"
    if (( ARTIFACTS_FINALIZED != 0 )); then
        (( ARTIFACT_GUARD_FAILED == 0 ))
        return
    fi

    if [[ -n "$ARTIFACT_DIR" && -f "${DATA_DIR}/regtest/debug.log" ]]; then
        cp -- "${DATA_DIR}/regtest/debug.log" "${ARTIFACT_DIR}/bitcoin-debug.log"
    fi
    if [[ -z "$RPC_PASSWORD" && -f "$COOKIE_FILE" ]]; then
        load_cookie_credentials || true
    fi
    if [[ -n "$ARTIFACT_DIR" ]] && ! guard_retained_artifacts; then
        ARTIFACT_GUARD_FAILED=1
        status=1
    fi
    if [[ -n "$ARTIFACT_DIR" ]]; then
        printf '%s\n' "$status" >"${ARTIFACT_DIR}/exit-status"
    fi
    ARTIFACTS_FINALIZED=1
    (( ARTIFACT_GUARD_FAILED == 0 ))
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    set +e
    stop_bitcoind
    if ! finalize_artifacts "$status"; then
        status=1
        printf '%s\n' 'RGB regtest failure: retained artifact credential leak detected; unsafe artifact removed' >&2
    fi
    rm -rf -- "$RUN_ROOT"
    if [[ -n "$ARTIFACT_DIR" ]]; then
        printf 'RGB regtest artifacts: %s\n' "$ARTIFACT_DIR"
    fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

for command_name in cargo find grep jq mktemp seq; do
    command -v "$command_name" >/dev/null 2>&1 || fail "required command not found: ${command_name}"
done
[[ ! -L "${REPO_ROOT}/target" ]] || fail "repo target directory must not be a symlink"
mkdir -p "$ARTIFACT_PARENT"
[[ -d "$ARTIFACT_PARENT" && ! -L "$ARTIFACT_PARENT" && -O "$ARTIFACT_PARENT" ]] || \
    fail "artifact parent must be an owned, non-symlink directory"
ARTIFACT_DIR="$(mktemp -d "${ARTIFACT_PARENT}/run.XXXXXX")"
readonly ARTIFACT_DIR
[[ "$RPC_PORT" =~ ^[0-9]+$ && "$P2P_PORT" =~ ^[0-9]+$ ]] || fail "ports must be decimal integers"
(( 10#$RPC_PORT >= 1024 && 10#$RPC_PORT <= 65535 )) || fail "RPC port out of range"
(( 10#$P2P_PORT >= 1024 && 10#$P2P_PORT <= 65535 )) || fail "P2P port out of range"
[[ "$RPC_PORT" != "$P2P_PORT" ]] || fail "RPC and P2P ports must differ"

DAEMON_ROOT="$(bash "${SCRIPT_DIR}/install_bitcoin_core.sh")"
readonly BITCOIND="${DAEMON_ROOT}/bitcoin-31.1/bin/bitcoind"
readonly BITCOIN_CLI="${DAEMON_ROOT}/bitcoin-31.1/bin/bitcoin-cli"
mkdir -p "$DATA_DIR"

btc() {
    "$BITCOIN_CLI" -regtest -datadir="$DATA_DIR" -rpcconnect=127.0.0.1 \
        -rpcport="$RPC_PORT" "$@"
}

wallet() {
    btc -rpcwallet="$WALLET" "$@"
}

"$BITCOIND" -regtest -datadir="$DATA_DIR" -server=1 -daemon=0 -listen=0 \
    -discover=0 -dnsseed=0 -txindex=1 -fallbackfee=0.0002 -port="$P2P_PORT" \
    -rpcbind=127.0.0.1 -rpcallowip=127.0.0.1 -rpcport="$RPC_PORT" \
    >"${ARTIFACT_DIR}/bitcoin-stdout.log" 2>&1 &
BITCOIND_PID=$!

for _ in $(seq 1 120); do
    btc getblockchaininfo >/dev/null 2>&1 && break
    sleep 0.25
done
btc getblockchaininfo >/dev/null 2>&1 || fail "Bitcoin Core RPC did not become ready"
load_cookie_credentials || fail "Bitcoin Core cookie credential was not available"
btc createwallet "$WALLET" >/dev/null

MINING_ADDRESS="$(wallet getnewaddress mining bech32)"
wallet generatetoaddress 101 "$MINING_ADDRESS" >/dev/null
GENESIS_ADDRESS="$(wallet getnewaddress rgb-genesis bech32)"
GENESIS_TXID="$(wallet sendtoaddress "$GENESIS_ADDRESS" 1.0)"
wallet generatetoaddress 1 "$MINING_ADDRESS" >/dev/null
GENESIS_TX="$(btc getrawtransaction "$GENESIS_TXID" true)"
GENESIS_VOUT="$(jq -r --arg address "$GENESIS_ADDRESS" '.vout[] | select(.scriptPubKey.address == $address) | .n' <<<"$GENESIS_TX")"
[[ "$GENESIS_VOUT" =~ ^[0-9]+$ ]] || fail "could not locate the real genesis UTXO"

export RGB_REGTEST_RPC_URL="http://127.0.0.1:${RPC_PORT}/wallet/${WALLET}"
export RGB_REGTEST_RPC_USER="$RPC_USER"
export RGB_REGTEST_RPC_PASSWORD="$RPC_PASSWORD"
export RGB_REGTEST_GENESIS_TXID="$GENESIS_TXID"
export RGB_REGTEST_GENESIS_VOUT="$GENESIS_VOUT"
export RGB_REGTEST_RECEIVER_ADDRESS="$(wallet getnewaddress rgb-receiver bech32)"
export RGB_REGTEST_CHANGE_ADDRESS="$(wallet getnewaddress rgb-change bech32)"
export RGB_REGTEST_MINING_ADDRESS="$MINING_ADDRESS"
export RGB_REGTEST_ARTIFACT_DIR="$ARTIFACT_DIR"

cd "$REPO_ROOT"
cargo test -p conxian_engine --test rgb_regtest_transition --features rgb-native \
    -- --ignored --exact bitcoin_core_signed_mined_rgb_transition_is_durable_and_fail_closed --nocapture

jq -e \
    '.receiver_amount == 40 and .receiver_vout == 0 and
     .change_amount == 60 and .change_vout == 1 and
     .durability == "verified_after_reopen" and
     .negative_bad_signature == "rejected_without_state_mutation" and
     .negative_wrong_bitcoin_commitment == "rejected_without_state_mutation"' \
    "${ARTIFACT_DIR}/proof.json" >/dev/null || fail "proof artifact is incomplete"

stop_bitcoind
finalize_artifacts 0 || fail "retained artifact credential leak detected; unsafe artifact removed"
