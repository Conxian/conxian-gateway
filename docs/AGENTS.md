# Conxian Agent Guidelines

## Conxian Gateway Architecture

The Conxian Gateway consolidates core Bitcoin/Stacks state logic (internal/engine) and API/Auth layers (internal/api) into a singular, audit-ready Rust binary.

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
