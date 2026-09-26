# Conxian Ecosystem: Org-Wide SLA Positioning, Governance & Structural Research

**Version:** 0.1.5
**Date:** September 2026
**Status:** Approved Architectural Policy
**Authors:** Conxian Architecture & Governance Working Group

---

## Executive Summary

A critical evaluation of the Conxian GitHub ecosystem (Conxian organization) reveals a broad, ambitious attack surface—spanning foundational protocol primitives (`lib-conxian-core`), hardware execution enclaves (`conxius-enclave-sdk`), sovereign wallets (`conxius-wallet`), cross-chain bridges (`conxian-nexus`), and enterprise payment/messaging gateways (`conxian-gateway` targeting ISO 20022 and legacy system bridging).

Attempting to enforce rigid, commercial-grade Service Level Agreements (SLAs) across this stack under typical open-source funding models requires a realistic reckoning of structural constraints, cash flow realities, developer bandwidth, and protocol risk.

This document establishes the strategic, financial, and legal framework governing Service Level Agreements across all Conxian organization repositories and commercial offerings.

---

## 1. The Core Reality: Open-Source Funding vs. Enterprise SLAs

Commercial SLAs (e.g., 99.9% uptime, 1-hour critical incident response times, guaranteed patch delivery) are legally and financially binding guarantees backed by penalty clauses or service-credit refunds.

In the open-source and early-stage sovereign infrastructure space:

1. **Sporadic & Lumpy Funding:** Pre-seed budgets, protocol grants, or early trial revenues do not support a 24/7/365 follow-the-sun incident response engineering team.
2. **The "Maintainer Bottleneck":** When core architecture relies heavily on lean teams or solo system architects, a high-severity incident can easily paralyze both product development and support operations.
3. **Asymmetric Risk:** Sovereign infrastructure (Bitcoin L1 layers, hardware security modules, zero-custody TEE wallets, and cryptographic verifiers) carries severe security implications. A rushed patch to meet an arbitrary SLA window can introduce catastrophic regression or key-material exposure vectors.

---

## 2. Conxian Stack vs. Peer Open-Source Companies

Comparing Conxian’s architectural footprint to other open-source infrastructure and Web3/Enterprise bridge players (such as OpenZeppelin, WalletConnect, or early-stage protocol tooling organizations) highlights distinct operational gaps and requirements:

| Metric / Dimension | Typical Open-Source Infrastructure Player | Conxian Ecosystem Profile |
|---|---|---|
| **Surface Area** | Focused (often 1–3 tightly coupled libraries or a single SDK). | Expansive: Spans L1 primitives (`lib-conxian-core`), hardware execution (`conxius-enclave-sdk`), mobile wallets (`conxius-wallet`), and enterprise financial middleware (`conxian-gateway` / ISO 20022 / CIPS / mBridge). |
| **Support Model** | "Community-best-effort" via GitHub Issues and Discord; paid enterprise tiers are strictly bounded. | Bridges decentralized tech with legacy financial systems (ISO 20022, camt.053, pacs.008, SAP/Oracle ERPs), which inherently invites enterprise expectations. |
| **SLA Strategy** | No SLAs for free tiers. Paid enterprise tiers cover integration support, not underlying immutable protocol uptime (since L1/L2 networks dictate base availability). | Risk of over-committing to uptime/support metrics on components that rely on external consensus layers (Bitcoin/Stacks) and hardware enclaves. |

---

## 3. Risk & Benefit Analysis (Pros & Cons)

### Pros (Why Teams Are Tempted to Offer Commercial SLAs)
* **Enterprise Credibility:** Traditional financial institutions and corporate partners require formal support frameworks before piloting middleware like `conxian-gateway` or ISO 20022 / camt.053 adapters.
* **Monetization Lever:** Strict SLAs justify premium enterprise licensing, support contracts, or managed-service fees (crucial for transitioning from grant/pre-seed funding to sustainable revenue).
* **Internal Discipline:** Forces rigorous testing, CI/CD automation, and automated security workflows (such as Sentinel guardrails and contamination guards) to prevent regressions.

### Cons & Risks (Why Org-Wide SLAs Are Structurally Dangerous)
* **The "Liability Trap":** If a bug in `conxius-enclave-sdk` or `conxian-nexus` causes a state-verification failure or delayed transaction settlement, a rigid commercial SLA could expose an early-stage entity to crippling liabilities or refund claims.
* **Exhaustion of Engineering Capital:** Forcing lean engineering teams to monitor PagerDuty rotations or meet rigid ticket-resolution windows takes precious hours away from core systems architecture and protocol hardening.
* **External Dependency Failures:** Conxian's components depend on external network states (Bitcoin L1 congestion, Stacks network behavior, Android StrongBox/TEE firmware updates, Canton ACS state proofs). Guaranteeing uptime or resolution speeds on systems not fully controlled is a structural mismatch.

---

## 4. Strategic Recommendations & Strategic Policy

### Rule 1: Tiered Support & SLA Matrix (Separate Protocol from Enterprise Wrapper)
* **Core Protocol / Public Repositories (Conxian Org):** Explicitly state **No SLA**. Use standard open-source disclaimers (MIT / Apache 2.0). Support is provided on a community-best-effort basis via GitHub Discussions and Issues.
* **Enterprise / Gateway Tier (`conxian-gateway` / Managed B2B Adapters):** Only offer SLAs under a signed B2B commercial contract or paid enterprise tier (managed via `conxian-business`). Crucially, limit the scope of the SLA to integration support, configuration assistance, and business-hour response times (e.g., Next-Business-Day response), **never absolute network uptime**.

### Rule 2: Redefine SLA Metrics for Early-Stage Deep Tech
* Instead of guaranteeing "system availability" (which is impossible for decentralized/sovereign layers), frame commitments around **Response Time to Acknowledgement** or **Target Patch Advisory Windows** for non-critical bugs.
* Exclude force majeure events, L1 network halts, block reorganization delays, and hardware-vendor-specific enclave/StrongBox API deprecations from liability.

### Rule 3: Protect the Core via Automated Guardrails
* Lean heavily on automated workflows (CI pipelines, automated test suites, contamination guards, release verifiers, and strict staging environments) to ensure that code moving into production candidate branches (like `main` and `staged`) undergoes rigorous automated validation before human intervention is required.

---

## 5. Enterprise ERP Webhook SLA & Telemetry Implementation

To support enterprise clients operating under signed B2B contracts without exposing the core protocol to liability, `conxian-gateway` incorporates deterministic **ERP Webhook Callback Telemetry** (`camt.053` OData v4 sync):

1. **Exponential Backoff Retry Strategy:** Failed OData v4 webhook callbacks (e.g., HTTP 5xx or connection timeouts) calculate retry intervals deterministically:
   $$\text{delay}(n) = \min(\text{base\_delay} \times 2^n, \text{max\_delay})$$
2. **Failure Classification & Non-Blocking Isolation:** Failed ERP dispatches mark the transaction status as `SYNC_FAILED_HTTP_<CODE>` or `DISPATCH_ERROR`, while preserving local statement generation without crashing or blocking the gateway runtime.
3. **Auditability & Proof of Delivery:** All OData v4 payloads carry base64-encoded XML statements with explicit metadata (`@odata.context`, `statement_id`, `status`).

---

## 6. Conclusion & Verdict

Attempting to run a 24/7 commercial-grade SLA across the entire public Conxian GitHub surface with pre-seed/early-stage funding is too ambitious and dangerous. **Protect the core protocol layer with strict open-source disclaimers, and restrict any form of SLA guarantees exclusively to paid enterprise integration wrappers where revenue can directly fund dedicated support capacity.**
