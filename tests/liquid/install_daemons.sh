#!/usr/bin/env bash
set -euo pipefail

# Install the exact daemon archives used by the opt-in Liquid harness.  The
# archive is verified before extraction; no daemon is installed system-wide.

readonly BTC_VERSION="31.1"
readonly ELEMENTS_VERSION="23.3.3"
readonly BTC_URL_BASE="https://bitcoincore.org/bin/bitcoin-core-${BTC_VERSION}"
readonly ELEMENTS_URL_BASE="https://github.com/ElementsProject/elements/releases/download/elements-${ELEMENTS_VERSION}"

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
readonly CACHE_DIR="${LIQUID_DAEMON_CACHE_DIR:-/tmp/conxian-regtest-bins}"
readonly INSTALL_DIR="${LIQUID_DAEMON_INSTALL_DIR:-${REPO_ROOT}/target/liquid-daemons}"

case "$(uname -m)" in
    x86_64)
        readonly ARCH="x86_64"
        readonly BTC_SHA256="b80d9c3e04da78fb6f0569685673418cf686fadba9042d926d13fb87ff503f9e"
        readonly ELEMENTS_SHA256="90d6659a4f5d6d94bbf2321f6114e1286fbec8031cfc614b2f2319ddfcd9b3e1"
        ;;
    aarch64|arm64)
        readonly ARCH="aarch64"
        readonly BTC_SHA256="dcf1873f2208ba4f962f3398d47e154c39c0084be8f4553e05c940d0ace3d004"
        readonly ELEMENTS_SHA256="279c6cf96ca0583e93fa8531ca671ffde91694254fce4719e6f3b1d0d883dd34"
        ;;
    *)
        printf 'unsupported architecture: %s\n' "$(uname -m)" >&2
        exit 1
        ;;
esac

need_command() {
    command -v "$1" >/dev/null 2>&1 || {
        printf 'required command not found: %s\n' "$1" >&2
        exit 1
    }
}

need_command curl
need_command sha256sum
need_command tar

find_cached_archive() {
    local kind="$1"
    local version="$2"
    local candidate
    shift 2

    for candidate in "$@"; do
        if [[ -f "${CACHE_DIR}/${candidate}" ]]; then
            printf '%s\n' "${CACHE_DIR}/${candidate}"
            return 0
        fi
    done

    local archive="${CACHE_DIR}/${kind}-${version}-${ARCH}-linux-gnu.tar.gz"
    if [[ -f "$archive" ]]; then
        printf '%s\n' "$archive"
        return 0
    fi

    return 1
}

fetch_and_verify() {
    local name="$1"
    local url="$2"
    local expected_sha256="$3"
    local archive_path="$4"

    mkdir -p "$(dirname -- "$archive_path")"
    if [[ ! -f "$archive_path" ]]; then
        printf 'Downloading %s from official release URL\n' "$name" >&2
        curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
            --output "$archive_path" "$url"
    fi

    printf '%s  %s\n' "$expected_sha256" "$archive_path" | sha256sum --check --status - || {
        printf 'SHA256 verification failed for %s: %s\n' "$name" "$archive_path" >&2
        rm -f -- "$archive_path"
        exit 1
    }
}

mkdir -p "$CACHE_DIR"
mkdir -p "$(dirname -- "$INSTALL_DIR")"

btc_archive="$(find_cached_archive bitcoin "$BTC_VERSION" \
    bitcoin.tar.gz \
    bitcoin-core-${BTC_VERSION}-${ARCH}-linux-gnu.tar.gz \
    bitcoin-${BTC_VERSION}-${ARCH}-linux-gnu.tar.gz || true)"
if [[ -z "$btc_archive" ]]; then
    btc_archive="${CACHE_DIR}/bitcoin-core-${BTC_VERSION}-${ARCH}-linux-gnu.tar.gz"
fi
fetch_and_verify \
    "Bitcoin Core ${BTC_VERSION}" \
    "${BTC_URL_BASE}/bitcoin-${BTC_VERSION}-${ARCH}-linux-gnu.tar.gz" \
    "$BTC_SHA256" \
    "$btc_archive"

elements_archive="$(find_cached_archive elements "$ELEMENTS_VERSION" \
    elements.tar.gz \
    elements-${ELEMENTS_VERSION}-${ARCH}-linux-gnu.tar.gz || true)"
if [[ -z "$elements_archive" ]]; then
    elements_archive="${CACHE_DIR}/elements-${ELEMENTS_VERSION}-${ARCH}-linux-gnu.tar.gz"
fi
fetch_and_verify \
    "Elements Core ${ELEMENTS_VERSION}" \
    "${ELEMENTS_URL_BASE}/elements-${ELEMENTS_VERSION}-${ARCH}-linux-gnu.tar.gz" \
    "$ELEMENTS_SHA256" \
    "$elements_archive"

rm -rf -- "$INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
tar --extract --gzip --file "$btc_archive" --directory "$INSTALL_DIR"
tar --extract --gzip --file "$elements_archive" --directory "$INSTALL_DIR"

readonly BITCOIND="${INSTALL_DIR}/bitcoin-${BTC_VERSION}/bin/bitcoind"
readonly BITCOIN_CLI="${INSTALL_DIR}/bitcoin-${BTC_VERSION}/bin/bitcoin-cli"
readonly ELEMENTSD="${INSTALL_DIR}/elements-${ELEMENTS_VERSION}/bin/elementsd"
readonly ELEMENTS_CLI="${INSTALL_DIR}/elements-${ELEMENTS_VERSION}/bin/elements-cli"

for executable in "$BITCOIND" "$BITCOIN_CLI" "$ELEMENTSD" "$ELEMENTS_CLI"; do
    if [[ ! -x "$executable" ]]; then
        printf 'expected daemon executable missing after extraction: %s\n' "$executable" >&2
        exit 1
    fi
done

"$BITCOIND" --version | grep -F "Bitcoin Core daemon version v${BTC_VERSION}" >/dev/null || {
    printf 'unexpected Bitcoin Core version\n' >&2
    exit 1
}
"$ELEMENTSD" --version | grep -F "${ELEMENTS_VERSION}" >/dev/null || {
    printf 'unexpected Elements Core version\n' >&2
    exit 1
}

# Keep stdout machine-readable so callers can capture the private install root.
printf '%s\n' "$INSTALL_DIR"
