# Gateway environment reference

Maps every key in `.env.example` to the component that consumes it, its default
development value, and how a secret should be stored. This is documentation of the
existing surface, not an activation: no key here is set to a live value by this file.

> Secret handling: values shown as `sentinel_*` are **non-secret development
> placeholders**. In any deployed environment, supply real values through a secret
> manager — never commit them. Recommended stores, in order of preference:
> HashiCorp Vault (or a cloud secret manager) → Kubernetes `Secret` (sealed or with
> an external-secrets operator) → Render/GitHub Actions environment secrets.

## Chain observation

| Key | Consumer | Dev default | Notes |
|---|---|---|---|
| `BITCOIN_RPC_URL` | engine (Bitcoin listener) | public mainnet RPC | Use a local/regtest node for dev. |
| `BITCOIN_RPC_USER` / `BITCOIN_RPC_PASS` | engine | empty | **Secret** when the node requires auth; set both or neither. |
| `BITCOIN_CORE_SHADOW_OBSERVATION_ENABLED` | engine | `false` | Opt-in read-only shadow observation. |
| `BITCOIN_SYNC_INTERVAL` | engine | `10` | Seconds. |
| `BABYLON_API_URL` | engine (Babylon adapter) | empty | Blank fails explicitly when header data is requested. |
| `MEMPOOL_ORCHESTRATOR_INTERVAL` | engine (mempool orchestrator) | `30` | Seconds. |
| `MEMPOOL_STUCK_THRESHOLD_SECS` | engine | `300` | Age before a pending tx is "stuck". |
| `MEMPOOL_MAX_FEE_BUMP_ATTEMPTS` | engine | `3` | RBF/CPFP bump cap. |
| `MEMPOOL_MAX_FEE_RATE_SAT_VB` | engine | `150` | Upper fee cap for bumps. |
| `MEMPOOL_MIN_BUMP_INCREMENT_SAT_VB` | engine | `2` | Minimum per-bump increment. |
| `STACKS_RPC_URL` | engine (Stacks adapter) | public Hiro API | |
| `STACKS_SYNC_INTERVAL` | engine | `30` | Seconds. |
| `ALEX_API_URL` | engine (ALEX adapter) | public ALEX API | |
| `ALEX_VENUE_MANIFEST_PATH` | engine | empty | Path to a pinned venue manifest. |

## RGB

| Key | Consumer | Dev default | Notes |
|---|---|---|---|
| `RGB_MODE` | engine (RGB adapter) | `disabled` | `disabled` / `shadow` / `active`; active fails closed on unknown contracts. |
| `RGB_NODE_URL` | engine | `http://localhost:8080` | Plain HTTP accepted only for loopback. |
| `RGB_STASH_PATH` | engine | empty | Native stockpile directory. |
| `RGB_ESPLORA_URL` | engine | empty | Required with a native stockpile in Active mode. |
| `RGB_ISSUER_POLICY_PATH` | engine | empty | Pinned issuer policy file. |

## API and auth

| Key | Consumer | Dev default | Notes |
|---|---|---|---|
| `API_PORT` | api | `3000` | Listen port. |
| `API_TOKEN` | api | `sentinel_API_TOKEN` | **Secret.** Static bearer token for legacy mode. |
| `ONBOARDING_TOKEN` | api (onboarding) | `sentinel_ONBOARDING_TOKEN` | **Secret.** No-card onboarding gate. |
| `HMAC_SECRET` | api | `sentinel_HMAC_SECRET` | **Secret.** Request signing. |
| `FIAT_WEBHOOK_SECRET` | api (fiat webhooks) | `sentinel_FIAT_WEBHOOK_SECRET` | **Secret.** |
| `SETTLEMENT_INGRESS_SECRET` | api (settlement ingress) | `sentinel_SETTLEMENT_INGRESS_SECRET` | **Secret.** |
| `OFFLINE_QUEUE_SECRET` | api | `sentinel_OFFLINE_QUEUE_SECRET` | **Secret.** |
| `ORACLE_PUBKEY` | api | `sentinel_ORACLE_PUBKEY` | Public key (not secret). |
| `GATEWAY_PUBLIC_BASE_URL` | api | `https://api.conxian-labs.com` | Used in discovery/links. |
| `M2M_GATEWAY_AUTH_MODE` | api (m2m_auth) | `legacy` | `legacy` / `dual` / `jwt`; `jwt`/`dual` fail closed if JWT config is missing. |
| `GATEWAY_JWT_SECRET` | api | `sentinel_GATEWAY_JWT_SECRET` | **Secret.** Required when mode is `jwt`/`dual`. |
| `GATEWAY_JWT_ISSUER` | api | `conxian-platform` | Expected `iss`. |
| `GATEWAY_JWT_AUDIENCE` | api | `conxian-gateway` | Expected `aud`. |
| `GATEWAY_JWT_CLOCK_SKEW_SECONDS` | api | `30` | Bounded skew. |

## Fiat on-ramp providers (all disabled by default)

| Key | Consumer | Dev default | Notes |
|---|---|---|---|
| `RAMP_MODE` / `RAMP_API_KEY` | api (fiat) | `disabled` / `sentinel_*` | API key is a **secret**. |
| `STITCH_MODE` / `STITCH_CLIENT_ID` / `STITCH_CLIENT_SECRET` | api (Stitch) | `disabled` | Client secret is a **secret**. |
| `OZOW_MODE` | api (Ozow) | `disabled` | |
| `ALCHEMY_PAY_MODE` / `ALCHEMY_PAY_APP_ID` | api (Alchemy Pay) | `disabled` | |
| `BANXA_MODE` | api (Banxa) | `disabled` | |
| `INFOBIP_MODE` / `INFOBIP_API_KEY` / `INFOBIP_BASE_URL` | api (Infobip) | `disabled` | API key is a **secret**. |

## Compose lane tokens

`docker-compose.yml` defines three lanes; each reads a distinct API token so a
lane never shares credentials:

| Lane | Service | Token variable |
|---|---|---|
| Community sovereign | `sovereign-node` | `SOVEREIGN_API_TOKEN` (default `local-dev-token`) |
| Business managed | `business-gateway` | `BUSINESS_API_TOKEN` (default `business-dev-token`) |
| Enterprise | `enterprise-gateway` | `ENTERPRISE_API_TOKEN` (default `enterprise-dev-token`) |

The defaults are non-secret and dev-only; set real tokens via the secret manager in
any deployed environment.

## Bootstrapping secrets (dev / CI)

Generate non-production values for a local run; never reuse them in production:

```bash
# HMAC / queue / webhook secrets
for k in HMAC_SECRET OFFLINE_QUEUE_SECRET FIAT_WEBHOOK_SECRET SETTLEMENT_INGRESS_SECRET; do
  printf '%s=%s\n' "$k" "$(openssl rand -hex 32)"
done

# A local API token
printf 'API_TOKEN=%s\n' "$(openssl rand -hex 24)"
```

## License matrix (SBOM)

The Gateway is split by the two-tier model in `LICENSING.md`. The CycloneDX SBOM
is produced by `.github/workflows/release.yml` (`cargo-cyclonedx` +
`scripts/normalize_release_sbom.py`) and attached to each release as
`conxian-gateway-<version>.cdx.json`.

| Component | Path | License | Viral? |
|---|---|---|---|
| `conxian_core` | `pkg/conxian-core` | MIT | No |
| `conxian-cli` | `cmd/conxian-cli` | MIT | No |
| `conxian_compliance` | `internal/compliance` | AGPL-3.0-or-later | **Yes (§13 network clause)** |
| `conxian_engine` | `internal/engine` | AGPL-3.0-or-later | **Yes** |
| `conxian_api` | `internal/api` | AGPL-3.0-or-later | **Yes** |
| `gateway` | `cmd/gateway` | AGPL-3.0-or-later | **Yes** (links the AGPL crates) |

The runtime container image is built from this repository (`build: .` in
`docker-compose.yml`), so its license is the AGPL-3.0-or-later of the linked
crates. The authoritative per-dependency license set is the released CycloneDX
SBOM; this table records only the first-party boundary.

## See also

- `LICENSING.md` — the two-tier license rationale (`ADR-009`).
- `docs/CAPABILITY_DISCOVERY.md` — discovery surfaces.
- `docs/DEPLOY_MODES.md` — managed vs sovereign deployment.
