# Research: World ID Verification (CON-66 / CON-1284)

## 1. Verification API
World ID verification is handled via the Worldcoin Developer Portal API.

- **Endpoint**: `POST https://developer.worldcoin.org/api/v2/verify/{app_id}`
- **Request Body**:
  ```json
  {
    "nullifier_hash": "0x...",
    "merkle_root": "0x...",
    "proof": "0x...",
    "verification_level": "orb",
    "action": "my_action",
    "signal_hash": "0x..."
  }
  ```
- **Success Response**:
  ```json
  {
    "success": true,
    "action": "my_action",
    "nullifier_hash": "0x...",
    "created_at": "..."
  }
  ```

## 2. Implementation Strategy
- Implement a `WorldIdVerifier` in `internal/compliance/src/identity.rs`.
- Use `minreq` with HTTPS features to call the verification endpoint.
- Store `APP_ID` and `ACTION_ID` in the gateway configuration.
- Map World ID errors (e.g., `invalid_proof`, `already_verified`) to `ConxianError::Compliance`.

## 3. Integration Plan
1. Update `IdentityResolutionRequest` to include necessary World ID fields if they don't fit in `identifier`/`signature`.
2. Replace `resolve_worldid` placeholder with live API call.
3. Add unit tests with mock server responses.
