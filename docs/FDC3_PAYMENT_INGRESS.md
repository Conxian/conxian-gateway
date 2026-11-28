# FDC3 payment ingress

`POST /api/v1/fdc3/payment` and `POST /api/v1/ingress/fdc3/payment` translate a
standard FDC3 `fdc3.payment` context into the Gateway's bounded ISO 20022
`pacs.008` JSON profile and return a typed `fdc3.transactionResult`.

This endpoint is **translation only**. It does not create a settlement proposal,
move value, verify a hardware attestation, or assert settlement finality. The
existing `POST /api/v1/ingress/iso20022` path remains the signed, attested
settlement ingress.

## Request

Bearer authentication with the `write:settlement` scope is required.

```json
{
  "type": "fdc3.payment",
  "id": { "UETR": "123e4567-e89b-12d3-a456-426614174000" },
  "amount": 125.55,
  "currency": "ZAR",
  "debtor": {
    "name": "Originating Bank",
    "account": "ZA12345678901234567890",
    "agent": "BUKBGB22"
  },
  "creditor": {
    "name": "Receiving Bank",
    "account": "ZA09876543210987654321",
    "agent": "CHASUS33"
  },
  "networkRouting": { "settlementDate": "2026-10-13" }
}
```

`type`, `amount`, `currency`, `id`, `debtor.account`, and `creditor.account`
are required. `amount` must be a finite positive number, `currency` a
three-letter uppercase ISO 4217 code, and identifiers and text fields are
bounded. Invalid input returns `400` with a `Failed` transaction result.

## Response

```json
{
  "type": "fdc3.transactionResult",
  "status": "Created",
  "context": {
    "type": "conxian.iso20022.routing",
    "version": "1.0",
    "payload": {
      "source": "ISO20022_PACS008",
      "transaction_id": "123e4567-e89b-12d3-a456-426614174000",
      "amount_minor": 12555,
      "amount_scale": 2,
      "currency": "ZAR",
      "finality": "UNKNOWN"
    }
  },
  "message": "Translated fdc3.payment to ISO 20022 pacs.008 routing envelope; no settlement was executed."
}
```

`status: "Created"` means the routing envelope was created, not that a payment
was settled. The `context` uses opaque debtor and creditor references; raw
party names are not returned.

## Connector integration

The Cloudflare FDC3 Connector forwards `StartPayment` to this route when both
secrets are configured:

- `CONXIAN_GATEWAY_FDC3_PAYMENT_URL` —
  `https://<gateway>/api/v1/ingress/fdc3/payment`
- `CONXIAN_GATEWAY_FDC3_PAYMENT_TOKEN` — a `write:settlement` bearer token

Without the URL the connector fails closed with `501` and a `Failed`
transaction result.
