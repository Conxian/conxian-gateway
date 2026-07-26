# ALEX Settlement Evidence and Activation Gate

**Review date:** 2026-07-22; implementation/evidence checkpoint updated 2026-07-26
**Scope:** `Conxian/conxian-gateway#247` and the cross-repository evidence needed before ALEX can be used as a production settlement rail.

## Executive decision

ALEX remains **read-only, shadow, and research-gated**. Conxian must not execute or enable a mainnet ALEX swap until the evidence gates in this document are satisfied and the activation policy is approved. **Change under review:** the unmerged branch can discover and prepare data, but it does not establish a production signer, broadcast, receipt, reserve, or reconciliation path.

This artifact is a decision record, not an implementation or launch approval. Simnet fixtures, synthetic treasury values, fake transaction IDs, historical release claims, API pair discovery, and API-reported TVL are not production evidence.

## Change under review for issue `#247`

The unmerged `charlie/alex-venue-policy-gate-247` branch adds the reviewable core/engine boundary below. These statements describe the change under review, not behavior already merged to `main`, and they do not change the executive decision above:

- **Change under review:** `pkg/conxian-core/src/alex_settlement.rs` defines serde-friendly network, exact principal, asset, pool, helper, quote/source, venue/admin, policy, decision, rejection, unsigned-intent, and status types.
- **Change under review:** policy evaluation requires a supported network, exact allowlisted asset/pool/helper tuple, network-qualified principals, explicit nonzero `min_dy`, quote expiry/freshness, matching policy/config revisions, expected helper code hash, known active admin state, bounded price impact, and an explicit exposure cap. The proposed 20% exposure value is represented only as `ALEX_EXPOSURE_SAFETY_CEILING_BPS`; a concrete policy still has to set `max_exposure_bps`.
- **Change under review:** the intent hash is a versioned SHA-256 over canonical fields and is documented as a future persistence/reconciliation dedupe key. This slice has no persistence and therefore does not claim replay prevention.
- **Change under review:** `observed` source data may produce `UNSIGNED_PREPARED`; `fixture` and `unverified` sources produce `SHADOW_ONLY` and cannot produce a settled or completed status.
- **Change under review:** a strict `alex-venue-manifest-v1` JSON model carries one exact venue snapshot and its complete policy. Unknown fields, unsupported versions, blank IDs/revisions, invalid time windows, non-observed evidence, revision/network/config/code-hash drift, and a policy that omits the exact tuple are rejected. The repository ships no candidate mainnet manifest or guessed principal.
- **Change under review:** `POST /api/v1/alex/prepare` is bearer-authenticated, fail-closed, unsigned/read-only evidence preparation. It is not a paid or x402 capability gate. It loads no split principal/hash defaults, rechecks manifest validity at request time, obtains evidence through `AlexClient`, invokes `AlexSettlementPolicy::evaluate`, and only then creates an engine-owned opaque approval capability for raw payload construction.
- **Change under review:** production compatibility quote evidence remains `UNVERIFIED` and incomplete. It supplies no fabricated price impact, exposure, observed timestamp, or proof, so production preparation stops at stable `409 ALEX_VERIFICATION_REQUIRED` when a manifest is loaded; without a manifest it returns stable 503. Fully observed deterministic test fakes prove the policy-gated construction path.
- **Change under review:** legacy `POST /api/v1/alex/swap` always returns stable `409 ALEX_EXECUTION_DISABLED` and never calls the raw payload builder. `GET /api/v1/alex/quote` remains explicitly read-only/unverified and no longer supplies ticker or zero-amount defaults.
- **Change under review:** `ALEX_VENUE_MANIFEST_PATH` is optional. Missing, unreadable, invalid, stale, or network-mismatched manifests disable only ALEX preparation and do not crash gateway startup. Logs contain only stable reason codes and safe manifest identity/revision context.

This is an implementation boundary and test contract, not evidence of a deployed ALEX pool/helper, reserves, signer, broadcast, receipt, reconciliation, or production readiness.

## Evidence taxonomy

| Label | Meaning | Examples in this review |
| --- | --- | --- |
| **Verified revision state** | Source or checked repository behavior at a cited revision. | The historical gateway handler prepared a payload; current FROST boundaries fail closed. |
| **Observed live data** | A dated network/API observation made during this review. | `api.alexlab.co/v1/quote` returned HTTP 404 on 2026-07-22. |
| **Historical/stale claim** | A prior issue, PR, UI, script, or secondary report that is not sufficient for launch. | PR #351's “production-ready” wording; the rehearsal script's expected 501. |
| **Unknown** | Not proven by the reviewed evidence. | Current USDCx ALEX pool reserves, deployment height, code hash, and controlled swap receipt. |

## Historical baseline and change-under-review matrix

Historical baseline source revision: [`6838d872`](https://github.com/Conxian/conxian-gateway/tree/6838d872513b681cf88f07fc5431f02b856b6d0e). Links pinned to that revision establish only the old baseline. Rows explicitly labeled “change under review” describe the unmerged `charlie/alex-venue-policy-gate-247` branch and intentionally have no self-referential commit permalink. The document itself was previously merged in [PR #273](https://github.com/Conxian/conxian-gateway/pull/273) at `0dc6390ddbfbb4d74c472da3a86e90aa2397524f`.

| Capability | Current behavior | Evidence classification / launch consequence |
| --- | --- | --- |
| Gateway quote route (change under review) | `GET /api/v1/alex/quote` requires explicit `token_x`, `token_y`, and nonzero `amount`, returns source/status metadata, and labels the result non-execution-eligible. It remains a private bearer-authenticated, read-only compatibility route. | **Unmerged review boundary.** No ticker or amount defaults remain, but the route is not a verified production quote until the upstream contract and response semantics are proven. |
| Configured upstream quote path | `AlexRpcClient` retains `${ALEX_API_URL}/v1/quote?...` only as an explicitly unverified compatibility probe. | **Observed/verified.** Official [ALEX REST API references](https://docs.alexlab.co/developers/integrations/api-references) do not document this legacy path. Environment probes were inconsistent: a 404 was observed on 2026-07-22, while another environment received 403/WAF behavior by 2026-07-26. This inconsistency reinforces that `/v1/quote` cannot be the sole quote, liquidity, TVL, or execution evidence; it does not prove that all ALEX market data is unavailable. |
| Policy-gated preparation (change under review) | Bearer-authenticated `POST /api/v1/alex/prepare` requires a verified manifest, coherent and fresh observed quote/config/exposure evidence, successful policy evaluation, and nonzero `min_dy` before an opaque engine-owned capability can cross the raw unsigned builder boundary. An arbitrary legacy `x-402-payment` header grants no additional capability. | **Unmerged review boundary.** The production compatibility adapter cannot satisfy this gate because it remains unverified and does not synthesize missing fields. No signing or broadcast follows preparation. |
| Legacy swap route (change under review) | `POST /api/v1/alex/swap` returns stable `409 ALEX_EXECUTION_DISABLED`. | **Unmerged review boundary.** The route does not invoke a payload builder and cannot return an unsigned payload, signer result, transaction ID, or receipt. |
| Payload preparation (historical baseline) | At the pinned `6838d872` revision, `execute_alex_swap` first calls `parse_gateway_x402_payload`, then calls `build_swap_payload` and returns `{ "status": "prepared", "payload": ... }`; the payload hard-coded `SP3K8BC0PPEVCV7NZ6QSRWPQ2JE9E5B6N3PA0XBHT.alex-swap-helper-v1` and defaulted `min_dy` to `1`. | **Historical/verified at the pinned revision.** [`handlers.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/api/src/handlers.rs), [`alex.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/engine/src/stacks/alex.rs) |
| Payload preparation (change under review) | The unmerged branch removes the hard-coded helper and `min_dy` default, requires explicit manifest-backed configuration and policy checks, and keeps preparation unsigned. | **Unmerged review boundary.** Repository-relative paths: `internal/api/src/handlers.rs`, `internal/engine/src/stacks/alex.rs`, `pkg/conxian-core/src/alex_settlement.rs`. |
| Execution (historical baseline) | At `6838d872`, `AlexRpcClient::execute_swap` built the payload, logged that secure signer-enclave integration was pending, and returned an error. No signer key was used, no Stacks transaction was signed, and no transaction was broadcast. | **Historical/verified at the pinned revision.** No verified signer/broadcast/receipt path exists. [`alex.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/engine/src/stacks/alex.rs) |
| Execution (change under review) | The unmerged branch keeps `/api/v1/alex/swap` stably disabled before any builder invocation. | **Unmerged review boundary.** Repository-relative paths: `internal/api/src/handlers.rs`, `internal/api/src/routes.rs`. |
| Simulation (historical baseline) | At the pinned revision, `SimulatedAlexClient` returned quote `100` and the literal `txid_alex_simulated_swap_rehearsal`. | **Historical/verified test behavior, not production evidence.** [`alex.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/engine/src/stacks/alex.rs) |
| Simulation (change under review) | The unmerged branch rejects simulated execution and does not return a transaction ID, receipt, or unsigned settlement payload. | **Unmerged review boundary.** Repository-relative path: `internal/engine/src/stacks/alex.rs`. |
| Rehearsal script (change under review) | The branch rehearsal uses bearer authentication only, sends no x402 proof, asserts fail-closed preparation, and confirms stable disabled execution; it does not expect HTTP 501 or claim a receipt. | **Unmerged rehearsal-only behavior.** A failure status is expected until exact helper/policy configuration and all upstream evidence gates are independently approved. |
| Treasury monitor (historical baseline) | At `6838d872`, the monitor polled a fixed simulated-shaped `sBTC -> STX` quote, fell back to `0.5` on error, initialized `1,000,000` STX, `10.5` BTC, and `$5,000,000` sBTC liquidity, then simulated growth/FX/yield. | **Historical/verified at the pinned revision, explicitly non-production.** These values are synthetic metrics and fallback proxies, not evidence of reserves, TVL, liquidity, or yield. [`treasury/mod.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/engine/src/treasury/mod.rs) |
| SDK surface | Gateway `@conxian/client-sdk` is private version `0.1.4` and has no typed ALEX quote, intent, status, or reconciliation surface. The only discovered package named `@conxian/sdk` is a sandbox/mock package at `0.4.0-alpha`. | **Verified current inventory.** Do not publish a production ALEX SDK surface until the gateway contract and evidence policy are approved. |

## Cross-repository ownership and dependency matrix

| Owner / artifact | Current responsibility or finding | Canonical evidence |
| --- | --- | --- |
| Gateway issue `#247` | Primary request for ALEX settlement research. It contains the 80/2/18 distribution claim and an `ERC-8183 -> ALEX` flow, but those are requirements to validate, not evidence of deployment. | [conxian-gateway#247](https://github.com/Conxian/conxian-gateway/issues/247) |
| Gateway ALEX engine (historical baseline) | At `6838d872`, the engine exposed quote and payload-builder interfaces, a simulated client, and an execution method that built a payload before failing closed at the missing signer boundary. | [`internal/engine/src/stacks/alex.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/engine/src/stacks/alex.rs) |
| Gateway ALEX core/engine (change under review) | The unmerged branch adds typed settlement policy and manifest models plus the opaque approval capability required to cross the raw unsigned payload-builder boundary. | **Unmerged review boundary.** Repository-relative paths: `pkg/conxian-core/src/alex_settlement.rs`, `internal/engine/src/stacks/alex.rs`. |
| Gateway API/routes/config (historical baseline) | At `6838d872`, the API exposed bearer-protected quote and swap routes, parsed x402 data in the swap handler, and configured `ALEX_API_URL`; there was no `/api/v1/alex/prepare` route or `ALEX_VENUE_MANIFEST_PATH`. | [`handlers.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/api/src/handlers.rs), [`routes.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/api/src/routes.rs), [`config.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/cmd/gateway/src/config.rs) |
| Gateway API/routes/config (change under review) | The unmerged branch keeps `/api/v1/alex/quote` read-only and bearer-authenticated, adds policy-gated bearer-authenticated `/api/v1/alex/prepare`, disables `/api/v1/alex/swap`, retains `ALEX_API_URL`, and adds optional `ALEX_VENUE_MANIFEST_PATH`. No split helper/pool/hash defaults remain. | **Unmerged review boundary.** Repository-relative paths: `internal/api/src/handlers.rs`, `internal/api/src/routes.rs`, `cmd/gateway/src/config.rs`. |
| Gateway treasury/core models (historical baseline) | At `6838d872`, the repository contained synthetic ALEX-anchored metrics and the `AlexSwapRequest` shape; neither was custody or reserve proof. | [`treasury/mod.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/internal/engine/src/treasury/mod.rs), [`pkg/conxian-core/src/lib.rs`](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/pkg/conxian-core/src/lib.rs) |
| `Conxian/Conxian` launch-readiness | Explicitly keeps ALEX production release wiring disabled; simnet fixtures are not mainnet evidence. It lists candidate reserve/helper principals and requires exact deployment, funding, pool, controlled-swap, and rollback evidence. | [`docs/ALEX_LAUNCH_READINESS.md`](https://github.com/Conxian/Conxian/blob/main/docs/ALEX_LAUNCH_READINESS.md) |
| `Conxian/Conxian#526` / Linear `CON-1529` | Active but blocked P1 activation gate (Linear state: `Triage`; labels: `blocked`, `needs-decision`). Requires exact network/token/wrapper/listing/pool/helper/deployer/liquidity evidence, a nonzero-`min-dy` controlled swap, receipts/postconditions/balance/reserve reconciliation, and negative/rollback tests. | [Conxian#526](https://github.com/Conxian/Conxian/issues/526), [Linear CON-1529](https://linear.app/conxian-labs/issue/CON-1529/p1-research-and-implement-verified-alex-production-activation) |
| `Conxian/Conxian#489` | Closed historical testnet-only integration; it does not prove a current mainnet deployment or liquidity path. | [Conxian#489](https://github.com/Conxian/Conxian/issues/489) |
| `Conxian/Conxian#500` | Production oracle source, decimal, staleness, fallback, and deployment wiring remain incomplete; ALEX activation is not thereby approved. | [Conxian#500](https://github.com/Conxian/Conxian/issues/500) |
| `Conxian/Conxian#468` | CXLP mint/burn and pool integration remain incomplete. | [Conxian#468](https://github.com/Conxian/Conxian/issues/468) |
| `Conxian/Conxian#536` | CLP custody/positions/fees/IL/rollback execution remains incomplete and depends on #468/#500; it explicitly does not make ALEX #526 complete. | [Conxian#536](https://github.com/Conxian/Conxian/issues/536) |
| `Conxian/Conxian#522` | Merged correction that removes ALEX production publish/registration from release artifacts and retains simnet fixtures. | [PR #522](https://github.com/Conxian/Conxian/pull/522), [merge commit `30d54e35`](https://github.com/Conxian/Conxian/commit/30d54e35bd55b88950a1a2dfc88dcb80ea8f3c1f) |
| `Conxian/Conxian#351` | Historical release PR whose “ALEX Lab Mainnet Integration” / “production-ready” wording was not backed by the current launch-readiness evidence. Treat as a superseded overclaim. | [PR #351](https://github.com/Conxian/Conxian/pull/351) |
| `conxian_market` economics | Funding policy contains multiple fee layers and an ALEX pool strategy, but not an approved gateway execution policy. | [`FUNDING_AND_ECONOMICS.md`](https://github.com/Conxian/conxian_market/blob/3ca050110a92a13be8d197f82de203aa883cfc32/docs/research/FUNDING_AND_ECONOMICS.md), [`WALLET_TREASURY_FEASIBILITY.md`](https://github.com/Conxian/conxian_market/blob/ffd5ac52f9af494b6d972503d71217ce22fcbe20/docs/research/WALLET_TREASURY_FEASIBILITY.md) |
| `conxian_market#6` | Market work item covering the funding/economics direction; it is not evidence of approved fee routing or ALEX liquidity. | [conxian_market#6](https://github.com/Conxian/conxian_market/issues/6) |
| `conxius-enclave-sdk#180` | Closed documentation/runbook issue. Current FROST code is a structural, fail-closed boundary; production DKG, nonce/share verification, aggregation, attestation, and audit evidence remain open. | [issue #180](https://github.com/Conxian/conxius-enclave-sdk/issues/180), [`src/protocol/frost.rs`](https://github.com/Conxian/conxius-enclave-sdk/blob/e8e6d090dfe59bba22842a627b9d7ef86fd9b996/src/protocol/frost.rs), [FROST integration guide](https://github.com/Conxian/conxius-enclave-sdk/blob/e8e6d090dfe59bba22842a627b9d7ef86fd9b996/docs/guides/FROST_TREASURY_INTEGRATION.md) |
| `conxius-wallet` | Stacks signing and MuSig2 paths still fail closed or use placeholders; they do not prove a production ALEX signer or 3-of-5 FROST operation. | [`StacksManager.kt`](https://github.com/Conxian/conxius-wallet/blob/02db401e037b6d121dd0c1e4f05893c3e33963f9/android/core-bitcoin/src/main/kotlin/com/conxius/wallet/bitcoin/StacksManager.kt), [`Musig2Manager.kt`](https://github.com/Conxian/conxius-wallet/blob/02db401e037b6d121dd0c1e4f05893c3e33963f9/android/core-bitcoin/src/main/kotlin/com/conxius/wallet/bitcoin/Musig2Manager.kt) |
| `lib-conxian-core` | FROST share generation/distribution/aggregation checks shape but return typed `Unsupported`; it is not audited signing crypto. | [`src/protocol/frost.rs`](https://github.com/Conxian/lib-conxian-core/blob/506147597f93dff1e11de9fe97bdc439c244cb79/src/protocol/frost.rs) |
| `conxian-nexus` | Observation/proof layer and gateway transport/RPC with an explicit observation/execution boundary. | [`README.md`](https://github.com/Conxian/conxian-nexus/blob/41ea522e7fa85a185884d5cc98f98c77b1234573/README.md) |
| `conxian_ui` | SDK page advertises `v2.1.0-stable` and `PRODUCTION_READY`, but no ALEX contract surface was found and the marketing claim is not reconciled with package/repo evidence. | [`src/app/sdk/page.tsx`](https://github.com/Conxian/conxian_ui/blob/a72ce543cad0553164f3eed85e3f9b1d2186c86e/src/app/sdk/page.tsx) |
| `conxius-orbit` | CLI/deployment/operations toolkit; no verified ALEX-specific execution surface was found. | [`package.json`](https://github.com/Conxian/conxius-orbit/blob/9508f45399a945bc0afd4d2443cf6435f520724e/package.json) |
| `conxian-business` sandbox | The only discovered `@conxian/sdk` package is `0.4.0-alpha`, explicitly a mock SDK for sandbox examples with simulated statuses/txids/attestations. | [`package.json`](https://github.com/Conxian/conxian-business/blob/24f0f8b397ae203e7f09572880cfe73a70eb2b46/cxn-sandbox/packages/@conxian/sdk/package.json), [`src/index.ts`](https://github.com/Conxian/conxian-business/blob/24f0f8b397ae203e7f09572880cfe73a70eb2b46/cxn-sandbox/packages/@conxian/sdk/src/index.ts) |

**Access note:** The cited `conxian_market` and `conxian-business` repositories are private organization sources. Their links require authenticated organization access; unauthenticated readers cannot independently verify those snapshots.

## Projects, issues, and SDK findings

- No GitHub Project, project item, milestone, assignee, or linked PR was found for gateway `#247` at review time. The repository has no GitHub Projects returned by the checked project inventory, and #247 has no project item or milestone.
- The issue's `CON-1474` link is unresolved/not found in the checked Linear workspace. The active but blocked gate is [`CON-1529`](https://linear.app/conxian-labs/issue/CON-1529/p1-research-and-implement-verified-alex-production-activation), which links to `Conxian#526`.
- At the pinned historical revision, the gateway package [`@conxian/client-sdk` 0.1.4](https://github.com/Conxian/conxian-gateway/blob/6838d872513b681cf88f07fc5431f02b856b6d0e/packages/client-sdk/package.json) had no ALEX-specific quote, settlement-intent, status, or reconciliation API.
- **Change under review:** the bounded typed contract remains internal/core only on the unmerged branch (`pkg/conxian-core/src/alex_settlement.rs`).
- The only discovered package named `@conxian/sdk` is the sandbox/mock [`0.4.0-alpha`](https://github.com/Conxian/conxian-business/blob/24f0f8b397ae203e7f09572880cfe73a70eb2b46/cxn-sandbox/packages/@conxian/sdk/package.json). No production ALEX SDK surface was found.
- After gateway contract approval, the recommended SDK additions are typed `Quote`, `SettlementIntent`, `SettlementStatus`, and `Reconciliation` records that preserve network, asset principals, policy decision, transaction ID, receipt, and failure reason. They must not expose an execution method before the signer/broadcast gate is complete.

## Upstream inventory and source boundaries

The following are direct public sources checked during this review. A source establishes only what it actually documents; repository source, API output, or an audit does not by itself prove a current deployment or reserve state.

| Source | What it is useful for | Boundary |
| --- | --- | --- |
| [ALEX developer docs](https://docs.alexlab.co/developers) and [official links](https://docs.alexlab.co/resources/official-links) | Canonical documentation entry points and official project links. | Documentation can lag deployments; verify against exact network state. |
| [ALEX REST API references](https://docs.alexlab.co/developers/integrations/api-references) | Documents `https://api.alexgo.io` and inventory endpoints including all swaps, pool stats/liquidity, TVL, prices, and orderbook. | The reviewed inventory does not establish the gateway's `/v1/quote` contract; the configured quote request returned 404 on 2026-07-22. |
| [ALEX mainnet contracts/tokens](https://docs.alexlab.co/developers/integrations/networks/mainnet) | Official documented contract names/principals and token inventory. | Names/listings are not deployment-height, code-hash, reserve, or liquidity proof. The reviewed token list did not establish a current USDCx pool. |
| [ALEX testnet contracts/tokens](https://docs.alexlab.co/developers/integrations/networks/testnet) | Testnet reference and reset caveat. | Testnet cannot be promoted to mainnet evidence. |
| [ALEX AMM trading pool](https://docs.alexlab.co/developers/products/alexs-automated-market-maker-amm/trading-pool) and [protocol contracts](https://docs.alexlab.co/developers/alex-contracts/protocol-contracts) | Pool mechanics, token approval/governance, pause/blocklist concepts, and contract roles. | Does not prove the target pool's current parameters or admin state. |
| [ALEX `alex-sdk`](https://github.com/alexgo-io/alex-sdk) and [generated API docs](https://alexgo-io.github.io/alex-sdk/) | Discovery, route/fee calculation, and transaction-construction boundaries; caller remains responsible for signing/broadcast. | SDK output is not a receipt, reserve proof, or Conxian production integration. |
| [ALEX contract source](https://github.com/alexgo-io/alex-v1), including [`alex-reserve-pool.clar`](https://github.com/alexgo-io/alex-v1/blob/dev/clarity/contracts/pool/alex-reserve-pool.clar) | Reference implementation and function shapes. | Source branch is not proof of deployed code, version, height, or code hash. |
| [ALEX public orderbook repository](https://github.com/alexgo-io/alex-orderbook-public) and [orderbook docs](https://docs.alexlab.co/developers/products/what-is-orderbook) | Public orderbook architecture and contract references. | Orderbook is a separate workstream; its public repository is not acceptance evidence for the first AMM quote/intent milestone. |
| [APower FAQ](https://docs.alexlab.co/what-can-you-do/staking/faqs) and [Launchpad docs](https://docs.alexlab.co/what-can-you-do/launchpad) | APower and Launchpad terminology. | Current upstream docs describe APower as non-transferable/non-tradable; do not model it as a transferable settlement asset. |
| [ALEX Clarity Alliance audit](https://cdn.alexlab.co/pdf/ALEX_Clarity_Alliance_2025-05-16.pdf), [ALEX DAMM audit](https://cdn.alexlab.co/pdf/ALEX_DAMM_Audit_2025-05.pdf), and [older Least Authority audit](https://cdn.alexlab.co/pdf/Least_Authority_ALEX_Protocol_Smart_Contracts_Final_Audit_Report.pdf) | Scope-specific findings and audit history. | An audit is not current deployment evidence; scope, remediation, version, and target code hash must match the candidate contracts. [Trading Pool v2 documentation](https://docs.alexlab.co/developers/products/alexs-automated-market-maker-amm/trading-pool-v2) separately says its audit is pending. |
| [2024 ALEX exploit/grant terms](https://terms.alexlab.co/terms-and-conditions-of-alex-protocol-exploit-treasury-grant-program) and [2025 terms](https://terms.alexlab.co/terms-and-conditions-of-alex-protocol-exploit-treasury-grant-program-2025) | Official chronology and asset-denominated loss statements. | The ALEX governance host did not resolve during this review, so no governance or reopening URL is asserted here. Any governance action or new multisig must still be checked independently; it would not prove Conxian's operational signer, target-pool safety, or current liquidity. |
| [Stacks Foundation June 2025 incident statement](https://stacks.foundation/june-6-alex-incident) | Confirms the June 2025 incident was at the ALEX application layer, not a Stacks protocol vulnerability. | It is not a complete ALEX post-mortem or current pool inventory. |
| [Stacks USDCx contracts](https://docs.stacks.co/learn/bridging/usdcx/contracts), [USDCx token](https://docs.stacks.co/learn/bridging/usdcx/contracts/usdcx-token), and [aeUSDC migration](https://docs.stacks.co/learn/bridging/usdcx/bridge-app/migrating-aeusdc) | Exact USDCx principals, role/pause controls, and the distinction between USDCx and legacy Allbridge `aeUSDC`. | A Stacks token contract is not proof that ALEX lists or pools that asset. |
| [Stacks transaction reference](https://docs.stacks.co/reference/stacks.js/stacks-transactions), [unsigned/multisig transaction guide](https://docs.stacks.co/stacks.js/build-transactions), [post-condition overview](https://docs.stacks.co/post-conditions/overview), and [post-condition examples](https://docs.stacks.co/post-conditions/examples) | Read-only calls, unsigned transaction assembly, multisig workflows, deny-mode postconditions, and asset-outflow constraints. | Stacks transaction multisig support does not imply ALEX operational/admin multisig readiness. |

## Asset and pool findings

### 2026-07-26 helper ABI checkpoint

The reviewed live helper ABI uses trait references for token contracts and represents `min-dy` as optional. That upstream shape is compatibility evidence only: Conxian's policy boundary still requires exact network-qualified asset principals and a present, nonzero `min_dy`. An optional upstream ABI argument must never become an omitted or zero Gateway safety value. See the canonical [ALEX protocol contracts](https://docs.alexlab.co/developers/alex-contracts/protocol-contracts), [ALEX contract source](https://github.com/alexgo-io/alex-v1), and [Stacks transaction construction reference](https://docs.stacks.co/reference/stacks.js/stacks-transactions).

### Asset identity

The review must use exact network-qualified principals, not ticker strings:

- Stacks mainnet USDCx documented principal: `SP120SBRBQJ00MCWS7TM5R8WJNTTKD5K0HFRC2CNE.usdcx`.
- For testnet, use the exact principal from the [official USDCx contract page](https://docs.stacks.co/learn/bridging/usdcx/contracts) at execution time; no unverified testnet principal is asserted here.
- `aeUSDC` is the legacy Allbridge bridged USDC described in the [migration documentation](https://docs.stacks.co/learn/bridging/usdcx/bridge-app/migrating-aeusdc); it is not USDCx and must not be substituted by ticker or UI label.
- Normalize `sBTC`, `SBTC`, and `xBTC` only after resolving the exact contract principal, decimals, wrapper/listing status, and network. The same rule applies to `USDC`, `USDCx`, and `aeUSDC`.

### Pool state

No current official ALEX USDCx pool was verified during this review. The official mainnet inventory reviewed on 2026-07-22 did not establish one, and direct unauthenticated requests to some ALEX data endpoints were blocked with HTTP 403 in this environment. That is an evidence limitation, not proof that no pool exists.

An API pair row, orderbook ticker, or `sBTC` pair discovery is not proof of reserves or executable liquidity. Before any activation decision, record all of the following for the exact candidate pool/helper:

1. Network and chain ID.
2. Exact asset principals, decimals, and wrapper/listing status.
3. Pool, helper, router, registry, oracle, and admin principals.
4. Deployment height, deployment transaction ID, and deployed code hash/version.
5. Fee, oracle, registry, pause, blocklist, and admin state at the observation height.
6. Reserves, LP supply, pool weights/factors, and independent read-only reconciliation.
7. Initial-liquidity/funder evidence and current balance snapshots.
8. A controlled transaction with a nonzero `min-dy`, postconditions, signed/broadcast transaction ID, receipt, event logs, balance changes, and reserve/LP reconciliation.

## Security chronology and required controls

The chronology must not collapse two different incidents:

| Period | Evidence-backed distinction | Treatment |
| --- | --- | --- |
| **May 2024** | The official [2024 terms](https://terms.alexlab.co/terms-and-conditions-of-alex-protocol-exploit-treasury-grant-program) place an ALEX exploit on/about 2024-05-14 and state that 13,283,922.62 STX remained unrecovered as of 2024-06-07. Public secondary analysis, including [CertiK's ALEX analysis](https://www.certik.com/blog/alex), associates the event with the XLink bridge/private-key/upgrade-control failure and derives an approximately `$4.3M` figure. | Keep the incident separate from the 2025 protocol/self-listing issue. `$4.3M` is secondary/derived unless an official source is found; use the official STX amount and date for the primary evidence. |
| **June 2025** | The official [2025 terms](https://terms.alexlab.co/terms-and-conditions-of-alex-protocol-exploit-treasury-grant-program-2025) place an exploit on/about 2025-06-06 and list unrecovered assets as of 2025-06-08: 8,403,867.57 STX, 21.85 sBTC, 149,850 aUSD, and 2.80 aBTC. The [Stacks Foundation statement](https://stacks.foundation/june-6-alex-incident) describes this as an ALEX application-layer incident. A [secondary public analysis](https://www.clarityalliance.org/articles/labubu) discusses self-listing verification bypass/pool risk and derives approximately `$8.37M`. | Keep it separate from the 2024 bridge/key incident. `$8.37M` is secondary/derived unless an official source is found; do not use it as a precise official loss figure. |

The gateway activation design must map each risk to a testable control:

- **Allowlists:** exact network, asset principal, pool/helper principal, deployed code hash, and approved function signature; reject ticker-only requests.
- **Postconditions:** use deny-mode postconditions with explicit asset and STX limits. Postconditions constrain outgoing assets; they are not a substitute for balance, receipt, and state reconciliation.
- **Slippage:** require a policy-computed, nonzero `min-dy`; reject zero/default values and stale quotes.
- **Expiry:** bind quote/intent validity to a short deadline and reject replay/expired intents.
- **Signing:** produce unsigned transactions first; require the approved native Stacks multisig/signer path. Stacks transaction multisig capability does **not** prove that ALEX operational/admin multisig is ready.
- **Admin state:** monitor ALEX pause, blocklist, registry, oracle, and upgrade/admin changes; fail closed on unknown state.
- **Exposure:** enforce per-asset and total treasury caps, including the market policy's proposed 20% maximum exposure only after that policy is approved and measurable.
- **Liquidity:** monitor TVL, reserves, LP supply, price impact, and exit depth; do not use an API TVL field as reserve proof.
- **Emergency response:** rehearse LP reduction/exit with bounded amounts, circuit breakers, signer disablement, and rollback/deny behavior before production enablement.
- **Audits:** match current audits to the exact deployed code hash and scope; unresolved or out-of-scope findings block activation.

## Economics and terminology contradictions

The fee policy is not normalized:

| Source | Stated allocation | Consequence |
| --- | --- | --- |
| Gateway `#247` | Builder 80% / Protocol 2% / Ecosystem 18%. | Issue-level proposal only; not approved routing policy. |
| `conxian_market` funding research | Builder revenue 80% / Platform Treasury 10% / Ecosystem 10%, plus a separate 2% protocol fee. The 2% fee is further described as Ops 50% / Founders 30% / Ecosystem 20%. | This conflicts with 80/2/18 and has an additional nested distribution. |
| Other fee language | A separate 50/30/20 distribution is also described in the funding research. | Do not infer which layer applies to an ALEX settlement. |

**Decision:** block fee routing, treasury accounting, and SDK fee fields until one policy is approved, versioned, and tested against exact settlement events. Do not silently merge 80/2/18 with 80/10/10 or 50/30/20.

**2026-07-26 owner decision remains open:** the 80/2/18 versus 80/10/10 economics conflict is not resolved. **Change under review:** the unmerged venue-manifest slice encodes no fee recipients, routing, rounding, or treasury automation.

Terminology rules:

- `sBTC`, `SBTC`, and `xBTC` are different labels until an exact principal and wrapper/listing relationship is proven.
- `USDC`, `USDCx`, and `aeUSDC` are different assets until exact principals and bridge provenance are proven.
- APower is protocol-specific and non-transferable/non-tradable according to current upstream documentation; it is not a transferable settlement asset.
- Launchpad, APower, orderbook, and ERC-8183 escrow are separate workstreams. They are not acceptance criteria for the first verified ALEX quote/settlement-intent milestone.

## Recommended architecture

Use a layered, fail-closed path:

```text
ALEX API/SDK discovery and telemetry only
        |
        v
Direct Stacks read-only calls for authoritative pool/token/admin state
        |
        v
Gateway quote normalization + risk policy + exact-principal allowlists
        |
        v
Unsigned settlement intent with expiry, nonzero min-dy, and deny-mode postconditions
        |
        v
Approved signer / native Stacks multisig boundary
        |
        v
Broadcast
        |
        v
Receipt, event, balance, reserve, LP, and policy reconciliation
```

The ALEX API/SDK may help discover routes, fees, and telemetry, but it must not be the sole authority for balances, reserves, admin state, or settlement completion. The gateway should expose a read-only quote and an unsigned, policy-approved settlement intent before it exposes any execution method. A signer must return a signed transaction only after checking the same intent hash and policy decision. Broadcast and reconciliation must be explicit states, not inferred from payload construction or a returned string.

## Phased work and gates

### Phase 0 — evidence and policy gate

- Resolve the exact mainnet/testnet network, token principals, wrapper/listing relationships, pool/helper/registry/oracle/admin principals, deployment heights, code hashes, and approved contract versions.
- Replace the gateway `/v1/quote` assumption with a verified upstream contract or a direct read-only quote implementation; record request/response schemas and authentication/rate-limit behavior.
- Approve one fee policy and one asset terminology registry.
- Keep ALEX read-only/shadow; no signer or broadcast wiring.

### Phase 1 — quote and settlement-intent milestone

- [x] **Change under review:** add typed quote/source, risk decision, and unsigned settlement-intent records in the gateway core boundary.
- [x] **Change under review:** require exact principal allowlists, quote expiry/freshness, nonzero `min-dy`, exposure caps, expected revisions/code hash, known active admin state, and a deterministic policy decision.
- [x] **Change under review:** add negative tests for wrong network, wrong principal, zero `min-dy`, expired/stale quote, stale code hash/config revision, paused/unknown admin state, excessive price impact, and exposure-cap breach.
- [ ] Add authoritative read-only evidence bundles and deny-mode postconditions; no mainnet execution acceptance yet.

### Phase 2 — signer and controlled testnet execution

- Close the signer/enclave boundary with audited cryptography, nonce/replay protection, signer quorum policy, and hardware/attestation evidence.
- Sign only approved intents; retain unsigned transaction, policy, signature, broadcast, and receipt artifacts.
- Execute a bounded testnet or explicitly approved controlled transaction with nonzero `min-dy`, postconditions, receipt, events, balance changes, and reserve/LP reconciliation.
- Exercise rejection and rollback/exit paths.

### Phase 3 — production activation review

- Re-run all evidence at the intended production height and code hash.
- Verify current admin/pause/blocklist/oracle/registry state, liquidity and exit depth, signer quorum, incident runbook, circuit breakers, and monitoring alerts.
- Require independent review/sign-off from gateway, market policy, wallet/enclave, and risk owners.
- Enable only the approved asset/pool set and only after a small controlled production transaction passes the same evidence bundle.

### Separate workstreams

Launchpad/APower, the ALEX orderbook, and ERC-8183 escrow/settlement integration require their own contracts, ownership, risk, and acceptance criteria. They must not be used to fill missing evidence for the first ALEX quote/settlement-intent milestone. In particular, APower's non-transferability prevents treating it as a generic settlement output.

## Concrete issue/PR breakdown and acceptance evidence

| Work item | Scope | Required evidence |
| --- | --- | --- |
| Gateway `#247` / this research PR | Establish the evidence taxonomy, current capability matrix, source inventory, terminology, and activation decision. | Merged document; no execution enablement; links resolve to exact source paths or explicitly dated observations. |
| `Conxian#526` / `CON-1529` | Mainnet activation gate and cross-repository decision. | Exact network/token/wrapper/listing/pool/helper/deployer/liquidity principals; deployment and funding tx IDs; code hashes; fee/oracle/admin state; controlled nonzero-`min-dy` swap; receipt/postconditions; balance/reserve/LP reconciliation; negative and rollback tests. |
| Gateway follow-up | Replace loose strings with typed quote/intent/status/reconcile contracts and fail-closed policy checks. | Schema fixtures containing network, principals, code hash, quote expiry, min-dy, postconditions, policy decision, transaction ID, receipt, and reconciliation result; tests for all rejection cases. |
| Signer/enclave (`conxius-enclave-sdk#180`) | Production signer/quorum boundary. | Audited FROST or approved signer implementation; 3-of-5 policy; nonce/replay/share verification; attestation; signer disablement; independent review; no placeholder or typed-`Unsupported` path. |
| Wallet and operations | Native Stacks signing, multisig operations, broadcast, monitoring, and emergency exit. | Signed transaction artifacts, broadcast response, receipt lookup, admin-state alerts, exposure/TVL/reserve/LP dashboards, and an exit rehearsal. |
| Market economics / `conxian_market#6` | Normalize fee and asset policies. | Approved versioned policy resolving 80/2/18 versus 80/10/10 plus 2%/50/30/20; event-level accounting tests; no fee routing before approval. |
| Oracle/CLP dependencies (`Conxian#500`, `#468`, `#536`) | Resolve oracle, custody, pool, fee, and rollback prerequisites where they affect the chosen settlement policy. | Exact ownership and deployment evidence; stale/negative oracle tests; custody/position/fee/IL/rollback evidence; explicit dependency sign-off. |
| Separate Launchpad/APower/orderbook/ERC-8183 tracks | Research and implement separately from the first quote/intent gate. | Own issue, contract inventory, risk policy, and controlled evidence; no cross-credit toward ALEX AMM activation. |

The minimum controlled-swap evidence must include **exact asset/pool/helper principals, network, deployment/code-hash evidence, transaction IDs, signed/broadcast responses, receipt and event records, nonzero `min-dy`, deny-mode postconditions, before/after balances, reserve and LP-supply reconciliation, and negative/rollback results**. A simnet fixture, payload, fake txid, historical PR, API pair, or API TVL value cannot replace any item in this list.

## Unknowns and source limitations

- The current ALEX quote endpoint contract is unresolved. Official REST documentation does not document the legacy `/v1/quote` path; probes produced 404 in one environment and 403/WAF behavior in another. Those observations establish that the compatibility integration is not reproducibly verified, not that ALEX has no quote capability.
- Direct unauthenticated access remains environment-dependent. Authentication, WAF, endpoint versioning, and rate-limit behavior require an approved, reproducible access method, and the compatibility API cannot be sole quote/TVL evidence.
- The ALEX governance host did not resolve in the review environment; current governance/reopening state is therefore unknown and is not used as activation evidence.
- No current on-chain read was accepted as proof of a USDCx ALEX pool, reserves, LP supply, admin state, or code hash in this review. The official docs' token inventory is not a negative on-chain proof.
- No signer, broadcast, receipt, or reconciliation path was verified for ALEX. Current FROST, wallet, and core boundaries are fail-closed or placeholder/unsupported.
- At the pinned pre-implementation revision, the gateway handler parsed x402 data before preparing a payload while the rehearsal script omitted that payload and expected 501. **Change under review:** the unmerged branch removes x402 from the ALEX route boundary and rehearsal, uses bearer authentication, and accepts only fail-closed preparation plus stable disabled-execution outcomes; neither behavior is evidence of production execution. Repository-relative paths: `internal/api/src/handlers.rs`, `internal/api/src/routes.rs`, `scripts/alex_rehearsal.sh`.
- Historical dollar figures (`$4.3M` and `$8.37M`) are secondary/derived in the reviewed sources. Official chronology and asset-denominated figures take precedence; unverified incident dollar amounts are not primary activation facts.
- UI marketing claims and historical PR wording are not launch evidence. Current repository state, exact deployed principals/code hashes, controlled transaction artifacts, and independent review are authoritative for activation.
