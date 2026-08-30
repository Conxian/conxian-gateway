# BitVM3 Adapter — Gap Analysis & Implementation Roadmap

> **Session 57+ | 2026-08-06**
>
> Tracks the BitVM3 adapter gap identified in [Gateway issue #189](https://github.com/Conxian/conxian-gateway/issues/189) and addressed structurally in [PR #322](https://github.com/Conxian/conxian-gateway/pull/322).

## Current State

| Component | Status | Evidence |
|-----------|--------|----------|
| **BitVM3 Adapter** | ✅ Research placeholder (registry-wired, fail-closed) | `internal/engine/src/bitcoin/bitvm3_adapter.rs` — fail-closed, 4 contract tests; registered as `bitvm3` in the Gateway `multi_chain` registry |
| **BitVM Adapter (Groth16)** | 🟡 Boundary | `bitvm_adapter.rs` — BN254 envelope, backend-neutral; MockGroth16Verifier only |
| **BitVM2 Adapter** | 🟡 Boundary | Top-level `bitvm_adapter.rs` — role/encoding/instance validation; SDK path fixed (PR #322) |
| **BitVMX-CPU Eval** | 🔬 Research | `tools/bitvmx-eval/` — isolated subprocess evaluator; not in production dep graph |
| **Groth16 Production Backend** | ❌ Not wired | `groth16_verifier.rs` — contract defined; no pairing library, no production verifier |
| **RISC Zero Verifier** | 🟡 Unwired | `risc0_verifier.rs` — adapter exists; no runtime integration |

## What's Missing for BitVM3 Production Readiness

The structural placeholder is in place. Before any production wiring:

### 1. External Dependencies (blocked)
- **Stable BitVM3/GC SDK**: No public release, API, or audited implementation exists (IACR ePrint 2026/933 is paper/protocol only)
- **BitVMX-GC**: Targeting 2026; no stable public revision (closed source as of Feb 2026)
- **Garbled circuit verifier**: GOAT `bitvm2-gc` and `garbled-snark-verifier` are research references with licensing, resource, and validation blockers

### 2. Gateway-Internals

Already landed (no SDK required):
- ✅ `BitVm3Adapter` re-exported from `conxian_engine` (`internal/engine/src/lib.rs`)
- ✅ `bitvm3` variant registered in the `multi_chain` registry in `main.rs`, so
  `/api/v1/chains/bitvm3/*` is acknowledged and fails closed (HTTP `501`,
  `code: verifier_unavailable`, `authoritative: false`)
- ✅ API-level regression tests cover verify / prepare / height for the lane

Remaining (when a stable garbled-circuit SDK ships):
- Define BitVM3-specific Groth16/GC envelope schema (extends or parallels existing BN254 envelope)
- Add positive/negative test vectors for garbled-circuit verification
- Add HTTP route with fail-closed semantics (follows PR #278 pattern)

### 3. Cross-Repository Gates (from canonical triage)
| Gate | Status | Owner |
|------|--------|-------|
| Platform #1187: simulation success quarantine | ❌ Open P0 | conxius-platform |
| Nexus #169: reconcile BLS12-381 ↔ BN254 verifier | ❌ Open P1 | conxian-nexus |
| Enclave #202: production proof capability gate | ❌ Open P0 | conxius-enclave-sdk |

### 4. Promotion Gates (do not proceed until all satisfied)
- [ ] Stable maintained API/release + reconciled license
- [ ] Explicit curve, circuit, VK registry, public-input, and root/state-transition contract
- [ ] Pairing, curve-point, and subgroup validation
- [ ] Positive and negative vectors (mutated proof/input/root, malformed envelope cases)
- [ ] Complete SPV/dispute/disablement semantics
- [ ] Reproducible resource measurements on approved hardware
- [ ] Independent security review and verified deployment evidence
- [ ] Explicit ownership for cryptographic verification, evidence normalization, enclave attestation, policy enforcement, and client presentation

## Next Steps (when unblocked)

1. Pin a specific BitVM3/GC SDK revision with reconciled license
2. Define the BitVM3 envelope schema (curve, circuit ID, VK registry, public input order)
3. Generate independent positive/negative test vectors
4. Implement `verify_state_proof` with actual garbled-circuit verification
5. Add HTTP route behind feature gate
6. Run resource benchmarks (wall time, peak RSS, proof size)
7. Cross-reference with Platform #1187, Nexus #169, Enclave #202

## Monitoring Triggers

Re-open implementation scoping when:
- A public BitVM3 or BitVMX-GC SDK ships with a stable release/API
- Independent positive/negative test vectors are reproducible
- License is reconciled and approved
- Cross-repo gates (Platform, Nexus, Enclave) are resolved

Until then: keep `BitVm3Adapter` as research-only, fail-closed, and tracked in #189.

---

*Generated during Session 57+ (2026-08-06) as part of the A+B expansion phase.*
