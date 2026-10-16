# Deterministic journal preview before privileged ERP delivery

Refs: Conxian/conxian-business#1390, #1343 and #1411.

The current camt/OData callback is not a vendor-qualified general-ledger posting
contract. Add a portable, offline journal preview in the Gateway client SDK.
Validate a bounded, versioned request with organization/ledger/event scope,
source repository revision, explicit network/asset and atomic asset quantity,
separate accounting-currency scale/valuation reference, and balanced debit/credit
lines. Use decimal strings and exact integer arithmetic. Return a deterministic
scope key and payload digest for a future durable outbox's duplicate/conflict
checks. Preserve source status as an unverified claim.

The preview must never fetch, sign, post, retry, collect fees, grant a license or
turn submitted hashes into proof. Its output states that source/valuation evidence
is unverified and posting is unauthorized. No customer information is logged.
Existing provider, custody, replay, independent review and release gates remain.

Acceptance: valid multi-line journals and large values; rejected imbalance,
non-canonical units, wrong network/asset binding, unknown authority fields, invalid
dates/revisions, excessive size/lines and malformed JSON values. Reordered object
keys preserve digests; changed payloads retain the scope key but change the payload
digest; organization/ledger/event changes alter the scope key. Output is immutable.
Run SDK/workspace build, type and tests plus applicable repository hygiene checks.
Native runtime code and existing ERP delivery routes remain unchanged.

Next owner work: actual tenant-specific account/currency mappings, trusted proof
and valuation verification, durable transactional outbox/replay, reorg/reversal
policy, authenticated ERP connectors, recipient read-back and independent pilot
acceptance. A transport 2xx or this preview is not a posted journal receipt.
