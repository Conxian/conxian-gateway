# Research: Sovereign Yield Index (SYI)

## Current operational status — 2026-11-11

SYI is a research concept, not a live financial metric. The legacy Gateway
TreasuryMonitor used an unverified ALEX quote, substituted a fixed fallback
proxy on quote failure, seeded hard-coded treasury balances, generated
synthetic liquidity growth, applied fixed tranche/yield/tax assumptions, and
derived FX values from the same proxy. These values were written into shared
Gateway state and exposed by API/metrics surfaces. Gateway #880 records the
production evidence and the fail-closed correction.

The monitor no longer polls or mutates state. The treasury SYI API contract
returns an unavailable status, null rate/timestamp, and an explicit reason.
Treasury fields in state and JSON metrics are null and marked unavailable;
unqualified financial Prometheus series are omitted. Historical source and UI
values must not be interpreted as observed assets, yield, customer balances,
revenue, or adoption.

## Proposed methodology (not implemented)

The conceptual ratio below is a research hypothesis only:

\[
SYI = \frac{\text{sBTC realized yield}}{\text{BTC opportunity cost}}
     \times \text{sovereignty adjustment}
\]

No production value can be calculated until both numerator and denominator
are sourced from authoritative, independently verifiable observations with
compatible asset definitions, units, windows, and accounting treatment.
A non-custodial design preference is not itself a quantitative yield premium;
any adjustment factor requires a documented, defensible methodology and review.

## Minimum reactivation evidence

1. **Provider and network** — identify the authoritative provider, chain/network,
   contract/pool, and endpoint; prove the provider is not a fixture or
   compatibility-only endpoint.
2. **Asset identity and units** — bind the exact sBTC/BTC/STX assets, decimals,
   quote direction, input amount, and conversion semantics.
3. **Observation quality** — capture source timestamp, observed timestamp,
   freshness/expiry, block height or equivalent, and reject stale, malformed,
   substituted, or incomplete observations.
4. **Accounting semantics** — distinguish balances, liquidity, market depth,
   realized yield, projected yield, fees, taxes, and hypothetical allocations.
   Do not infer ownership or treasury control from a market quote.
5. **Failure behavior** — unavailable provider data must produce unavailable
   status, never a fallback proxy, seeded balance, or growth estimate.
6. **API and telemetry** — preserve explicit availability and provenance;
   never expose null/unknown as zero or label fixtures as live.
7. **Independent acceptance** — verify the exact source, artifact, runtime
   configuration, provider/network journey, rollback, and operational soak.

## Boundaries

- This research does not authorize peg-in/peg-out, signing, custody, swaps,
  treasury allocation, settlement, payment entitlement, or on-chain value.
- The sBTC bridge monitor remains a separate read-only lifecycle observation
  path; it is not an SYI data source until separately qualified.
- Proposed 80/10/10 or senior/junior distributions are not live calculations,
  approved commercial terms, or customer commitments.
- Gateway #880 owns the runtime correction and evidence gate. Historical
  implementation notes remain history, not current readiness claims.
