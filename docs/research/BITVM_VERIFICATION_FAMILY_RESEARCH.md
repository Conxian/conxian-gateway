# BitVM Verification Family: Evidence Review & Integration Strategy

**Status:** BitVM v1 Live (T2 Boundary) · BitVM3 Research (T3) | **Lines:** 210 + 160
**Last refreshed:** 2026-08-07 | **Session:** 49

---

## Executive Summary

BitVM is a Bitcoin-native verification paradigm that enables arbitrary
computation verification on Bitcoin without soft forks. The approach uses
optimistic rollup-style challenge-response games backed by Bitcoin transactions.
Two variants exist:

- **BitVM v1 (Groth16):** Uses Groth16 zero-knowledge proofs verified through a
  bisection challenge game. The Gateway has a **live T2 Boundary adapter** with
  injectable `Groth16Verifier` trait, BN254 envelope parsing, and fail-closed
  defaults.

- **BitVM3:** A CPU-based approach using RISC-V instruction verification through
  BitVMX. The Gateway has a **research-only T3 stub** (issue #189) that
  explicitly fails all verification paths.

**Decision:** BitVM v1 remains T2 Boundary until a production Groth16 verifier
backend is integrated. BitVM3 remains T3 Research pending 9 documented
promotion gates.

---

## 1. BitVM v1 (Groth16)

### 1.1 Architecture

```
BitVmAdapter (210 lines)
    │
    ├─ ChainAdapter::verify_state_proof() — DELIBERATELY CRIPPLED
    │   └─ Always returns VerifierUnavailable
    │   └─ Forces callers through proper Groth16 path
    │
    ├─ verify_groth16_envelope(proof_bytes) → public entry point
    │   └─ Parses canonical Groth16 envelope
    │   └─ Validates structure + network match
    │   └─ Delegates to injected Groth16Verifier
    │
    └─ Groth16Verifier trait (injectable)
        └─ MockGroth16Verifier (default, test-only)
        └─ Production backend: NOT YET IMPLEMENTED
```

### 1.2 Gap: Production Groth16 Verifier

The adapter's architecture is complete — parsing, validation, fail-closed
defaults — but no production `Groth16Verifier` exists. The mock verifier
always returns `true`.

**Promotion gates:**
1. Select Groth16 verification library (bellman, arkworks, or lambdaworks)
2. Implement `ProductionGroth16Verifier` with BN254 curve support
3. Pass public Groth16 test vectors
4. Wire into Gateway (replace MockGroth16Verifier)
5. Promote to T1

---

## 2. BitVM3 (BitVMX-CPU)

### 2.1 Status

Explicitly fail-closed research stub. All verification returns
`ConxianError::VerifierUnavailable`. The `prepare_unsigned_transaction()`
response self-documents: `"status": "research_only"`, `"production_supported":
false`.

### 2.2 9 Promotion Gates (from adapter doc)

1. BitVM3 protocol spec stable (no longer pre-release)
2. Production-grade RISC-V executor with reproducible traces
3. Audited BitVMX bridge contract on Bitcoin testnet
4. Positive verification vectors (known-valid proofs)
5. Negative verification vectors (known-invalid proofs)
6. State diff validation against expected post-state
7. Challenge-period monitoring in Gateway
8. Fee-bump strategy for challenge-response transactions
9. TEE integration path for accelerated verification

### 2.3 Tracker

- Issue #189: BitVM3 adapter (open)
- BitVMX evaluation: `experiments/` directory
- Cross-reference: `BITVM3_BITVMX_EVIDENCE_AND_TRIAGE.md`

---

## 3. Decision Gates

| Gate | Adapter | Status |
|------|---------|--------|
| Groth16 envelope parsing | BitVM v1 | ✅ Live |
| Groth16 structural validation | BitVM v1 | ✅ Live |
| Production Groth16 verifier | BitVM v1 | ❌ T1 blocker |
| RISC-V execution traces | BitVM3 | ❌ T3 |
| Challenge-period monitoring | BitVM3 | ❌ T3 |
| Audit (both) | BitVM v1 + v3 | ❌ T1/T3 |

---

## 4. Cross-References

- **ADAPTER_FAMILY_STRATEGY.md:** BitVM v1 at T2 Boundary, BitVM3 at T3 Research
- **BITVM3_BITVMX_EVIDENCE_AND_TRIAGE.md:** BitVM3 evaluation (#189)
- **Issue #189:** BitVM3 adapter tracking
- **BitVM v1 code:** `internal/engine/src/bitcoin/bitvm_adapter.rs` (210 lines)
- **BitVM3 code:** `internal/engine/src/bitcoin/bitvm3_adapter.rs` (160 lines)
