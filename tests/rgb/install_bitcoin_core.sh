#!/usr/bin/env bash
set -euo pipefail

# Install the exact Bitcoin Core archive used by the opt-in RGB regtest lane.
# The archive is checksum-verified and extracted only below target/.

readonly BTC_VERSION="31.1"
readonly BTC_URL_BASE="https://bitcoincore.org/bin/bitcoin-core-${BTC_VERSION}"
readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
readonly CACHE_DIR="${REPO_ROOT}/target/rgb-daemon-cache"
readonly INSTALL_DIR="${REPO_ROOT}/target/rgb-daemons"

fail() {
    printf 'RGB daemon installer failure: %s\n' "$*" >&2
    exit 1
}

for command_name in curl sha256sum tar; do
    command -v "$command_name" >/dev/null 2>&1 || fail "required command not found: ${command_name}"
done

[[ ! -L "${REPO_ROOT}/target" ]] || fail "repo target directory must not be a symlink"

case "$(uname -m)" in
    x86_64)
        readonly ARCH="x86_64"
        readonly BTC_SHA256="b80d9c3e04da78fb6f0569685673418cf686fadba9042d926d13fb87ff503f9e"
        ;;
    aarch64|arm64)
        readonly ARCH="aarch64"
        readonly BTC_SHA256="dcf1873f2208ba4f962f3398d47e154c39c0084be8f4553e05c940d0ace3d004"
        ;;
    *) fail "unsupported architecture: $(uname -m)" ;;
esac

mkdir -p -- "$CACHE_DIR" "$INSTALL_DIR"
[[ ! -L "$CACHE_DIR" && ! -L "$INSTALL_DIR" && -O "$CACHE_DIR" && -O "$INSTALL_DIR" ]] || \
    fail "target cache and install directories must be owned, non-symlink directories"

readonly ARCHIVE="${CACHE_DIR}/bitcoin-${BTC_VERSION}-${ARCH}-linux-gnu.tar.gz"
if [[ ! -f "$ARCHIVE" ]]; then
    curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
        --output "$ARCHIVE" \
        "${BTC_URL_BASE}/bitcoin-${BTC_VERSION}-${ARCH}-linux-gnu.tar.gz"
fi
[[ -f "$ARCHIVE" && ! -L "$ARCHIVE" && -O "$ARCHIVE" ]] || fail "unsafe cached archive"
printf '%s  %s\n' "$BTC_SHA256" "$ARCHIVE" | sha256sum --check --status - || {
    rm -f -- "$ARCHIVE"
    fail "Bitcoin Core archive checksum mismatch"
}

rm -rf -- "${INSTALL_DIR}/bitcoin-${BTC_VERSION}"
tar --extract --gzip --file "$ARCHIVE" --directory "$INSTALL_DIR"
readonly BITCOIND="${INSTALL_DIR}/bitcoin-${BTC_VERSION}/bin/bitcoind"
readonly BITCOIN_CLI="${INSTALL_DIR}/bitcoin-${BTC_VERSION}/bin/bitcoin-cli"
[[ -x "$BITCOIND" && -x "$BITCOIN_CLI" ]] || fail "Bitcoin Core executables missing"
"$BITCOIND" --version | grep -F "Bitcoin Core daemon version v${BTC_VERSION}" >/dev/null || \
    fail "unexpected Bitcoin Core version"

printf '%s\n' "$INSTALL_DIR"
