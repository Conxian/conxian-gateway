# Runtime initialization

`API_PORT` remains the explicit HTTP-port override. With no override, the hosting
platform's `PORT` is used; with neither variable present the default remains 3000.
Invalid explicit ports stop startup instead of falling back to a different port.

The image builds the actual source once with `cargo build --release --locked`.
The Docker context excludes environment files, local state, keys and build output.
Authentication and exclusive local state ownership remain mandatory. Provider
credentials are required only for the provider modes actually enabled; disabled
providers do not establish available payment/compliance operations.

With Docker available, run:

```sh
docker build --tag conxian-gateway:init .
python3 scripts/container_init_smoke.py --image conxian-gateway:init
```

The disposable check exercises hosting-port binding, missing/incorrect token
denial, authenticated discovery, refusal of a second state writer and restart.
It uses an isolated local state mount and generated test-only credentials. It
does not establish settlement, chain synchronization, stateful business-transaction
recovery or production entitlement. The matching CI workflow has read-only access.

Production activation retains the current durable-storage, per-operation provider,
tenant, key provisioning, source/artifact, review, promotion and acceptance gates.
The existing Blueprint's persistent disk and single-writer deployment assumptions
must be fulfilled before any hosted runtime is represented as operational.

The Blueprint and disposable probe use the actual public route
`/api/v1/health`. This route is liveness only; it does not attest RPC health,
settlement finality, provider qualification or production readiness.

Within the approved operator secret boundary, run:

```sh
python3 scripts/startup_secret_preflight.py
```

The preflight reads the injected environment locally and reports only known
missing/invalid key names and an exit code. It does not export values,
fingerprints, lengths, unrelated environment data or provider responses. Its
contract tests compare the required/conditional secret set with Rust startup.
It covers secret presence and the existing webhook-domain separation check;
it does not assess secret strength, provisioning authority, filesystem,
network, database, provider entitlement or release acceptance. Disabled
providers remain unavailable even when the minimum startup-secret set passes.
Smoke-test failures report only fixed phase labels.

The Blueprint now prompts for only those five unconditional startup secrets.
Credentials for disabled providers and optional managed authentication are
provisioned separately when their operations are qualified. Render preserves
existing environment variables omitted from a Blueprint; omissions do not
rotate or delete existing credentials. Its initial creation flow prompts for
every `sync: false` entry, so optional credentials are not listed as startup
requirements. See the [Blueprint environment-variable reference](https://render.com/docs/blueprint-spec#setting-environment-variables).

