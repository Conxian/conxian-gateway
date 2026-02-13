# Migration Guide: Anya-core & OPSource to Conxian Gateway

## Overview
Conxian Gateway succeeds `Anya-core` and `OPSource`, consolidating their core functionalities into a singular, audit-ready Rust binary.

## Deprecation Status
- **Anya-core**: Deprecated. Core Bitcoin/Stacks state logic ported to `internal/engine`.
- **OPSource**: Deprecated. API and Auth layer ported to `internal/api`.

## Action Items
1. Archive `Anya-core` and `OPSource` repositories.
2. Update all B2B integrations to use the new Gateway API (`/api/v1`).
3. Deploy Conxian Gateway as the primary state monitor and compliance pipe.
