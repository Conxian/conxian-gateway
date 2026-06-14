# Adapter Family Strategy (CON-709 / CON-710 / CON-711)

## Overview
To support "Universal Chain Support", Conxian Gateway utilizes an adapter-family strategy that groups chains by their architectural primitives (UTXO, EVM, Account-based).

## 1. UTXO Family (Bitcoin, Liquid)
- **Shared Logic**: Transaction building using PSBT (BIP-174), descriptor-based wallet management, and fee estimation via mempool observation.
- **Divergence**: Liquid requires handling of Confidential Transactions (blinding) and Elements-specific opcodes.

## 2. EVM Family (Ethereum, Rootstock, Base)
- **Shared Logic**: JSON-RPC (eth_*) for state lookup, EIP-1559 fee markets, and Solidity contract interaction patterns.
- **Divergence**: Rootstock (RSK) requires specific handling for Bitcoin merged mining finality and the Powpeg interface.

## 3. Stacks (Nakamoto)
- **Logic**: Clarity contract calls, sBTC peg-in/out observation, and Nakamoto-era block finality (fast blocks anchored to Bitcoin).

## 4. Trust Tiers and Readiness
- **Pilot (Tier 2)**: Core logic implemented, tests in simulation, shadow-mode enabled. (RSK, Liquid)
- **Build-now (Tier 1)**: Full production-grade implementation, mainnet-ready, performance-tuned. (Bitcoin, Stacks)
- **Research (Tier 3)**: Feasibility study, architectural RFC, no active execution. (Babylon, BitVM)
