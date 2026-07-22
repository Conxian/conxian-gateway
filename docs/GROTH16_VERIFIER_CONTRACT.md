# Groth16 verifier boundary contract

This document defines the internal boundary introduced for issue #219. It is
an adapter/backend contract, not a production proving or pairing-verification
implementation.

## Scope and non-goals

- The boundary currently admits **BN254** with 32-byte big-endian scalar-field
  elements.
- `Groth16Verifier` receives public inputs, a witness commitment, a canonical
  statement hash, block context, and proof bytes. It never receives raw witness
  values.
- The contract is backend-neutral. A future `ark-groth16`, BitVM, or other
  backend may implement the trait without changing the statement format.
- `MockGroth16Verifier` is deterministic fixture-only behavior. It is exported
  only when the explicit `mock-integrations` feature is enabled (and for unit
  tests), registers exact key bytes plus their circuit association, and returns
  a deterministic result. It does **not** perform pairings and must not be used
  as cryptographic proof verification.
- No prover or production proving dependency is included.

## Canonical types and validation

`Groth16Statement` contains:

1. schema version (`u16`, currently `1`);
2. curve (`bn254`) and field encoding (fixed-width 32-byte big-endian);
3. non-empty ASCII graphic `circuit_id` (maximum 128 UTF-8 bytes);
4. `VerificationKeyId` (32 bytes);
5. an ordered, non-empty public-input vector (maximum 256 field elements),
   whose final two slots are reserved for the witness commitment;
6. a non-zero 32-byte witness commitment; and
7. Bitcoin block context: network, exact anchor height, 32-byte block hash,
   and an optional maximum valid height.

Field elements must be strictly less than the BN254 scalar modulus. Values are
not reduced modulo the modulus, so alternate encodings are rejected. Public
inputs are never sorted; their order is consensus-critical.

### Witness commitment public-input layout (schema v1)

The runtime envelope carries the witness commitment as a 32-byte value, but a
future Groth16 backend must verify the commitment as circuit-bound public
inputs. Schema v1 reserves the final two public-input slots in this exact
order:

```text
public_inputs = circuit_inputs || commitment_high_128 || commitment_low_128
```

The commitment bytes are interpreted big-endian. Bytes `0..16` become the
high 128-bit limb and bytes `16..32` become the low 128-bit limb. Each limb is
zero-extended on the left to a 32-byte big-endian BN254 field element; no
modulus reduction or alternate byte order is permitted. The adapter validates
these two slots against `witness_commitment` and rejects a mismatch before
calling the backend.

This boundary binds the supplied commitment to the proof's public-input
representation, but it cannot prove that the circuit computed the commitment
correctly. The registered circuit must constrain these public commitment limbs
to the private witness-derived commitment relation. A production pairing
backend remains responsible for checking the proof against that circuit and
its verification key.

The block height in a statement is the exact Bitcoin anchor height, not a
confirmation count. Verification requires `current_block_height >=
block_height`. If `max_valid_height` is present, it must be at least the anchor
height and the current height must not exceed it. Height zero, an all-zero
block hash, future anchors, and expired statements are rejected.

The compressed proof envelope is exactly 128 bytes:

| Segment | Width |
| --- | ---: |
| compressed G1 `A` | 32 bytes |
| compressed G2 `B` | 64 bytes |
| compressed G1 `C` | 32 bytes |

The boundary rejects empty, all-zero, or incorrectly sized proof bytes. It
does not claim to validate curve points, subgroup membership, or pairings;
those checks belong to a real backend.

## Domain-separated encoding and hashes

All integer lengths and integers below use unsigned big-endian encoding.
Fixed-width byte arrays are not interpreted as variable-length JSON values.
JSON object/map order is irrelevant because the parser maps named envelope
fields into this ordered structure before hashing.

### Statement encoding

`Groth16Statement::canonical_encode()` emits, in this exact order:

1. literal domain `CONXIAN-GROTH16-STATEMENT-ENCODING-V1`;
2. schema version (`u16`);
3. curve tag (`1` for BN254);
4. field encoding tag (`1` for 32-byte big-endian BN254 elements);
5. `u32` circuit-ID byte length followed by circuit-ID bytes;
6. 32-byte verification-key ID;
7. `u32` public-input count followed by each 32-byte field element in order;
8. 32-byte witness commitment;
9. Bitcoin network tag (`1` mainnet, `2` testnet, `3` signet, `4` regtest);
10. anchor block height (`u64`);
11. 32-byte block hash in canonical Bitcoin display order;
12. one-byte expiry flag (`0` absent, `1` present), followed by `u64`
    `max_valid_height` when present.

The statement hash is:

```text
SHA256(
  "CONXIAN-GROTH16-STATEMENT-HASH-V1"
  || u32_be(len(canonical_statement))
  || canonical_statement
)
```

The request carries this hash separately as `statement_hash`. The verifier
must recompute it and reject a mismatch before backend work.

### Verification-key ID

The key ID is bound to exact key bytes:

```text
SHA256(
  "CONXIAN-GROTH16-VERIFICATION-KEY-ID-V1"
  || u32_be(len(vk_bytes))
  || vk_bytes
)
```

Registration rejects an ID that does not equal this digest. Each registered
record also contains the exact `circuit_id`, schema version, and curve. The
adapter/backend must reject an unknown or mismatched
`(circuit_id, verification_key_id)` association before proof verification, and
verification rechecks the stored bytes so key/hash integrity cannot be
silently bypassed. Key bytes are limited to 1 MiB and must be non-empty.

### Witness commitment

Witness values are prover-side field elements only. For deterministic fixture
reproduction, `compute_witness_commitment` hashes:

```text
payload = u32_be(witness_count) || field_0 || field_1 || ...
commitment = SHA256(
  "CONXIAN-GROTH16-WITNESS-COMMITMENT-V1"
  || u32_be(len(payload))
  || payload
)
```

The runtime request contains only the resulting 32-byte commitment. The
checked-in fixture includes synthetic `witness_values` solely so tests can
reproduce the expected commitment; those values are not accepted in the
runtime BitVM envelope.

## BitVM handoff

`parse_bitvm_groth16_envelope` accepts the explicit JSON envelope fields:

```text
schema_version
curve
circuit_id
verification_key_id       (64 hex characters)
public_inputs              (ordered array of 64-character hex values)
witness_commitment         (64 hex characters)
block_context.network
block_context.block_height
block_context.block_hash
block_context.max_valid_height
proof                      (256 hex characters)
statement_hash             (64 hex characters)
```

The envelope is decoded into the canonical Rust types, rejects unknown fields
and any `witness`/`raw_witness` field, and calls `Groth16VerificationRequest::validate()`
before delegation. That validation includes the reserved witness-commitment
public-input slots. `BitVmAdapter::verify_groth16_envelope_with` then checks the
registered circuit/key association before delegating to a borrowed
`Groth16Verifier`; `with_verifier` provides the injected `Arc` form. The
envelope network must also match the adapter's configured Bitcoin network. A
backend result with `valid: false` is converted to the typed
`InvalidProof(BackendRejected)` error instead of being returned as a successful
adapter result. Structured tracing records only the anchor height and
statement hash; it does not emit circuit identifiers, proof bytes, witness
material, or key bytes.

## Verification semantics and observability

The boundary has three deliberately separate layers:

1. **Canonical envelope validation** checks schema, encodings, public-input
   order, witness-commitment limb binding, circuit/key association, freshness,
   and statement-hash integrity. It is not cryptographic proof verification.
2. **Backend proof verification** is supplied by a future pairing-capable
   implementation. A successful adapter result is reserved for a backend that
   reports a valid proof; invalid backend results fail closed as
   `InvalidProof`.
3. **Legacy generic chain-state routes** are not proof verification. The
   BitVM `ChainAdapter::verify_state_proof` implementation fails closed with
   a typed unavailable error, and `POST /api/v1/chains/bitvm/verify` maps that
   capability gap to HTTP `501 Not Implemented` rather than returning a
   successful `verified` field.

The repository's existing metrics are shared flat gateway-state counters and
the engine adapter has no stable metrics sink. This boundary therefore does
not add a bespoke global registry or misleading “verified” counters before a
production backend exists. When that backend is integrated, add bounded
low-cardinality counters/histograms for `stage={parse,validation,backend}`
and `outcome={accepted,rejected,unavailable}` plus latency; never use circuit
identifiers, statement hashes, proof digests, or verification-key IDs as
labels.

## Generic BitVM API migration note

The generic `POST /api/v1/chains/bitvm/verify` route is **not cryptographic
evidence**. It now returns HTTP `501 Not Implemented` with this stable shape:

```json
{
  "chain": "bitvm",
  "status": "unsupported",
  "code": "verifier_unavailable",
  "authoritative": false,
  "message": "BitVM verification is unavailable on the generic chain route"
}
```

Clients must not treat the historical HTTP 200 response with
`"verified": true` as proof or use it to authorize settlement. That behavior
was metadata-based and could not establish cryptographic validity. A future
canonical BitVM/Groth16 verification flow requires the explicit envelope
contract above plus a reviewed production backend; the checked-in mock remains
fixture-only and does not change this API posture.

## Fixture contract

`internal/engine/tests/fixtures/groth16/bitvm_fixture.json` is the reference
synthetic vector. It has three circuit-specific inputs followed by the two
ordered witness-commitment limbs, three private synthetic witness values, a
deterministic witness commitment, a non-empty 128-byte proof, key/circuit
identity, regtest block context, the expected statement hash, and
`expected_valid: true`.

Integration tests reproduce the commitment and statement hash, exercise both
injected and borrowed BitVM handoffs, reject commitment-limb mismatches before
backend invocation, reject input reorder/mutation, commitment mutation, proof
mutation, wrong key/circuit association, malformed field/proof encodings,
stale/future/expired block context, statement-hash tampering, raw witness
material, and an explicit backend `valid: false` result.
