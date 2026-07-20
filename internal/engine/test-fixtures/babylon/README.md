# Babylon BTC header fixtures

`mainnet_mainchain.json` and `mainnet_tip.json` contain canonical Bitcoin
mainnet heights `0` through `2` in deterministic Babylon `mainchain` and `tip`
response envelopes. The mainchain fixture intentionally preserves Babylon's
tip-first response order.

Canonical Bitcoin data was captured on **2026-07-20** from these direct
Blockstream Esplora endpoints:

- `https://blockstream.info/api/block-height/0`
- `https://blockstream.info/api/block-height/1`
- `https://blockstream.info/api/block-height/2`
- `https://blockstream.info/api/block/000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f/header`
- `https://blockstream.info/api/block/00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048/header`
- `https://blockstream.info/api/block/000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd/header`

The Babylon envelopes were authored offline. Raw headers, derived hashes,
proof-of-work, per-header work, and cumulative-work transitions were
independently verified with the repository-resolved `bitcoin` crate
(`0.32.102`). Tests are deterministic and do not make live network requests.
