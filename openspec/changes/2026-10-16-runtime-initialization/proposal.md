# Reproducible container initialization

Refs: #466, Conxian/conxian-business#1390, Conxian/conxian-business#1423.

Build the actual Gateway source once with `--locked`, replacing the dummy-source
cache build. Exclude local state, credentials and generated files from the Docker
context. Honor `PORT` when `API_PORT` is absent; reject invalid explicit ports.

Preserve persistent single-writer state, authentication, disabled provider modes,
tenant boundaries, signing and independent release gates. Validate port behavior,
the existing default/mock/WireMock suites and container initialization in CI.
No service placement, secret, DNS or production authorization is changed.
