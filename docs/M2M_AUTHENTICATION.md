# Gateway M2M Authentication

> **Status:** implemented, on `main`. M2M JWT/dual modes remain **unactivated in
> production** (default `legacy`). This documents the Gateway verifier; it is not
> a production-acceptance or activation claim.

## Where this fits

The organization M2M contract is defined by **`conxius-platform`**
(`docs/M2M_AUTHENTICATION.md` and the `2026-07-22-issue-1160-m2m-jwt-auth`
OpenSpec). That contract issues short-lived, scoped HS256 service JWTs. The
Gateway is a **verifier** of that contract — it never issues tokens.

- Issuer (authority): `conxius-platform`.
- Verifiers: `conxian-gateway` (`internal/api/src/m2m_auth.rs`) and
  `conxian_market` (`src/org_m2m.ts`). Both accept the same tokens.
- A machine therefore holds **one** credential the whole ecosystem verifies.

## Token contract (verified exactly)

A compact JWS presented as `Authorization: Bearer <token>`:

| Aspect | Requirement |
| --- | --- |
| `alg` | exactly `HS256` (no `none`, `HS384`, `HS512`, asymmetric) |
| `typ` | exactly `JWT` |
| `kid` | **must be absent** (single configured secret; not a JWKS contract) |
| `iss` | exactly the configured issuer (`GATEWAY_JWT_ISSUER`) |
| `aud` | exactly the configured audience (`GATEWAY_JWT_AUDIENCE`); a JSON array is rejected |
| `sub` | a registered service id (see below) |
| `scope` | space-delimited; known scopes only; no duplicates; includes `m2m:internal`; subset of the service's ceiling |
| `iat` / `nbf` / `exp` | integer numeric dates; TTL **60..900 s**; skew within **0..60 s** |
| `jti` | non-empty, ≤ 128 chars, no whitespace/control characters |

Registered service ids: `gateway`, `elizaos`, `nexus`, `orbit`, `wallet`, `ui`,
`admin-dashboard`, `pulse-bos`. (`external` cannot hold a JWT.)

Verification is **fail-closed**: any deviation returns an error and never
authorizes a request.

## Bearer modes

`M2M_GATEWAY_AUTH_MODE` selects which credential kinds are accepted:

| Mode | Accepts | Notes |
| --- | --- | --- |
| `legacy` (default) | the static `API_TOKEN` only | previous behaviour; unchanged |
| `dual` | the static `API_TOKEN` **or** a valid M2M JWT | for migration |
| `jwt` | a valid M2M JWT only | static token not accepted |

An **unknown** mode value is rejected (deny), and a `dual`/`jwt` request whose
JWT configuration is missing or invalid **fails closed** — it never silently
downgrades to legacy.

Strict Bearer precedence: a present-but-invalid `Authorization` header is never
rescued by another credential.

## Per-route scope enforcement

Authenticating a token is not authorizing a route. After a token verifies, the
middleware resolves a **required scope** from a deny-by-default method/path
matrix (`required_scope_for_request` in `internal/api/src/auth.rs`) and denies
with `403` unless the identity holds it. Only then is the verified identity
attached to request extensions for downstream handlers.

- Unmapped routes and method mismatches **deny** JWT credentials.
- Admin release/governance mutations require `write:admin` / `write:governance`.
- Treasury/settlement mutations require their write scopes; reads require the
  matching read scope.
- **Every new route added to a bearer-auth router must have an explicit scope
  mapping and a route-coverage test.**

This closes the authorization defect tracked in
[#896](https://github.com/Conxian/conxian-gateway/issues/896) (a verified token
could otherwise reach any protected route).

## Configuration

| Variable | Purpose |
| --- | --- |
| `M2M_GATEWAY_AUTH_MODE` | `legacy` \| `dual` \| `jwt` (default `legacy`) |
| `GATEWAY_JWT_SECRET` | shared HS256 secret, ≥ 32 bytes; must match the issuer |
| `GATEWAY_JWT_ISSUER` | expected `iss` (default `conxian-platform`) |
| `GATEWAY_JWT_AUDIENCE` | expected `aud` (default `conxian-gateway`) |
| `GATEWAY_JWT_CLOCK_SKEW_SECONDS` | `0..60` (default `30`) |

Set values only through the approved secret boundary; never commit them.

## Tests

`internal/api/tests/m2m_auth_conformance.rs` mirrors the platform's adversarial
cases: algorithm/`kid` downgrade, bad signature, wrong `iss`/`aud`, unknown
service, missing claims, expired/future outside skew, TTL bounds, scope
ceiling/duplicate/unknown/missing-`m2m:internal`, bearer bounds, the mode
acceptance matrix, and route-scope authorization. Fixture secrets are generated
per run — no hard-coded cryptographic value.

## Activation gate

Do **not** activate `dual`/`jwt` in production until: the exact deployed source
has a genuine independent non-author security review; the production mode and
configuration are read back through a secret-safe channel; positive/negative
protected-route journeys, rollback and soak are recorded. See
[#896](https://github.com/Conxian/conxian-gateway/issues/896).

## See also

- `conxius-platform` `docs/M2M_AUTHENTICATION.md` — the authoritative contract.
- `internal/api/src/m2m_auth.rs` — the verifier.
- `internal/api/src/auth.rs` — the middleware and scope matrix.
