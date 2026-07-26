# RGB BIP340 Issuer Policy Evidence — 2026-07-26

## Decision

Conxian defines one explicit, opt-in RGB issuer signature profile:

- match the RGB `Identity` as an exact, case-sensitive printable-ASCII string
  of `1..=4096` bytes;
- resolve it only through a versioned public-key allowlist;
- require algorithm name `bip340-secp256k1`;
- interpret `SigBlob` as exactly the raw 64-byte BIP340 signature;
- verify against the exact 32 callback bytes with the pinned x-only secp256k1
  public key, without rehashing or text encoding.

All missing, unknown, malformed, duplicate, or unsupported policy inputs reject.
`RejectIssuerSignatures` remains the default. This slice adds no public import
endpoint and no automatic runtime policy loading.

## Upstream evidence

The dependency graph pins `rgb-std` `0.12.0-rc.3`. Its verified tag commit is
[`e183bebfed9ffd0ba8c6f7110fe3e097f23cd70b`](https://github.com/RGB-WG/rgb-std/commit/e183bebfed9ffd0ba8c6f7110fe3e097f23cd70b).
At that revision, `Consignment::articles` accepts an application callback with
the shape `FnOnce(StrictHash, &Identity, &SigBlob)` rather than selecting a
cryptographic algorithm:

- [pinned `rgb-std` consignment source](https://github.com/RGB-WG/rgb-std/blob/e183bebfed9ffd0ba8c6f7110fe3e097f23cd70b/src/consignment.rs)
- [`rgb-std` 0.12.0-rc.3 API documentation](https://docs.rs/rgb-std/0.12.0-rc.3/rgb/struct.Consignment.html)

The pinned semantic implementation computes `ArticlesId::commit_id()` before
calling the supplied validator. `SigBlob` is documented as an algorithm-
abstracting opaque blob, with a bounded non-empty byte representation rather
than a concrete signature type:

- [`sonic-api` 0.12.0 `SigBlob` documentation](https://docs.rs/sonic-api/0.12.0/sonicapi/struct.SigBlob.html)

`Identity` comes from pinned `ultrasonic` `0.12.0` and is an application-defined
printable-ASCII string type (`1..=4096` bytes), not a public-key or algorithm
identifier. The inspected source revision was verified as
[`e755603f4662b9e93f3a329414e02a149b9f2f65`](https://github.com/AluVM/ultrasonic/commit/e755603f4662b9e93f3a329414e02a149b9f2f65):

- [pinned `Identity` definition](https://github.com/AluVM/ultrasonic/blob/e755603f4662b9e93f3a329414e02a149b9f2f65/src/util.rs)

A source review of the pinned callback path and package APIs found no compatible
concrete issuer validator that Conxian could safely adopt without defining an
application profile. This is a statement about the inspected pinned dependency
path, not a claim that no RGB ecosystem implementation exists elsewhere.

## Alternatives rejected

- **Infer an algorithm from `Identity`:** rejected because upstream treats the
  string as opaque application identity. Prefix conventions would be an
  unreviewed protocol and could change verification behavior silently.
- **Infer an algorithm from `SigBlob` length/content:** rejected because the
  blob is deliberately algorithm-abstract. Payload sniffing creates downgrade
  and ambiguity risk.
- **Hash the callback value again:** rejected because the callback already
  supplies the exact commitment bytes selected by upstream. A second hash would
  define a different signing protocol.
- **Accept any parseable secp256k1 key or self-describing signature:** rejected
  because issuer authorization requires an independently provisioned trust
  decision, not merely valid cryptography.
- **Fallback to accept-all for compatibility:** rejected. Unknown identities,
  algorithms, and policies remain fail closed.

## Rotation, revocation, and operations

The JSON policy is a startup-loadable public-key allowlist, not a private-key
store. Rotation is an explicit configuration replacement under operator change
control. Revocation removes the exact identity entry or replaces its pinned key
before the next controlled load. The backend does not fetch keys, follow DID
documents, merge policies, or hot-reload. Deployments that later add runtime
wiring must define atomic configuration rollout, audit provenance, rollback,
and restart behavior without weakening the default reject policy.

The bounded policy-file loader is Unix-only. It fails closed on non-Unix
platforms until a reviewed handle-level no-follow implementation can provide
equivalent protection against path replacement and special-file inputs.

## Remaining evidence gap

Unit tests prove exact identity/key/message binding, the pinned `Identity`
length boundary, malformed-input rejection, strict policy parsing, bounded Unix
regular-file loading, and no second hash. They do not replace a complete
independently reproducible Bitcoin/RGB regtest fixture that creates and imports
a real state-changing signed transition. That fixture, plus an approved
controlled import call site, remains required before claiming production
Active-mode issuer import.
