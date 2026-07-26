# RFC: RGB Protocol-Rail Adapter

## Status
- **Phase 2 filesystem/consignment, transactional update, and opt-in issuer
  policy library boundaries implemented**
- Issue #228 remains open until the BIP340 policy is wired through an approved
  controlled import surface and a deterministic end-to-end Bitcoin/RGB fixture
  is accepted.

## Context
As part of the Conxian Gateway evolution, we need to support the RGB protocol as a first-class citizen for smart contract and asset logic on Bitcoin. This adapter provides the bridge between the Conxian engine and the RGB node/state.

## Scope
- Protocol-rail adapter contract definition.
- Support for 'disabled', 'shadow', and 'active' rollout modes.
- Integration with node-backed contract lookup via HTTP.

## Adapter Interface

```rust
pub enum RolloutMode {
    Disabled,
    Shadow,
    Active,
}

pub struct RgbAdapterConfig {
    pub mode: RolloutMode,
    pub node_url: String,
    pub stash_path: Option<String>,
    pub esplora_url: Option<String>,
}

#[async_trait]
pub trait RgbAdapter {
    async fn lookup_contract(&self, contract_id: &str) -> ConxianResult<Option<ContractState>>;
    async fn verify_transition(&self, transition_id: &str) -> ConxianResult<bool>;
}
```

## Rollout Modes

### 1. Disabled
- The adapter is a no-op and returns empty/false results.
- No interaction with the RGB node.

### 2. Shadow (Current Goal)
- The adapter performs lookups and verifications but results do not affect the main execution path.
- Node failures and unknown responses may use an explicitly simulated result.
- Simulation is never used by Active mode.

### 3. Active
- The native RGB stockpile is authoritative for contract presence and
  verification boundaries. The JSON metadata cache is descriptive only.
- Active does not use HTTP or simulation as consensus proof. Missing native
  configuration and stockpile errors fail closed.
- With `rgb-native`, `RGB_STASH_PATH` and `RGB_ESPLORA_URL` are required.

## Implementation Details
- Concrete implementation in `internal/engine/src/bitcoin/rgb_adapter.rs`.
- Uses `minreq` on a blocking task for lightweight HTTP communication with the RGB node.
- All non-disabled paths pass through one shared contract-ID normalizer. Native
  builds use `rgb::ContractId` for full Baid64 validation and canonicalization;
  default-feature builds still reject empty, legacy, prefixless, and malformed
  IDs at the boundary and accept only the canonical chunked `contract:` shape.
  An optional Baid64 mnemonic fragment is accepted but removed before stash,
  HTTP, and response propagation. Legacy `rgb:` values are not consensus-valid
  IDs.
- `StashResolver` owns an exact-pinned
  `rgb_persist_fs::StockpileDir<bp::seals::TxoSeal>` and reloads it after
  successful imports. Corrupt contract directories fail closed at startup.
- Unknown-contract imports run in a same-filesystem staging directory and are
  atomically promoted only after `rgb::Contracts::consume_from_file` succeeds.
  Existing-contract imports copy only the target `*.contract` directory into
  isolated same-filesystem state, run the unchanged consensus importer there,
  sync and reload the verified candidate, then promote it through a durable
  phase journal and retained old-contract backup. The descriptive metadata,
  seal registry, issuers, and unrelated contracts are not copied or mutated by
  this transaction. The per-contract transaction directory is acquired with an
  atomic create, so overlapping updates for the same contract fail closed
  instead of promoting candidates derived from stale state.
- Startup scans durable RGB update journals before loading `StockpileDir`.
  Prepared or backed-up transactions restore the prior verified contract;
  a durably persisted `promoted` phase is the irreversible commit point, so
  promoted transactions retain the verified replacement and finish cleanup.
  Post-commit cleanup failures never enter the old-state rollback path: the
  live `StockpileDir` is reloaded before returning a clear committed-but-cleanup-
  incomplete/uncertain error. The promoted journal is retained until backup
  cleanup is synced and transaction-directory deletion is attempted. If the
  final stockpile-root sync fails after deletion, the current namespace may no
  longer contain the journal; the committed live state is still reloaded, and
  restart is safe whether the promoted journal reappears or remains removed.
- Recovery accepts only the canonical transaction-directory basename derived
  from the validated journal contract ID. Transaction, staged, backup, and
  contract paths used for recovery must be direct non-symlink directories;
  prefixed non-directory entries, unsupported/corrupt journals, unsafe or
  mismatched contract directories, and path/file-type inspection failures fail
  closed before mutation. A promoted journal without its committed live
  contract also fails closed rather than restoring the pre-commit backup.
- Candidate files and each relevant directory are synced before rename.
  Pre-commit failures restore/reload the prior state; successful promotion and
  all reported post-commit cleanup failures reload the live `StockpileDir`.
- These boundaries compensate for the pinned `rgb-persist-fs` behavior that
  creates or mutates contract persistence before `evaluate_commit` completes
  and exposes no filesystem rollback transaction. Exact RGB RC pins remain
  unchanged.
- Descriptive metadata remains in an atomic JSON cache, but it is never used
  by `verify_transition` or the Active proof path.
- The wallet-owned auth-token registry stores only strict-encoded seal
  definitions and RGB auth tokens. It validates the committed token, is
  idempotent for identical replays, and rejects overwrite attempts and corrupt
  persistence atomically. The stash is a local-filesystem trust boundary, not
  an encryption boundary: on Unix the resolver restricts its owned directory
  to the owner and writes registry files with owner read/write permissions;
  file data is synced before rename and the parent directory is synced after
  atomic replacement.
- `import_consignment` preflights the pinned RGB consignment envelope, rejects
  unsigned consignments, invokes the caller-owned issuer signature validator,
  and delegates full operation/codex/witness consensus checks to
  `rgb::Contracts::consume_from_file`.
- `Bip340IssuerPolicy` is an explicit opt-in validator for controlled callers.
  It maps a case-sensitive exact RGB `Identity` string to one pinned
  secp256k1 x-only public key. The accepted `SigBlob` is exactly a raw 64-byte
  BIP340 signature over the exact 32 bytes supplied by the RGB callback for
  `ArticlesId::commit_id()`. There is no second hash, text encoding, algorithm
  inference, or fallback validator. Unknown identities, malformed policies,
  malformed keys/signatures, wrong commitments, and wrong keys reject.
- The policy parser accepts only JSON schema version `1`, algorithm
  `bip340-secp256k1`, and a non-empty issuer allowlist. Unknown fields,
  duplicate identities, non-printable/non-ASCII or empty identities, and
  non-32-byte x-only keys reject. The policy contains public keys only; private
  keys are never loaded or stored by this backend.
- In the pinned `allow_unknown = true` first-contract branch, RGB imports the
  articles and genesis stockpile without invoking the supplied seal resolver.
  Therefore contract genesis/import does not by itself prove wallet-owned seal
  ownership or query Esplora. Paths for already-known contracts retain the
  resolver callback and fail closed when a registered seal is absent or its
  Esplora check is not unspent.
- `export_consignment` serializes only registered RGB terminal auth tokens;
  no identity or other PII is stored in the registry.
- Esplora UTXO queries preserve spent, unspent, not-found, and transport-error
  distinctions. They are not substituted for RGB consensus verification.
- Mempool orchestration does not synthesize an RGB contract ID from a Bitcoin
  transaction ID.

## Setup and Configuration
- `RGB_MODE` defaults to `disabled`.
- `RGB_NODE_URL` defaults to `http://localhost:8080`; plain HTTP is allowed
  only for localhost/loopback development. Embedded credentials are rejected.
- `RGB_STASH_PATH` (a directory, not the old metadata file path) and
  `RGB_ESPLORA_URL` are optional in Disabled/Shadow, but
  must be configured together. Active mode with `rgb-native` requires both.
- `RGB_STASH_PATH` is process-owned Unix local-filesystem state. Before journal
  recovery, `StockpileDir::load`, metadata loading, registry loading, or any
  stockpile/registry mutation, the resolver takes a non-blocking exclusive OS
  lock on `<RGB_STASH_PATH>/.conxian-rgb-owner.lock` and retains it for the
  resolver lifetime. A second gateway using the same stash fails startup
  closed; dropping the owning resolver releases the lock. The lock file is not
  silently unlinked and uses owner read/write permissions.
- Non-Unix builds fail RGB stash resolver startup before creating the stash
  root. Windows and other non-Unix deployments are unsupported until a reviewed
  platform-native implementation can open the ownership lock without following
  symlinks or reparse points and provide equivalent link-safety checks.
- The ownership lock coordinates processes only where the underlying local
  filesystem provides reliable advisory file locking. Do not place
  `RGB_STASH_PATH` on NFS, object-store mounts, or other shared/network
  filesystems, and do not share one stash between containers or hosts.
- Per-contract atomic transaction acquisition remains defense in depth for
  overlapping imports; it does not replace process-lifetime stash ownership.
- Issuer policy loading is a library operation, not an environment variable or
  public HTTP import endpoint. On Unix, `Bip340IssuerPolicy::load_json_file`
  reads at most 64 KiB, rejects non-regular files and paths identified as
  symlinks, and uses no-follow, non-blocking, close-on-exec open semantics
  before checking the opened descriptor is regular. Non-Unix file loading
  fails closed as unsupported until an equivalent handle-level no-follow
  implementation exists. A controlled Unix caller can opt in:

  ```rust
  let policy = Bip340IssuerPolicy::load_json_file("/etc/conxian/rgb-issuers.json")?;
  resolver.import_consignment(&consignment_path, &contract_id, &policy)?;
  ```

  Example policy:

  ```json
  {
    "version": 1,
    "issuers": [
      {
        "identity": "did:example:conxian-rgb-issuer",
        "algorithm": "bip340-secp256k1",
        "xonly_public_key_hex": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
      }
    ]
  }
  ```

  The example key is schema illustration only, not a trusted issuer key.
  Applications must provision reviewed public keys through their own
  configuration/change-control process. The Gateway runtime does not currently
  load this file or expose a state-changing import endpoint.
- Simulation uses only the `contract:` HRI and is explicitly non-consensus.

## Boundary Behavior
- A tracked Bitcoin transaction ID is not an RGB contract ID. The mempool
  orchestrator intentionally skips RGB lookup until a real contract-ID source
  exists.
- `verify_transition` now checks only for a contract successfully persisted by
  the RGB stockpile. The adapter interface does not carry a consignment path,
  so callers requiring full operation verification must use
  `StashResolver::import_consignment`.
- A successful HTTP contract lookup must contain a matching `contract_id`, a
  non-empty `schema_id`, and an object-valued `state`. Empty or mismatched 200
  bodies are errors in Active mode and use Shadow-only simulation fallback.
- Shadow-only HTTP verification responses still require a boolean `valid`
  field and matching optional IDs; they are never Active proof.

## Error Semantics
- `ConxianError::Rgb`: Used for invalid IDs, configuration, persistence, native
  resolver, and RGB node errors.
- In Shadow mode, failures may fall back to simulation without affecting the
  main execution path.
- In Active mode, failures are returned and unknown contracts resolve to false
  or no result; they are never treated as positive proof.

## Remaining Phase 2 limitations
- The pinned `rgb-std` API deliberately leaves the signature algorithm to the
  application. The Gateway now provides the exact opt-in BIP340 profile above,
  but `RejectIssuerSignatures` remains the runtime/default policy. Arbitrary
  external issuers remain rejected unless an approved caller loads a valid
  policy that pins their exact identity and public key. No auto-detection or
  accept-all compatibility path exists.
- Deterministic unit coverage exercises malformed envelopes, contract-ID
  mismatch, staged cleanup after a semantically invalid unknown-contract
  import, fresh-stash reload after cleanup, the pinned unknown-contract
  resolver boundary, subprocess and same-process ownership contention/release,
  preservation of the losing resolver's root mode, independent stash roots,
  unsafe owner-lock paths, recovery-before-lock rejection, Unix permission
  hardening, registry replay/overwrite, unknown auth tokens, invalid signature
  policy, corrupted persistence, and the existing-contract
  filesystem state machine (including interruption replay, post-commit cleanup
  faults, and recovery path-identity rejection). The
  generated/replayed consignment fixture proves those filesystem and pinned API
  boundaries; it is not a real state-changing signed RGB transition or a
  Bitcoin/RGB regtest harness. A complete independently reproducible,
  state-changing signed transition/regtest fixture and controlled production
  wiring for the issuer policy remain required before treating Active
  consignment import as a production rollout milestone.
- The JSON cache remains for descriptive lookup compatibility and must not be
  interpreted as consensus evidence.
