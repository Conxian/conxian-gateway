# Startup contract and diagnostic evidence truth

Owners: Gateway #466/#548 and Business #1411/#1423/#1390. Phase R1; live qualification remains R3/R4.

The README still reports a fixed historical secret count, while current runtime, preflight and Blueprint distinguish unconditional from enabled-provider requirements. CLI doctor also labels URL parsing as reachability and claims deployment readiness; status probes a historical health route and prints an arbitrary response body. These representations must match what is actually checked.

Add an environment-independent machine-readable startup contract description, a generated README contract block and conformance coverage bound to current Rust/deployment declarations. Preserve the existing preflight result schema and secret-value boundary. Correct status/doctor claims, use the actual liveness route with bounded, non-redirected requests and redact response/error bodies. Do not add credential provisioning, secret-strength claims, live provider probes or release authorization.

Acceptance: minimum and conditional requirement counts, canonical-token alias behavior, source/Blueprint agreement and documentation drift fail closed in CI. Native CLI tests cover the correct route, non-success/redirect/timeout and response redaction; unobserved connectivity stays explicitly unobserved. Existing runtime, provider, persistence, auth, pricing and promotion behavior remains outside this change.
