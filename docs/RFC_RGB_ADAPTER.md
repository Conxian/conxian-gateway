# RFC: RGB Protocol-Rail Adapter

## Status
- **Phase 1.5 hardening implemented**
- Full issue #228 remains open: consignment/Stockpile integration and
  consensus verification are Phase 2 follow-ups.

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
- Native and HTTP results drive the execution path.
- Unknown contracts and native/HTTP failures fail closed; Active never turns a
  missing node result into a simulated success.
- With `rgb-native`, `RGB_STASH_PATH` and `RGB_ESPLORA_URL` are required.

## Implementation Details (Phase 1.5)
- Concrete implementation in `internal/engine/src/bitcoin/rgb_adapter.rs`.
- Uses `minreq` on a blocking task for lightweight HTTP communication with the RGB node.
- All non-disabled paths pass through one shared contract-ID normalizer. Native
  builds use `rgb::ContractId` for full Baid64 validation and canonicalization;
  default-feature builds still reject empty, legacy, prefixless, and malformed
  IDs at the boundary and accept only the canonical chunked `contract:` shape.
  An optional Baid64 mnemonic fragment is accepted but removed before stash,
  HTTP, and response propagation. Legacy `rgb:` values are not consensus-valid
  IDs.
- `StashResolver` persists metadata with atomic temp-file replacement and
  distinguishes spent, unspent, not-found, and transport-error UTXO results.
- Mempool orchestration does not synthesize an RGB contract ID from a Bitcoin
  transaction ID.

## Setup and Configuration
- `RGB_MODE` defaults to `disabled`.
- `RGB_NODE_URL` defaults to `http://localhost:8080`; plain HTTP is allowed
  only for localhost/loopback development. Embedded credentials are rejected.
- `RGB_STASH_PATH` and `RGB_ESPLORA_URL` are optional in Disabled/Shadow, but
  must be configured together. Active mode with `rgb-native` requires both.
- Simulation uses only the `contract:` HRI and is explicitly non-consensus.

## Boundary Behavior
- A tracked Bitcoin transaction ID is not an RGB contract ID. The mempool
  orchestrator intentionally skips RGB lookup until a real contract-ID source
  exists.
- The Phase 1.5 stash check establishes canonical ID parsing and local metadata
  presence only. It does not claim full `ContractVerify` completion.
- A successful HTTP contract lookup must contain a matching `contract_id`, a
  non-empty `schema_id`, and an object-valued `state`. Empty or mismatched 200
  bodies are errors in Active mode and use Shadow-only simulation fallback.
- A successful HTTP verification response must contain a boolean `valid` field.
  If the response includes `contract_id` or `transition_id`, it must match the
  normalized request ID; malformed verdicts are not accepted as verification.

## Error Semantics
- `ConxianError::Rgb`: Used for invalid IDs, configuration, persistence, native
  resolver, and RGB node errors.
- In Shadow mode, failures may fall back to simulation without affecting the
  main execution path.
- In Active mode, failures are returned and unknown contracts resolve to false
  or no result; they are never treated as positive proof.

## Phase 2 Blockers
- `rgb-persist-fs::StockpileDir` integration.
- Consignment import/export and receiver `AuthToken` → seal-definition registry.
- Signature policy and full consensus verification against Bitcoin state.
- Deterministic regtest fixtures/harness for end-to-end RGB transitions.
