#!/usr/bin/env bash
# Reference SOVEREIGN deploy: build the released source and install it on a
# client- or operator-managed host running the gateway as a bare binary under
# systemd. This is the self-managed lane described in docs/DEPLOY_MODES.md.
#
# It is NOT the managed production path. The managed gateway (Render
# srv-<redacted> -> https://gateway.conxian.org) is deployed by
# .github/workflows/deploy.yml. Never point a public hostname at a sovereign
# reference host.
#
# Usage:
#   scripts/deploy-sovereign.sh --host <ip-or-host> [--user root] [--ref origin/main] [--dry-run]
#
# Requires: cargo, ssh, scp on the build host; the operator's SSH key authorized
# on the target. Secrets stay on the target in /etc/conxian-gateway.env.
set -euo pipefail

HOST=""
USER="root"
REF="origin/main"
DRY_RUN=0
BIN_NAME="gateway"
REMOTE_BIN="/usr/local/bin/conxian-gateway"
UNIT="conxian-gateway"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host) HOST="${2:-}"; shift 2 ;;
    --user) USER="${2:-}"; shift 2 ;;
    --ref) REF="${2:-}"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ -n "${HOST}" ]] || { echo "error: --host is required" >&2; exit 2; }

echo "== sovereign deploy =="
echo "target : ${USER}@${HOST}"
echo "ref    : ${REF}"
echo "unit   : ${UNIT} (${REMOTE_BIN}, :3000)"

# 1. Build the exact released source.
git fetch --tags origin
git checkout --detach "${REF}"
cargo build --release --locked --bin "${BIN_NAME}"

STAMP="$(date +%s)"
if [[ "${DRY_RUN}" == "1" ]]; then
  echo "dry-run: would scp target/release/${BIN_NAME} -> ${USER}@${HOST}:/tmp/conxian-gateway.new"
  echo "dry-run: would back up ${REMOTE_BIN} -> ${REMOTE_BIN}.old.${STAMP} and restart ${UNIT}"
  exit 0
fi

# 2. Ship and swap atomically, keeping the previous binary for rollback.
scp "target/release/${BIN_NAME}" "${USER}@${HOST}:/tmp/conxian-gateway.new"
ssh "${USER}@${HOST}" "
  set -euo pipefail
  cp -a '${REMOTE_BIN}' '${REMOTE_BIN}.old.${STAMP}' 2>/dev/null || true
  chmod 755 /tmp/conxian-gateway.new
  mv /tmp/conxian-gateway.new '${REMOTE_BIN}'
  systemctl restart '${UNIT}'
"

# 3. Liveness smoke (sovereign hosts are private; probe locally).
for spec in "health" "version"; do
  code="$(ssh "${USER}@${HOST}" "curl -s -o /dev/null -w '%{http_code}' 'http://localhost:3000/api/v1/${spec}'")"
  echo "/api/v1/${spec} -> ${code}"
  [[ "${code}" == "200" ]] || { echo "error: /api/v1/${spec} returned ${code}" >&2; exit 1; }
done

echo "rollback: ssh ${USER}@${HOST} 'mv ${REMOTE_BIN}.old.${STAMP} ${REMOTE_BIN} && systemctl restart ${UNIT}'"
