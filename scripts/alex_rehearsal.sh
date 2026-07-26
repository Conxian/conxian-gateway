#!/bin/bash
set -euo pipefail

GATEWAY_URL=${1:-"http://localhost:3000"}
API_TOKEN=${2:-"test-token"}
: "${ALEX_ASSET_IN:?Set ALEX_ASSET_IN to an exact network-qualified principal}"
: "${ALEX_ASSET_OUT:?Set ALEX_ASSET_OUT to an exact network-qualified principal}"

request() {
  local method=$1
  local path=$2
  local body=${3:-}
  local output
  output=$(mktemp "${TMPDIR:-/tmp}/alex_rehearsal.XXXXXX")
  local args=(-s -o "$output" -w "%{http_code}" -X "$method" "$GATEWAY_URL$path"
    -H "Authorization: Bearer $API_TOKEN"
    -H "x-402-payment: rehearsal-proof")
  if [ -n "$body" ]; then
    args+=(-H "Content-Type: application/json" -d "$body")
  fi
  HTTP_CODE=$(curl "${args[@]}")
  HTTP_BODY=$(cat "$output")
  rm -f "$output"
}

echo "Running ALEX read-only/policy-gate rehearsal..."

echo "1. Requesting the explicitly unverified compatibility quote..."
QUOTE=$(printf '/api/v1/alex/quote?token_x=%s&token_y=%s&amount=1000' \
  "$ALEX_ASSET_IN" "$ALEX_ASSET_OUT")
request GET "$QUOTE"
echo "Quote HTTP status: $HTTP_CODE"
echo "Quote result: $HTTP_BODY"

PAYLOAD=$(printf '{"token_x":"%s","token_y":"%s","factor":100000000,"amount":1000,"min_dy":1}' \
  "$ALEX_ASSET_IN" "$ALEX_ASSET_OUT")

echo "2. Exercising policy-gated unsigned preparation..."
request POST "/api/v1/alex/prepare" "$PAYLOAD"
echo "Prepare HTTP status: $HTTP_CODE"
echo "Prepare result: $HTTP_BODY"
if [ "$HTTP_CODE" -lt 400 ]; then
  echo "Unexpected preparation success with the production unverified quote adapter" >&2
  exit 1
fi

echo "3. Confirming legacy execution remains disabled..."
request POST "/api/v1/alex/swap" "$PAYLOAD"
echo "Swap HTTP status: $HTTP_CODE"
echo "Swap result: $HTTP_BODY"
if [ "$HTTP_CODE" != "409" ] || ! printf '%s' "$HTTP_BODY" | grep -q 'ALEX_EXECUTION_DISABLED'; then
  echo "Legacy /alex/swap did not return stable ALEX_EXECUTION_DISABLED" >&2
  exit 1
fi

echo "ALEX rehearsal completed without signing or broadcast."
