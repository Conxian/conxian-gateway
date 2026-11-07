# Licensing boundary

Conxian Gateway uses a **two-tier licence model** so the transport/adapter layer
stays open and reusable while the monetization surface stays defensible.

| Crate | Path | Licence | Contents |
|---|---|---|---|
| `conxian_core` | `pkg/conxian-core` | **MIT** | Shared primitives, schemas, generic adapters. No internal dependencies — freely reusable. |
| `conxian_compliance` | `internal/compliance` | **AGPL-3.0-or-later** | ISO 20022 translation, ZKC, identity, verifier. |
| `conxian_engine` | `internal/engine` | **AGPL-3.0-or-later** | Billing / MRR, protocol-fee accounting, chain adapters. |
| `conxian_api` | `internal/api` | **AGPL-3.0-or-later** | REST API, x402 metering, MoR entitlement, tenant auth, rate limiting. |
| `gateway` | `cmd/gateway` | **AGPL-3.0-or-later** | Binary / runtime wiring; links the AGPL moat crates, so it is AGPL. |
| `conxian-cli` | `cmd/conxian-cli` | **MIT** | Installer and management CLI; does not link the moat. |

## Why

`ADR-009` designates `internal/engine/src/billing.rs` as the single authoritative
rate card, and `internal/api` carries the x402 / MoR / tenant metering. Under a
permissive licence a third party could fork the gateway, strip the rate card, and
resell the bridge as a service without contributing back.

The **AGPL-3.0 network clause (§13)** closes that gap: anyone who runs a modified
version of these crates as a network service must offer the corresponding source
to its users — or obtain a commercial licence from Conxian-Labs.

## The rule for contributors

> If a competitor could fork the file and run it as a service to take revenue,
> it belongs in an **AGPL** crate. Everything else belongs in **`conxian_core`** (MIT).

## Donation

Only `conxian_core` (MIT) is a candidate for donation to a neutral foundation
(FINOS/LFDT). The AGPL moat crates are **not** donatable and remain company-owned.
See `Conxian/.github-private` governance decision records and
`conxian-business#1790`.
