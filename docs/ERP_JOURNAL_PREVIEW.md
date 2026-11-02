# Evidence-scoped ERP journal preview

The Gateway's existing camt.053/camt.054 generation and OData callback are separate
from a general-ledger journal posting. Sending XML/JSON, receiving HTTP 2xx, or
authenticating an ERP request does not establish a posted, balanced journal in a
named SAP/Oracle/NetSuite tenant. This client-side preview validates accounting
inputs before a later authorized connector handles that boundary.

## Current implementation

`@conxian/client-sdk` exports `prepareJournalPreview(unknown)`. It runs offline,
checks `conxian-journal-preview/1`, and returns a deeply immutable document with
exact debit/credit totals, a scoped external ID and SHA-256 payload digest.
It requires explicit organization, ledger and event IDs, posting date, accounting
currency/scale, valuation evidence digest, source revision and asset quantity,
and at least two balanced journal lines. Asset quantities and accounting amounts
are decimal strings; no floating-point conversion, automatic exchange rate,
account selection or fee split occurs.

Source quantities use their own declared asset decimals. Journal lines use the
separate accounting currency's declared minor-unit scale. The two numbers need
not match: valuing BTC in a company's accounting currency requires independent
valuation evidence and accounting policy. The preview stores the submitted
valuation digest without verifying it.

```ts
import { prepareJournalPreview } from "@conxian/client-sdk";

// request is a caller-supplied JSON document matching the contract below.
const preview = await prepareJournalPreview(request);
console.log(preview.totals, preview.external_id, preview.payload_sha256);
// posting_authorized, source_evidence_verified and valuation_verified are false.
```

## Version 1 contract

| Field | Meaning |
| --- | --- |
| `schema_version` | Exactly `conxian-journal-preview/1` |
| `organization_id`, `ledger_id`, `event_id` | Explicit caller scope; not tenant authentication |
| `accounting_date` | Valid calendar date, `YYYY-MM-DD` |
| `currency`, `currency_decimals` | Three uppercase letters and declared scale; syntax only, not ISO registration or ERP currency mapping |
| `valuation_evidence_sha256` | Lowercase SHA-256 reference to caller valuation evidence; unverified |
| `source.repository`, `source.revision` | Producer repository and immutable 40-character revision claim |
| `source.network`, `source.asset` | Matching CAIP-2 / CAIP-19 syntax; no proof of registered asset, supported operation or namespace-specific canonicalization |
| `source.quantity_atomic`, `source.asset_decimals` | Positive atomic movement quantity and declared scale, separate from journal amounts |
| `source.transaction_id`, `source.observed_at` | Caller transaction/observation references; timestamp is canonical UTC with milliseconds |
| `source.evidence_sha256` | Lowercase SHA-256 reference; a digest does not establish a signature, Merkle root, finality or hardware identity |
| `source.settlement_status` | Caller claim: `unknown`, `pending`, `confirmed` or `reorged`; none authorizes posting |
| `lines` | Ordered `{ account_id, side: debit/credit, amount_minor }` records; positive canonical decimal strings |

The bounded profile accepts at most 64 KiB, 256 lines, 78-digit amounts and
decimal scales 0–38. IDs are bounded and control characters, unknown fields,
non-JSON values, invalid dates, mismatched assets and imbalance are rejected.
These are local input limits, not universal ERP/asset limits. Negative postings,
zero-value revaluations, tax, FX, cost-basis policy and vendor-specific fields
need a separately versioned and qualified mapping.

Object key order is normalized; line order is retained. The external ID hashes
the version and organization/ledger/event tuple. The payload digest binds every
normalized field. Exact duplicates have the same pair; changes inside the same
scope keep the external ID and change the digest. A future durable outbox must
atomically retain and compare both, reject conflicting payloads, distinguish an
ERP acknowledgment from a posted receipt, and reconcile ambiguous timeouts.
This helper itself stores no replay state and makes no exactly-once guarantee.

## Verified research and implementation choice

Primary sources checked on 16 October 2026:

- [NetSuite journalentry REST record](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_159886587653.html)
  requires real account/line mappings; its example has subsidiary and balanced
  debit/credit lines. Our generic preview is not asserted to be a NetSuite API payload.
- [NetSuite external-ID upsert](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156335203191.html)
  can update an existing record. Consequently, blindly retrying an upsert is not
  our required same-scope/different-payload conflict rejection. Its separate
  [asynchronous idempotency-key mechanism](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_164494900632.html)
  identifies duplicate requests and returns an existing job reference. A future
  connector must qualify the chosen operation and reconcile its result with the
  posted record; neither mechanism alone is a posted journal receipt.
- [SAP general-ledger APIs](https://help.sap.com/docs/SAP_S4HANA_ON-PREMISE/651d8af3ea974ad1a4d74449122c620e/54eaa306f7014d179cc540aab92faa92.html)
  distinguish OData reads from the journal-posting SOAP interface. A generic
  OData callback is not evidence of SAP posting qualification.
- [CAIP-19](https://standards.chainagnostic.org/CAIPs/caip-19) separates chain and
  asset identifiers and notes namespace-specific normalization requirements.
- [x402](https://github.com/x402-foundation/x402) is a payment protocol with
  separately chosen network/assets/schemes. It is not an accounting currency or
  a native Conxian token; a selected network's gas and application service fees
  remain separate concepts.
- [Nitro attestation verification](https://docs.aws.amazon.com/enclaves/latest/user/verify-root.html)
  binds an attested environment and contextual data. Our submitted digest is not
  an attestation verifier or proof of the external event's truth.

The selected change adds useful accounting conformance within the existing SDK
without introducing another fee engine, runtime signer, ledger authority or
unqualified ERP endpoint. Existing Gateway/Rust ownership remains intact.

## Next acceptance boundaries

Gateway owns the connector/outbox; Nexus owns observation evidence; Core/SDK
retain protocol/provider primitives; Market owns its fee/billing behavior; the
customer's accounting system retains its general-ledger policy and authorization.
Extend existing Business #1343/#1390 work instead of creating a parallel treasury.

Before real posting, verify the selected tenant's account, subsidiary/book,
currency/precision and date mapping; trusted evidence and valuation; durable
idempotency and reconciliation; reorg/reversal handling; authentication, audit,
failure/timeout behavior; and actual ERP receipt read-back. Fees and custody
changes require their existing owner approvals. No production, hardware or
vendor certification is established by this offline implementation.
