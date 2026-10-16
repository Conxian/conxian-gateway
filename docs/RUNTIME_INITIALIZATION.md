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
