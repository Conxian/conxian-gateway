# Research: Sovereign Yield Index (SYI)

## 1. Concept
The SYI is a protocol-level metric that tracks the relative yield performance of sBTC vs native BTC and other Stacks-based yield instruments. It is the core incentive mechanism for the "sBTC Suction" pattern.

## 2. Calculation
$SYI = \frac{\text{sBTC Realized Yield}}{\text{BTC Opportunity Cost}} \times \text{Sovereignty Multiplier}$

- **Sovereignty Multiplier**: Higher for non-custodial instruments.
- **Opportunity Cost**: Derived from ALEX and other DEX oracles.

## 3. Implementation in Gateway
The `TreasuryMonitor` in `internal/engine/src/treasury/mod.rs` will be enhanced to poll ALEX liquidity pools and calculate the SYI in real-time, exposing it via the `/api/v1/metrics` endpoint.

## 4. Next Steps
- Integrate ALEX oracle feeds for real-time yield tracking.
- Implement TEE-signed attestation for SYI reports to ensure institutional-grade reporting.
