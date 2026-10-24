# Gateway capability discovery

Gateway is the commercial middleware boundary for conxian-labs.com. Its basic self-hosted routing interfaces consume public conxian.org protocol primitives. Bitcoin L1 is the trust/finality root; external chains, layers, and assets remain supported through independently evidenced adapters. There is no native Conxian token or staking incentive.

Authenticated `GET /api/v1/capabilities` and managed `GET /api/v1/agent/capabilities` report only registrations in the running process's adapter map. They retain the existing bearer/managed authentication and rate-limit layers and return `Cache-Control: no-store`. Discovery does not call RPCs, sign, broadcast, or assert health.

Each adapter is `registered`, with `health: not-observed`, `production_support: not-evidenced`, and no qualified operations. Adapter IDs can represent families/protocols and must not be treated as exact chain/network IDs. An empty map stays empty. No SDK registry or evidence-row count is copied into runtime support.

The legacy `supported_chains` field remains compatible and is explicitly labelled `registered-adapters-only`. It is not signing authorization or proof of production support.

Gateway #526 tracks the next boundary: an authenticated, deployment-bound Core operation manifest, exact chain/network identity, TTL, evidence verification, and policy validation. Until then this registration manifest is diagnostic discovery, not qualified capability negotiation. Do not update dependency pins to an unreviewed Core branch to advertise completion.
