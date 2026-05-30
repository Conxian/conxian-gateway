# RFC: RGB Protocol-Rail Adapter

## Status
- **Draft** (Research Phase)

## Context
As part of the Conxian Gateway evolution, we need to support the RGB protocol as a first-class citizen for smart contract and asset logic on Bitcoin. This adapter provides the bridge between the Conxian engine and the RGB node/state.

## Scope
- Protocol-rail adapter contract definition.
- Support for 'disabled', 'shadow', and 'active' rollout modes.
- Integration with node-backed contract lookup.

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
}

#[async_trait]
pub trait RgbAdapter {
    async fn lookup_contract(&self, contract_id: &str) -> ConxianResult<Option<ContractState>>;
    async fn verify_transition(&self, transition_id: &str) -> ConxianResult<bool>;
}
```

## Rollout Modes

### 1. Disabled
- The adapter returns an error or empty result for all calls.
- No interaction with the RGB node.

### 2. Shadow (Current Goal)
- The adapter performs lookups and verifications but results do not affect the main execution path.
- Used for telemetry and validation against live node data.
- Errors are logged but do not panic or block processing.

### 3. Active
- Fully integrated into the execution path.
- Results from the adapter drive decision-making (e.g., settlement triggers).

## Non-Goals for Milestone 1 & 2
- Full asset issuance logic.
- Complex client-side validation logic (handled by RGB node).
- Multi-sig RGB contract creation.

## Implementation Plan
1. Define the trait and config.
2. Implement a mock-ready skeleton.
3. Wire into `MempoolOrchestrator` for shadow-mode observation.

## PoC Documentation (Milestone 2)

### Setup Assumptions
- The gateway has network access to an RGB node (default: `http://localhost:8080`).
- `RGB_MODE` environment variable is set to `shadow` or `active`.
- Contracts are identified by the `rgb:` prefix for simulation.

### Limitations
- Currently simulates node responses.
- Shadow mode logs findings but does not block if lookups fail or return empty.
- Verification logic is a pass-through in this PoC.

### Observed Behavior
- When a transaction is tracked in the mempool, the orchestrator triggers a lookup via the RGB adapter.
- In `shadow` mode, the logs will show "contract found but result will be ignored" if a match occurs.

### Error Semantics
- `ConxianError::Bitcoin`: Used for transport errors when communicating with the RGB node.
- `ConxianError::Security`: Used when transition verification fails in 'active' mode.
- In 'shadow' mode, all errors are caught, logged, and the execution path continues as if no match was found.
