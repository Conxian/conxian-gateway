# RFC: RGB Protocol-Rail Adapter

## Status
- **Phase 2 filesystem/consignment boundary implemented on the focused #228 branch**
- Issue #228 remains open until a concrete issuer signature backend and
  deterministic end-to-end Bitcoin/RGB fixture are accepted.

## Context
As part of the Conxian Gateway evolution, we need to support the RGB protocol as a first-class citizen for smart contract and asset logic on Bitcoin. This adapter provides the bridge between the Conxian engine and the RGB node/state.

## Scope
- Protocol-rail adapter contract definition.
- Support for 'disabled', 'shadow', and 'active' rollout modes.
- Integration with node-backed contract lookup via HTTP.

## Adapter Interface

```rust
pub enum RolloutMode {
    Disabled,
    Shadow,
    Active,
}

pub struct RgbAdapterConfig {
    pub mode: RolloutMode,
    pub node_url: String,
    pub stash_path: Option<String>,
    pub esplora_url: Option<String>,
}

#[async_trait]
pub trait RgbAdapter {
    async fn lookup_contract(&self, contract_id: &str) -> ConxianResult<Option<ContractState>>;
    async fn verify_transition(&self, transition_id: &str) -> ConxianResult<bool>;
}
```

## Rollout Modes

### 1. Disabled
- The adapter is a no-op and returns empty/false results.
- No interaction with the RGB node.

### 2. Shadow (Current Goal)
- The adapter performs lookups and verifications but results do not affect the main execution path.
- Node failures and unknown responses may use an explicitly simulated result.
- Simulation is never used by Active mode.

### 3. Active
- The native RGB stockpile is authoritative for contract presence and
  verification boundaries. The JSON metadata cache is descriptive only.
- Active does not use HTTP or simulation as consensus proof. Missing native
  configuration and stockpile errors fail closed.
- With `rgb-native`, `RGB_STASH_PATH` and `RGB_ESPLORA_URL` are required.

## Implementation Details
- Concrete implementation in `internal/engine/src/bitcoin/rgb_adapter.rs`.
- Uses `minreq` on a blocking task for lightweight HTTP communication with the RGB node.
- All non-disabled paths pass through one shared contract-ID normalizer. Native
  builds use `rgb::ContractId` for full Baid64 validation and canonicalization;
  default-feature builds still reject empty, legacy, prefixless, and malformed
  IDs at the boundary and accept only the canonical chunked `contract:` shape.
  An optional Baid64 mnemonic fragment is accepted but removed before stash,
  HTTP, and response propagation. Legacy `rgb:` values are not consensus-valid
  IDs.
- `StashResolver` owns an exact-pinned
  `rgb_persist_fs::StockpileDir<bp::seals::TxoSeal>` and reloads it after
  successful imports. Corrupt contract directories fail closed at startup.
- Unknown-contract imports run in a same-filesystem staging directory and are
  atomically promoted only after `rgb::Contracts::consume_from_file` succeeds.
  This compensates for the pinned `rgb-persist-fs` behavior that creates the
  `*.contract` directory before `evaluate_commit` completes; every failed
  import removes only its own staging directory and reports cleanup failures
  separately. Existing contract updates are rejected until a copy-on-write
  update path is available, so an import cannot mutate a valid contract in
  place.
- Descriptive metadata remains in an atomic JSON cache, but it is never used
  by `verify_transition` or the Active proof path.
- The wallet-owned auth-token registry stores only strict-encoded seal
  definitions and RGB auth tokens. It validates the committed token, is
  idempotent for identical replays, and rejects overwrite attempts and corrupt
  persistence atomically. The stash is a local-filesystem trust boundary, not
  an encryption boundary: on Unix the resolver restricts its owned directory
  to the owner and writes registry files with owner read/write permissions;
  file data is synced before rename and the parent directory is synced after
  atomic replacement.
- `import_consignment` preflights the pinned RGB consignment envelope, rejects
  unsigned consignments, invokes the caller-owned issuer signature validator,
  and delegates full operation/codex/witness consensus checks to
  `rgb::Contracts::consume_from_file`.
- In the pinned `allow_unknown = true` first-contract branch, RGB imports the
  articles and genesis stockpile without invoking the supplied seal resolver.
  Therefore contract genesis/import does not by itself prove wallet-owned seal
  ownership or query Esplora. Paths for already-known contracts retain the
  resolver callback and fail closed when a registered seal is absent or its
  Esplora check is not unspent.
- `export_consignment` serializes only registered RGB terminal auth tokens;
  no identity or other PII is stored in the registry.
- Esplora UTXO queries preserve spent, unspent, not-found, and transport-error
  distinctions. They are not substituted for RGB consensus verification.
- Mempool orchestration does not synthesize an RGB contract ID from a Bitcoin
  transaction ID.

## Setup and Configuration
- `RGB_MODE` defaults to `disabled`.
- `RGB_NODE_URL` defaults to `http://localhost:8080`; plain HTTP is allowed
  only for localhost/loopback development. Embedded credentials are rejected.
- `RGB_STASH_PATH` (a directory, not the old metadata file path) and
  `RGB_ESPLORA_URL` are optional in Disabled/Shadow, but
  must be configured together. Active mode with `rgb-native` requires both.
- Simulation uses only the `contract:` HRI and is explicitly non-consensus.

## Boundary Behavior
- A tracked Bitcoin transaction ID is not an RGB contract ID. The mempool
  orchestrator intentionally skips RGB lookup until a real contract-ID source
  exists.
- `verify_transition` now checks only for a contract successfully persisted by
  the RGB stockpile. The adapter interface does not carry a consignment path,
  so callers requiring full operation verification must use
  `StashResolver::import_consignment`.
- A successful HTTP contract lookup must contain a matching `contract_id`, a
  non-empty `schema_id`, and an object-valued `state`. Empty or mismatched 200
  bodies are errors in Active mode and use Shadow-only simulation fallback.
- Shadow-only HTTP verification responses still require a boolean `valid`
  field and matching optional IDs; they are never Active proof.

## Error Semantics
- `ConxianError::Rgb`: Used for invalid IDs, configuration, persistence, native
  resolver, and RGB node errors.
- In Shadow mode, failures may fall back to simulation without affecting the
  main execution path.
- In Active mode, failures are returned and unknown contracts resolve to false
  or no result; they are never treated as positive proof.

## Remaining Phase 2 limitations
- The pinned `rgb-std` API deliberately leaves the signature algorithm to the
  application. The gateway exposes `IssuerSignatureValidator` and ships a
  `RejectIssuerSignatures` fail-closed policy; it does not claim Ed25519,
  secp256k1, or another cryptographic backend.
- Deterministic unit coverage exercises malformed envelopes, contract-ID
  mismatch, staged cleanup after a semantically invalid unknown-contract
  import, fresh-stash reload after cleanup, the pinned unknown-contract
  resolver boundary, Unix permission hardening, registry replay/overwrite,
  unknown auth tokens, invalid signature policy, and corrupted persistence. A
  complete signed RGB/Bitcoin regtest fixture is still required before treating
  Active consignment import as a production rollout milestone.
- The JSON cache remains for descriptive lookup compatibility and must not be
  interpreted as consensus evidence.
