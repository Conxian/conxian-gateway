# Conxian Control-Plane

Institutional management dashboard for the Conxian Gateway.

## Purpose
The Control-Plane provides a secure, role-based interface for managing release governance, policy approvals, identity resolution, and treasury monitoring. It acts as the human-in-the-loop coordination layer for the Conxian BOS (Sovereign Business Operations System).

## Status
**Active Development.** Currently used for simulated governance and release promotion rehearsals. Aligned with v0.1.4 gateway standards.

## Audience
- **Operators**: Managing daily gateway operations and configuration.
- **Auditors**: Reviewing settlement logs and governance decisions.
- **Admins**: Requesting and approving release promotions.

## Core Modules
- **Release Governance**: Managed promotion gates from `dev` to `main`.
- **Policy Approvals**: Jurisdictional sharding and enclave enforcement.
- **Identity Resolution**: BNS, ENS, and World ID (WIF) management.
- **Treasury Pulse**: Real-time liquidity monitoring for sBTC/BTC.

## Development
This is a Next.js application using Tailwind CSS and the Conxian Schema library.

```bash
pnpm install
pnpm dev
```

## Readiness Gates
- **SSR Safety**: All client-side interactions must use the `"use client"` directive.
- **Schema Alignment**: Must utilize `@conxian/schemas` for all domain objects.
- **Auth Hardening**: Enforces role-based access control via the Gateway AuthStore.
