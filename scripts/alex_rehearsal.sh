#!/bin/bash

GATEWAY_URL=${1:-"http://localhost:3000"}
API_TOKEN=${2:-"test-token"}

echo "Running ALEX Deployment Readiness Rehearsal (CON-136)..."

echo "1. Requesting ALEX Quote for sBTC -> STX..."
QUOTE_RES=$(curl -s -G "$GATEWAY_URL/api/v1/alex/quote"   --data-urlencode "token_x=sBTC"   --data-urlencode "token_y=STX"   --data-urlencode "amount=100000000"   --data-urlencode "factor=100000000"   -H "Authorization: Bearer $API_TOKEN")

echo "Quote Result: $QUOTE_RES"

echo "2. Executing Rehearsal ALEX Swap..."
SWAP_RES=$(curl -s -X POST "$GATEWAY_URL/api/v1/alex/swap"   -H "Authorization: Bearer $API_TOKEN"   -H "Content-Type: application/json"   -d '{"token_x": "sBTC", "token_y": "STX", "amount": 1000, "factor": 1000}')

echo "Swap Result: $SWAP_RES"

echo "ALEX Rehearsal complete!"
