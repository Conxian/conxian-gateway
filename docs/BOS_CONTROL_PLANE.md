# BOS Control-Plane (v1.9.2)

The BOS Control-Plane is the orchestration and governance layer for the Conxian Gateway. It provides a sovereign interface for managing the institutional lifecycle of Bitcoin-native settlements.

## Architecture

The control plane is built as a Next.js application located in `apps/control-plane`. It communicates with the Conxian Gateway API (`/api/v1` and `/admin/v1`).

## Key Functional Areas

### 1. Dashboard
- **Gateway Connectivity**: Live status of the local gateway instance.
- **Persistence State**: Verification of atomic persistence (gateway_state.json).
- **Compliance Status**: ZSE (Zero Secret Egress) health check.

### 2. Release Governance
- **Promotion Workflow**: Gated transition of code from `dev` -> `staged` -> `main`.
- **Readiness Verification**: Automatic checking of security, treasury, and regulatory gates.
- **Decision Records**: Auditable logs of who approved or rejected a release.

### 3. Audit Log
- **Event Sourcing**: Stream of high-integrity events from the gateway.
- **Filtering**: Searchable logs by actor, event type, and target resource.
- **Immutable Provenance**: Designed to be backed by sovereign commitments.

### 4. Policy Approvals
- **Proposal Management**: Creation and review of institutional mandates.
- **Quorum Tracking**: Real-time monitoring of approval thresholds.
- **Timelock Enforcement**: Visual indicators of 144-block Stacks timelocks.

### 5. System Metrics
- **Settlement Volume**: Aggregated BTC volume across all supported layers.
- **TAM Capture**: Real-time tracking of addressable market penetration.
- **Latency Monitoring**: Request/Response timing for institutional SLAs.

## Security Controls
- **Role-Based Access**: Integrated with the Gateway's `AuthStore`.
- **ZSE Compliance**: The UI does not leak sensitive internal state or secrets.
- **Constant-Time Verification**: Backend calls leverage `subtle` for security.
