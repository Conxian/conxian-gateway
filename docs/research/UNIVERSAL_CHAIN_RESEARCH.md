# Universal Chain Support Research (CON-810 / CON-789)

## 1. Multi-Chain Adapter Patterns
Research conducted via Context7 into high-performance async Rust middleware for blockchain gateways (Axum/Tokio).

### Findings:
- **Trait-based Abstraction**: Use a core `ChainAdapter` trait to define common operations (state lookup, event subscription, transaction preparation).
- **Dynamic Dispatch**: Utilize `Arc<dyn ChainAdapter>` within Axum `State` to allow runtime selection of chain families.
- **Provider Injection**: Inject chain-specific RPC clients (Bitcoin, Stacks, EVM) into adapters at initialization.

## 2. Cross-Chain Event Bus Delivery
High-integrity event distribution from Nexus to Gateway.

### Findings:
- **Durable Checkpoints**: Implement persistent offset storage to ensure "at-least-once" delivery.
- **Backpressure Handling**: Use Tokio channels with bounded capacity and `poll_ready` patterns to prevent memory exhaustion during high-load event spikes.
- **Retry Policy**: Map backend errors to a taxonomy (Transient, Permanent, Indeterminate) to drive intelligent retry logic.

## 3. Tier 1 Chain Families
Decided families for initial execution:
1. **Bitcoin (Native)**: Core settlement layer.
2. **Stacks (Nakamoto)**: Smart contract and L2 coordination.
3. **EVM (Compatible)**: Broad application surface support.

## 4. Implementation Strategy
- **Layer 1**: Protocol coordination (Nexus) manages canonical state.
- **Layer 2**: Compliance pipe (Gateway) listens for state transitions and enforces trust policies.
- **Shared**: Core primitives in `lib-conxian-core` ensure consistency across adapters.
