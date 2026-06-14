# Partner Lane Adapter Research (CON-712 / CON-713)

## 1. Babylon (Partner) - CON-712
- **Focus**: Bitcoin Staking and Security Sharing.
- **Integration Pattern**: Babylon requires monitoring for staking transactions on Bitcoin L1 and verifying finality providers.
- **Adapter Strategy**: The adapter should expose interfaces to "prepare_staking_transaction" and "verify_finality_proof".
- **Status**: Researching integration with Babylon's finality gadget.

## 2. BitVM (Partner) - CON-713
- **Focus**: Optimistic Fraud Proofs and Arbitrary Computation on Bitcoin.
- **Integration Pattern**: BitVM integration involves monitoring state root commitments and managing the lifecycle of potential fraud proofs.
- **Adapter Strategy**: The adapter acts as a bridge for "commit_state_root" and "verify_optimistic_proof" using BitVM2-style SNARK verifiers.
- **Status**: Aligning with the BitVM2 state-root verification logic in the compliance layer.
