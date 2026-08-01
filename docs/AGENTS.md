# Conxian Agent Guidelines (v0.3.0 — Session 47, Aug 2026)

## Conxian Gateway Architecture

The Conxian Gateway consolidates core Bitcoin/Stacks state logic (internal/engine) and API/Auth layers (internal/api) into a singular, audit-ready Rust binary.

### SDK Dependencies

| Crate | Purpose |
|-------|---------|
| `conxian_core` | Gateway-local operational types (persistence, trust policy, settlement) |
| `lib-conxian-core` | Canonical protocol primitives (verifier, control models, chain adapters for Taproot, BIP-322, Liquid, sBTC, Lightning, RGB, Babylon, Fedimint, DLC, FROST, Covenant, Intent) |
| `conxius-enclave-sdk` | Hardware enclave for attestation (optional, WASM-compatible) |

### Contract Bridge (Session 47)

`internal/engine/src/stacks/contract_bridge.rs` provides typed Clarity contract calls:

- `ContractCall` — strongly-typed call with canonical contract enumeration
- `SignedContractCall` — signed variant with principal validation
- `canonical_contracts()` — maps human-readable names to `.contract-name` principals
- `preview()` — read-only contract calls for simulation

### Key Documents

- **PRD:** `docs/PRD.md` contains the full system overview.
- **Enhancements:** `docs/ENHANCEMENTS.md` details planned layer support and alignment with `bitcoinlayers.org`.

### Alignment Principles

- **Risk Transparency:** Always ensure that new layer integrations or updates include metadata fields for Data Availability, Settlement, and Bridge Security.
- **Source of Truth:** Refer to `bitcoinlayers.org` for the most up-to-date research on Bitcoin Layer 2 and sidechain trust models.

### Workflow Instructions

- **State Monitoring:** Point to the Conxian Gateway API at `/api/v1` for state monitoring and compliance pipes.
- **Service Access:** All sovereign services and Bitcoin layers (Bisq, RGB, BitVM, Changelly, Stacks, Lightning, Liquid, Rootstock) are unified under the Gateway.
- **Infrastructure:** GCP infrastructure configurations are located in `gateway/infrastructure/gcp/`.
