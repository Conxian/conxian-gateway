# Research: Nostr Wallet Connect (NWC) (CON-1267)

## 1. Protocol (NIP-47)
NWC allows applications to request payments from a user's wallet over the Nostr protocol.

- **Connection String**: `nostr+walletconnect://<pubkey>?relay=<relay>&secret=<secret>`
- **Events**:
  - **Request (Kind 23194)**: Encrypted request from client to wallet.
  - **Response (Kind 23195)**: Encrypted response from wallet to client.
- **Methods**: `pay_invoice`, `make_invoice`, `lookup_invoice`, `get_balance`.

## 2. Implementation Path
- **Transport**: Implement NIP-47 transport in `internal/api/src/nostr.rs`.
- **Encryption**: Use NIP-44 (or NIP-04 fallback) for event encryption.
- **Integration**: Wire NWC into the `LightningAdapter` to allow non-custodial payment authorization via the dashboard.

## 3. Library Selection
- Use `nostr-sdk` and `rust-nostr` crates for protocol handling.
- Leverage existing `secp256k1` integration for signing and encryption.
