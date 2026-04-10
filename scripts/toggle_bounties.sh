#!/bin/bash

GATEWAY_URL=${1:-"http://localhost:3000"}
API_TOKEN=${2:-"test-token"}
ENABLED=${3:-"true"}

echo "Toggling Bounty Payouts (CON-230)..."
curl -s -X POST "$GATEWAY_URL/api/v1/bounties/payouts/toggle"   -H "Authorization: Bearer $API_TOKEN"   -H "Content-Type: application/json"   -d "$ENABLED"

echo -e "\nBounty toggle complete."
