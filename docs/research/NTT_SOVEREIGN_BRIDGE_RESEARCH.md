# NTT Relayer & Sovereign Bridge Adapters

**Status:** Live (T1 Production) | **Lines:** 189 (relayer) + 126 (RSK) + 93 (Citrea)
**Last refreshed:** 2026-08-07 | **Session:** 49

---

## Executive Summary

The Native Token Transfer (NTT) Relayer enables cross-chain attestation
forwarding for sovereign bridges — protocols that move native assets between
blockchains without a trusted third party. The Gateway acts as an attestation
relayer, forwarding verified bridge events between chains and enforcing trust
policies on cross-chain transfers.

Three adapters implement the `ChainAdapter` trait for NTT-compatible chains:
- **Rootstock (RSK):** 126 lines, live JSON-RPC adapapter with bridge peg-in
  verification via `bridge_getStateForBtcReleaseClient`
- **Citrea:** 93 lines, live JSON-RPC adapter with `eth_blockNumber` for height
- **Strata:** 43 lines, stub placeholder with no cryptographic verification

---

## 1. NTT Relayer Core

### 1.1 Architecture

```
NttRelayer (191 lines)
    │
    ├─ run_until_shutdown(shutdown)
    │   └─ Polling loop with tokio watch channel
    │   └─ poll_interval configurable
    │
    ├─ process_ntt_events()
    │   └─ Iterates source-height registry
    │   └─ Calls ChainAdapter::get_latest_height()
    │
    ├─ submit_vaa(source_height)
    │   └─ Attestation forwarding for new heights
    │   └─ Gated by trust policy
    │
    └─ evaluate_trust_policy_from_env()
        └─ CONXIAN_NTT_TRUST_METADATA_ENV env var
        └─ TrustPolicyDecision::Allow / Block
        └─ Block → silent skip with warning (fail-closed)
```

### 1.2 Trust Policy

The relayer uses a self-managed trust policy (no Wormhole guardian network).
Trust metadata is evaluated from the `CONXIAN_NTT_TRUST_METADATA_ENV`
environment variable. On `Block` decision, VAA submission is silently skipped.
On `Allow`, attestations are forwarded.

This follows the "route without touch" principle — the Gateway verifies and
routes but does not custody bridge assets.

---

## 2. Chain Adapters

### 2.1 Rootstock (RSK) — 126 lines

| Capability | Status | Notes |
|-----------|--------|-------|
| JSON-RPC client | ✅ Live | `reqwest::Client` to RSKj Vetiver 9.0.3 |
| Block height | ✅ Live | `eth_blockNumber` RPC with hex parsing |
| Peg-in verification | ✅ Live | `bridge_getStateForBtcReleaseClient` — most sophisticated stub |
| Chain ID | ✅ Live | 30 (mainnet) / 31 (testnet) |
| Trust tier | T1 | Merged-mining finality |

The Rootstock adapter has the most sophisticated `verify_state_proof()` among
the NTT adapters: it queries the RSK bridge contract's `getStateForBtcReleaseClient`
RPC for BTC peg-in verification. This is real bridge state, not a placeholder.

### 2.2 Citrea — 93 lines

| Capability | Status | Notes |
|-----------|--------|-------|
| JSON-RPC client | ✅ Live | `reqwest::Client` |
| Block height | ✅ Live | `eth_blockNumber` RPC |
| ZK proof verification | ⬜ Shadow | Checks `zk_proof` field presence, not validity |
| Chain ID | ✅ Live | 5115 |
| Trust tier | T2 | ZK proof not verified |

Citrea is a Bitcoin ZK rollup. The adapter queries EVM-compatible JSON-RPC
endpoints. Proof verification is shadow-mode only — checks that `zk_proof`
field is present but does not verify the zero-knowledge proof.

### 2.3 Strata — 43 lines

| Capability | Status | Notes |
|-----------|--------|-------|
| JSON-RPC client | ❌ None | No `reqwest::Client` |
| Block height | ⬜ Stub | Always returns 0 |
| ZK proof | ⬜ Shadow | Checks `batch_root` presence |
| Trust tier | T3 | Placeholder only |

Strata is a ZK rollup bridge by Alpen Labs. The adapter is a pure placeholder
— no RPC client, no cryptographic verification, no block height.

---

## 3. Cross-References

- **ADAPTER_FAMILY_STRATEGY.md:** NTT at T1, RSK at T1, Citrea at T2, Strata at T3
- **CON-1268:** NTT relayer integration ticket
- **CON-711:** Rootstock NTT adapter
- **BRICS_FINANCIAL_SYSTEMS_RESEARCH.md:** Cross-chain routes for BRICS corridors

---

## 4. Recommendations

1. **Promote Citrea to T1** when ZK proof verification is implemented.
2. **Implement Strata adapter** — needs real JSON-RPC client and proof verification.
3. **Add NTT metrics:** `conxian_gateway_ntt_events_forwarded_total`,
   `conxian_gateway_ntt_events_blocked_total`.
