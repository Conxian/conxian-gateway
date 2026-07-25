use conxian_core::{ConxianError, ConxianResult};

#[cfg(feature = "rgb-native")]
use std::collections::{BTreeMap, HashMap};
#[cfg(feature = "rgb-native")]
use std::fs::{self, File, OpenOptions};
#[cfg(feature = "rgb-native")]
use std::io::Write;
#[cfg(feature = "rgb-native")]
use std::net::IpAddr;
#[cfg(feature = "rgb-native")]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[cfg(feature = "rgb-native")]
use std::path::{Path, PathBuf};
#[cfg(feature = "rgb-native")]
use std::process;
#[cfg(feature = "rgb-native")]
use std::str::FromStr;
#[cfg(feature = "rgb-native")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "rgb-native")]
use std::sync::{Mutex, RwLock};

#[cfg(all(feature = "rgb-native", unix))]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[cfg(feature = "rgb-native")]
use binfile::BinFile;
#[cfg(feature = "rgb-native")]
use commit_verify::StrictHash;
#[cfg(feature = "rgb-native")]
use rgb_persist_fs::StockpileDir;
#[cfg(feature = "rgb-native")]
use rgbcore::RgbSealDef;
#[cfg(feature = "rgb-native")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "rgb-native")]
use strict_encoding::{StreamReader, StrictDecode, StrictEncode, StrictReader, StrictWriter};

#[cfg(feature = "rgb-native")]
const IMPORT_STAGING_PREFIX: &str = ".rgb-import-";
#[cfg(feature = "rgb-native")]
const UPDATE_TRANSACTION_PREFIX: &str = ".rgb-update-";
#[cfg(feature = "rgb-native")]
const UPDATE_JOURNAL_FILE: &str = "journal.json";
#[cfg(feature = "rgb-native")]
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// The result of resolving a Bitcoin outpoint through Esplora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtxoStatus {
    Unspent,
    Spent,
    NotFound,
}

/// Persisted contract metadata stored alongside the RGB stash.
#[cfg(feature = "rgb-native")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractMeta {
    pub contract_id: String,
    pub ticker: Option<String>,
    pub name: Option<String>,
    pub supply: Option<u64>,
    pub precision: Option<u8>,
    pub last_transition: Option<String>,
}

/// Application-owned issuer signature policy for RGB articles.
///
/// `rgb-std` intentionally abstracts the signing algorithm. The gateway keeps
/// that boundary explicit: callers provide a validator, and the validator is
/// given only the article commitment, issuer identity, and signature bytes.
/// The identity is used for validation only and is never persisted by this
/// resolver.
#[cfg(feature = "rgb-native")]
pub trait IssuerSignatureValidator {
    fn validate(&self, articles_id: &[u8], issuer: &str, signature: &[u8]) -> Result<(), String>;
}

#[cfg(feature = "rgb-native")]
impl<F> IssuerSignatureValidator for F
where
    F: Fn(&[u8], &str, &[u8]) -> Result<(), String>,
{
    fn validate(&self, articles_id: &[u8], issuer: &str, signature: &[u8]) -> Result<(), String> {
        self(articles_id, issuer, signature)
    }
}

/// A fail-closed signature policy for callers that have not wired a concrete
/// issuer signature scheme yet.
#[cfg(feature = "rgb-native")]
#[derive(Debug, Default, Clone, Copy)]
pub struct RejectIssuerSignatures;

#[cfg(feature = "rgb-native")]
impl IssuerSignatureValidator for RejectIssuerSignatures {
    fn validate(
        &self,
        _articles_id: &[u8],
        _issuer: &str,
        _signature: &[u8],
    ) -> Result<(), String> {
        Err("no issuer signature verifier is configured".to_string())
    }
}

#[cfg(feature = "rgb-native")]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SealRegistryRecord {
    auth_token: String,
    seal_hex: String,
}

#[cfg(feature = "rgb-native")]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum UpdatePhase {
    Prepared,
    BackedUp,
    Promoted,
}

#[cfg(feature = "rgb-native")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UpdateJournal {
    version: u8,
    contract_id: String,
    contract_dir: String,
    phase: UpdatePhase,
}

/// StashResolver owns the RGB filesystem stockpile, a wallet-owned auth-token
/// registry, and a non-consensus metadata cache.
///
/// The stockpile is the only source used for native consensus verification.
/// The JSON cache is retained for backwards-compatible descriptive lookup, but
/// it can never make a contract or transition consensus-valid.
///
/// The stash directory is a local-filesystem trust boundary. Its contract
/// files, metadata, and wallet-correlated seal registry are trusted only after
/// strict decoding and consistency checks; they are not encrypted by this
/// component. On Unix, this resolver creates the directory with owner-only
/// permissions and persists the registry with owner read/write permissions.
#[cfg(feature = "rgb-native")]
pub struct StashResolver {
    cache: RwLock<HashMap<String, ContractMeta>>,
    metadata_path: PathBuf,
    stockpile_dir: PathBuf,
    stockpile: Mutex<StockpileDir<bp::seals::TxoSeal>>,
    seal_registry: RwLock<HashMap<rgb::AuthToken, bp::seals::WTxoSeal>>,
    seal_registry_path: PathBuf,
    testnet: bool,
    #[allow(dead_code)]
    esplora_url: String,
    esplora_client: esplora::BlockingClient,
}

#[cfg(feature = "rgb-native")]
impl StashResolver {
    /// Creates a mainnet stash resolver and loads existing persistence.
    pub fn new(db_path: impl AsRef<Path>, esplora_url: &str) -> ConxianResult<Self> {
        Self::new_with_network(db_path, esplora_url, false)
    }

    /// Creates a stash resolver for the selected Bitcoin network.
    ///
    /// `db_path` is a directory owned by the gateway. It contains the RGB
    /// `StockpileDir`, descriptive metadata, and the wallet seal registry.
    /// Missing files are initialized, while malformed existing files or a
    /// malformed stockpile fail startup rather than being silently ignored.
    pub fn new_with_network(
        db_path: impl AsRef<Path>,
        esplora_url: &str,
        testnet: bool,
    ) -> ConxianResult<Self> {
        let stockpile_dir = db_path.as_ref().to_path_buf();
        if stockpile_dir.as_os_str().is_empty() {
            return Err(ConxianError::Rgb(
                "RGB stash path must not be empty".to_string(),
            ));
        }

        validate_endpoint(esplora_url)?;
        fs::create_dir_all(&stockpile_dir).map_err(|_| {
            ConxianError::Rgb("failed to create RGB stockpile directory".to_string())
        })?;
        restrict_directory(&stockpile_dir).map_err(|_| {
            ConxianError::Rgb("failed to restrict RGB stockpile directory permissions".to_string())
        })?;
        recover_update_transactions(&stockpile_dir, testnet)?;

        let metadata_path = stockpile_dir.join("contract-metadata.json");
        let seal_registry_path = stockpile_dir.join("seal-registry.json");
        if seal_registry_path.exists() {
            restrict_file(&seal_registry_path).map_err(|_| {
                ConxianError::Rgb("failed to restrict RGB seal registry permissions".to_string())
            })?;
        }
        let cache = load_cache(&metadata_path)?;
        let seal_registry = load_seal_registry(&seal_registry_path)?;
        let stockpile = load_stockpile(&stockpile_dir, testnet)?;
        let esplora_client = esplora::Builder::new(esplora_url)
            .timeout(5)
            .max_retries(0)
            .build_blocking()
            .map_err(|_| ConxianError::Rgb("failed to initialize Esplora client".to_string()))?;

        Ok(Self {
            cache: RwLock::new(cache),
            metadata_path,
            stockpile_dir,
            stockpile: Mutex::new(stockpile),
            seal_registry: RwLock::new(seal_registry),
            seal_registry_path,
            testnet,
            esplora_url: esplora_url.to_string(),
            esplora_client,
        })
    }

    /// Looks up a canonical `contract:` Baid64 RGB contract ID.
    pub fn lookup_contract(&self, contract_id: &str) -> ConxianResult<Option<ContractMeta>> {
        let key = canonical_contract_id(contract_id)?;
        let cache = self
            .cache
            .read()
            .map_err(|_| ConxianError::Rgb("RGB stash lock is poisoned".to_string()))?;
        Ok(cache.get(&key).cloned())
    }

    /// Stores metadata using an atomic temporary-file replacement.
    pub fn store_contract(&self, mut meta: ContractMeta) -> ConxianResult<()> {
        let key = canonical_contract_id(&meta.contract_id)?;
        meta.contract_id = key.clone();

        let mut cache = self
            .cache
            .write()
            .map_err(|_| ConxianError::Rgb("RGB stash lock is poisoned".to_string()))?;
        let previous = cache.insert(key.clone(), meta);
        let snapshot = cache.clone();

        if let Err(error) = self.persist_snapshot(&snapshot) {
            match previous {
                Some(previous) => {
                    cache.insert(key, previous);
                }
                None => {
                    cache.remove(&key);
                }
            }
            return Err(error);
        }

        Ok(())
    }

    /// Performs the stockpile contract-presence check for a transition ID.
    ///
    /// This is deliberately not a full `ContractVerify` implementation: a
    /// valid ID that is absent from the local stash returns `false` rather than
    /// being treated as consensus-valid.
    pub fn verify_transition(&self, transition_id: &str) -> ConxianResult<bool> {
        let key = canonical_contract_id(transition_id)?;
        let contract_id = rgb::ContractId::from_str(&key)
            .map_err(|_| ConxianError::Rgb("invalid RGB contract ID".to_string()))?;
        let stockpile = self
            .stockpile
            .lock()
            .map_err(|_| ConxianError::Rgb("RGB stockpile lock is poisoned".to_string()))?;
        Ok(rgb::Stockpile::has_contract(&*stockpile, contract_id))
    }

    /// Registers a wallet-owned auth token to seal definition binding.
    ///
    /// Re-registering the same token with the identical seal is idempotent.
    /// Re-registering it with a different seal is rejected and does not mutate
    /// the registry. The token must equal the seal's committed auth token.
    pub fn register_auth_token(
        &self,
        auth_token: &str,
        seal: bp::seals::WTxoSeal,
    ) -> ConxianResult<()> {
        let token = rgb::AuthToken::from_str(auth_token)
            .map_err(|_| ConxianError::Rgb("invalid RGB auth token".to_string()))?;
        let mut registry = self
            .seal_registry
            .write()
            .map_err(|_| ConxianError::Rgb("RGB seal registry lock is poisoned".to_string()))?;
        if let Some(existing) = registry.get(&token) {
            if existing == &seal {
                return Ok(());
            }
            return Err(ConxianError::Rgb(
                "auth token replay or overwrite rejected".to_string(),
            ));
        }
        if seal.auth_token() != token {
            return Err(ConxianError::Rgb(
                "auth token does not match the seal definition".to_string(),
            ));
        }

        registry.insert(token, seal);
        if let Err(error) = persist_seal_registry(&self.seal_registry_path, &registry) {
            registry.remove(&token);
            return Err(error);
        }
        Ok(())
    }

    /// Resolves a registered wallet seal definition without consulting the
    /// metadata cache.
    pub fn resolve_auth_token(
        &self,
        auth_token: &str,
    ) -> ConxianResult<Option<bp::seals::WTxoSeal>> {
        let token = rgb::AuthToken::from_str(auth_token)
            .map_err(|_| ConxianError::Rgb("invalid RGB auth token".to_string()))?;
        let registry = self
            .seal_registry
            .read()
            .map_err(|_| ConxianError::Rgb("RGB seal registry lock is poisoned".to_string()))?;
        Ok(registry.get(&token).copied())
    }

    /// Imports and fully verifies a consignment using `rgb-std` and the
    /// filesystem stockpile. The metadata cache is not consulted.
    ///
    /// The pinned RGB API accepts an optional issuer signature, so this method
    /// performs a preflight decode and rejects unsigned consignments before
    /// invoking the consensus importer. The supplied validator must implement
    /// the actual issuer signature scheme; the gateway does not invent one.
    ///
    /// Unknown-contract imports are evaluated in a private staging directory
    /// and promoted only after the pinned RGB verifier returns success. Existing
    /// contracts are copied into isolated same-filesystem state, verified there,
    /// and promoted through a durable journal/backup transaction. Startup and
    /// failed-import recovery deterministically retain either the prior verified
    /// contract or the fully verified replacement.
    ///
    /// These application-owned boundaries are necessary because
    /// `rgb-persist-fs` creates and mutates persistence before
    /// `evaluate_commit` finishes and does not provide a filesystem rollback
    /// transaction.
    /// The pinned `allow_unknown = true` branch also does not invoke its
    /// `seal_resolver`; wallet-owned seal/Esplora validation therefore begins
    /// only on paths for contracts already known to the stockpile.
    pub fn import_consignment<V: IssuerSignatureValidator>(
        &self,
        path: impl AsRef<Path>,
        expected_contract_id: &str,
        signature_validator: &V,
    ) -> ConxianResult<()> {
        let expected = canonical_contract_id(expected_contract_id)?;
        let contract_id = preflight_consignment(path.as_ref(), &expected, signature_validator)?;

        let mut stockpile = self
            .stockpile
            .lock()
            .map_err(|_| ConxianError::Rgb("RGB stockpile lock is poisoned".to_string()))?;

        if rgb::Stockpile::has_contract(&*stockpile, contract_id) {
            let transaction =
                create_update_transaction(&self.stockpile_dir, contract_id, self.testnet)?;
            let staged_stockpile = load_stockpile(&transaction.staged_dir, self.testnet)?;
            let import_result = catch_unwind(AssertUnwindSafe(|| {
                consume_consignment(self, staged_stockpile, path.as_ref(), signature_validator)
            }))
            .map_err(|_| {
                ConxianError::Rgb(
                    "RGB consignment importer panicked during transactional verification"
                        .to_string(),
                )
            })
            .and_then(|result| result);

            if let Err(error) = import_result {
                return recover_failed_update(
                    error,
                    &transaction.transaction_dir,
                    &self.stockpile_dir,
                    self.testnet,
                    &mut stockpile,
                );
            }

            if let Err(error) =
                validate_transaction_candidate(&transaction, contract_id, self.testnet)
            {
                return recover_failed_update(
                    error,
                    &transaction.transaction_dir,
                    &self.stockpile_dir,
                    self.testnet,
                    &mut stockpile,
                );
            }
            if let Err(error) = promote_update_transaction(&transaction, contract_id, self.testnet)
            {
                return recover_failed_update(
                    error,
                    &transaction.transaction_dir,
                    &self.stockpile_dir,
                    self.testnet,
                    &mut stockpile,
                );
            }

            *stockpile = load_stockpile(&self.stockpile_dir, self.testnet)?;
            return Ok(());
        }

        let staging_dir = create_import_staging_dir(&self.stockpile_dir)?;
        let import_result = catch_unwind(AssertUnwindSafe(|| {
            let staged_stockpile = load_stockpile(&staging_dir, self.testnet)?;
            consume_consignment(self, staged_stockpile, path.as_ref(), signature_validator)
        }))
        .map_err(|_| {
            ConxianError::Rgb(
                "RGB consignment importer panicked during staged verification".to_string(),
            )
        })
        .and_then(|result| result);

        if let Err(error) = import_result {
            return fail_import_with_cleanup(error, &staging_dir);
        }

        let staged_contract = match find_staged_contract(&staging_dir, contract_id) {
            Ok(path) => path,
            Err(error) => return fail_import_with_cleanup(error, &staging_dir),
        };
        if let Err(error) = sync_directory_tree(&staged_contract) {
            return fail_import_with_cleanup(
                ConxianError::Rgb(format!(
                    "failed to sync staged RGB contract before promotion: {error}"
                )),
                &staging_dir,
            );
        }
        if let Err(error) = sync_parent_directory(&staging_dir) {
            return fail_import_with_cleanup(
                ConxianError::Rgb(format!(
                    "failed to sync staged RGB import directory before promotion: {error}"
                )),
                &staging_dir,
            );
        }

        let contract_name = match staged_contract.file_name() {
            Some(name) => name,
            None => {
                return fail_import_with_cleanup(
                    ConxianError::Rgb("staged RGB contract has no directory name".to_string()),
                    &staging_dir,
                )
            }
        };
        let live_contract = self.stockpile_dir.join(contract_name);
        if path_exists(&live_contract) {
            return fail_import_with_cleanup(
                ConxianError::Rgb(
                    "RGB contract promotion refused because the destination already exists"
                        .to_string(),
                ),
                &staging_dir,
            );
        }

        if let Err(error) = fs::rename(&staged_contract, &live_contract) {
            return fail_import_with_cleanup(
                ConxianError::Rgb(format!("failed to promote staged RGB contract: {error}")),
                &staging_dir,
            );
        }
        if let Err(error) = sync_parent_directory(&self.stockpile_dir) {
            return rollback_promoted_import(
                ConxianError::Rgb(format!(
                    "RGB contract promotion directory sync failed: {error}"
                )),
                &live_contract,
                &staging_dir,
                &self.stockpile_dir,
            );
        }
        if let Err(error) = fs::remove_dir(&staging_dir) {
            return rollback_promoted_import(
                ConxianError::Rgb(format!(
                    "RGB contract staging directory cleanup failed: {error}"
                )),
                &live_contract,
                &staging_dir,
                &self.stockpile_dir,
            );
        }

        *stockpile = load_stockpile(&self.stockpile_dir, self.testnet)?;
        Ok(())
    }

    /// Exports a consignment from a consensus-verified stockpile contract.
    ///
    /// Terminal values are RGB auth tokens, not user identifiers or PII.
    pub fn export_consignment(
        &self,
        contract_id: &str,
        terminals: &[String],
        output_path: impl AsRef<Path>,
    ) -> ConxianResult<()> {
        let canonical = canonical_contract_id(contract_id)?;
        let id = rgb::ContractId::from_str(&canonical)
            .map_err(|_| ConxianError::Rgb("invalid RGB contract ID".to_string()))?;
        let terminal_tokens = terminals
            .iter()
            .map(|token| {
                rgb::AuthToken::from_str(token)
                    .map_err(|_| ConxianError::Rgb("invalid RGB terminal auth token".to_string()))
            })
            .collect::<ConxianResult<Vec<_>>>()?;

        let stockpile = self
            .stockpile
            .lock()
            .map_err(|_| ConxianError::Rgb("RGB stockpile lock is poisoned".to_string()))?;
        let contracts: rgb::Contracts<StockpileDir<bp::seals::TxoSeal>> =
            rgb::Contracts::load(stockpile.clone());
        contracts
            .consign_to_file(output_path, id, terminal_tokens.iter())
            .map_err(|_| ConxianError::Rgb("RGB consignment export failed".to_string()))
    }

    /// Queries Esplora for an outpoint without collapsing transport errors into
    /// an unspent/spent result.
    pub fn check_utxo(&self, txid: &str, vout: u32) -> ConxianResult<UtxoStatus> {
        let txid = bp::Txid::from_str(txid)
            .map_err(|_| ConxianError::Rgb("invalid Bitcoin transaction ID".to_string()))?;
        let status = self
            .esplora_client
            .output_status(&txid, u64::from(vout))
            .map_err(|_| ConxianError::Rgb("Esplora UTXO query failed".to_string()))?;

        Ok(match status {
            Some(status) if status.spent => UtxoStatus::Spent,
            Some(_) => UtxoStatus::Unspent,
            None => UtxoStatus::NotFound,
        })
    }

    /// Runs the blocking Esplora query off the Tokio worker threads.
    pub async fn check_utxo_async(&self, txid: &str, vout: u32) -> ConxianResult<UtxoStatus> {
        let client = self.esplora_client.clone();
        let txid = txid.to_string();
        tokio::task::spawn_blocking(move || {
            let txid = bp::Txid::from_str(&txid)
                .map_err(|_| ConxianError::Rgb("invalid Bitcoin transaction ID".to_string()))?;
            let status = client
                .output_status(&txid, u64::from(vout))
                .map_err(|_| ConxianError::Rgb("Esplora UTXO query failed".to_string()))?;
            Ok(match status {
                Some(status) if status.spent => UtxoStatus::Spent,
                Some(_) => UtxoStatus::Unspent,
                None => UtxoStatus::NotFound,
            })
        })
        .await
        .map_err(|_| ConxianError::Rgb("Esplora worker failed".to_string()))?
    }

    fn persist_snapshot(&self, snapshot: &HashMap<String, ContractMeta>) -> ConxianResult<()> {
        let json = serde_json::to_vec_pretty(snapshot)
            .map_err(|_| ConxianError::Rgb("failed to serialize RGB stash metadata".to_string()))?;
        persist_atomic_file(&self.metadata_path, &json, "RGB stash metadata")
    }
}

#[cfg(feature = "rgb-native")]
fn consume_consignment<V: IssuerSignatureValidator>(
    resolver: &StashResolver,
    persistence: StockpileDir<bp::seals::TxoSeal>,
    path: &Path,
    signature_validator: &V,
) -> ConxianResult<()> {
    let mut contracts: rgb::Contracts<StockpileDir<bp::seals::TxoSeal>> =
        rgb::Contracts::load(persistence);
    let registry = &resolver.seal_registry;
    contracts
        .consume_from_file(
            true,
            path,
            |operation| {
                resolve_registered_seals(registry, operation, |seal| {
                    let Some(source) = seal.to_src() else {
                        // Witness-relative seals are checked by RGB's consensus
                        // verifier against the consignment witness.
                        return true;
                    };
                    matches!(
                        resolver.check_utxo(
                            &source.primary.txid.to_string(),
                            source.primary.vout_u32(),
                        ),
                        Ok(UtxoStatus::Unspent)
                    )
                })
            },
            |hash, issuer, signature| {
                signature_validator.validate(
                    &hash.to_byte_array(),
                    &issuer.to_string(),
                    signature.as_slice(),
                )
            },
        )
        .map_err(|_| ConxianError::Rgb("RGB consignment consensus verification failed".to_string()))
}

#[cfg(feature = "rgb-native")]
#[derive(Debug)]
struct UpdateTransaction {
    transaction_dir: PathBuf,
    staged_dir: PathBuf,
    backup_dir: PathBuf,
    live_contract: PathBuf,
    staged_contract: PathBuf,
    backup_contract: PathBuf,
    journal: UpdateJournal,
}

#[cfg(feature = "rgb-native")]
fn create_update_transaction(
    stockpile_dir: &Path,
    contract_id: rgb::ContractId,
    testnet: bool,
) -> ConxianResult<UpdateTransaction> {
    validate_stockpile_contract(stockpile_dir, contract_id, testnet)?;
    let live_contract = find_contract_directory(stockpile_dir, contract_id)?;
    let contract_dir = live_contract
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ConxianError::Rgb("RGB contract directory name is invalid".to_string()))?
        .to_string();

    let transaction_dir = create_update_transaction_dir(stockpile_dir, contract_id)?;
    let setup = (|| -> ConxianResult<UpdateTransaction> {
        let staged_dir = transaction_dir.join("staged");
        let backup_dir = transaction_dir.join("backup");
        fs::create_dir(&staged_dir).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to create RGB update staging directory: {error}"
            ))
        })?;
        fs::create_dir(&backup_dir).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to create RGB update backup directory: {error}"
            ))
        })?;
        restrict_directory(&staged_dir).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to restrict RGB update staging directory: {error}"
            ))
        })?;
        restrict_directory(&backup_dir).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to restrict RGB update backup directory: {error}"
            ))
        })?;

        let journal = UpdateJournal {
            version: 1,
            contract_id: contract_id.to_string(),
            contract_dir,
            phase: UpdatePhase::Prepared,
        };
        persist_update_journal(&transaction_dir, &journal)?;
        sync_parent_directory(&transaction_dir).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to sync RGB update transaction directory: {error}"
            ))
        })?;
        sync_parent_directory(stockpile_dir).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to sync RGB stockpile transaction entry: {error}"
            ))
        })?;

        let staged_contract = staged_dir.join(&journal.contract_dir);
        copy_directory_tree(&live_contract, &staged_contract).map_err(|error| {
            ConxianError::Rgb(format!("failed to stage RGB contract update: {error}"))
        })?;
        sync_directory_tree(&staged_contract).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to sync staged RGB contract update: {error}"
            ))
        })?;
        sync_parent_directory(&staged_dir).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to sync RGB update staging directory: {error}"
            ))
        })?;

        Ok(UpdateTransaction {
            backup_contract: backup_dir.join(&journal.contract_dir),
            transaction_dir: transaction_dir.clone(),
            staged_dir,
            backup_dir,
            live_contract,
            staged_contract,
            journal,
        })
    })();

    match setup {
        Ok(transaction) => Ok(transaction),
        Err(error) => fail_update_setup(error, &transaction_dir, stockpile_dir),
    }
}

#[cfg(feature = "rgb-native")]
fn create_update_transaction_dir(
    root: &Path,
    contract_id: rgb::ContractId,
) -> ConxianResult<PathBuf> {
    let path = root.join(format!(
        "{UPDATE_TRANSACTION_PREFIX}{}",
        hex::encode(contract_id.to_string())
    ));
    match fs::create_dir(&path) {
        Ok(()) => {
            if let Err(error) = restrict_directory(&path) {
                let _ = fs::remove_dir(&path);
                return Err(ConxianError::Rgb(format!(
                    "failed to restrict RGB update transaction directory: {error}"
                )));
            }
            Ok(path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(ConxianError::Rgb(
            "RGB contract update is already in progress or awaiting startup recovery".to_string(),
        )),
        Err(error) => Err(ConxianError::Rgb(format!(
            "failed to create RGB update transaction directory: {error}"
        ))),
    }
}

#[cfg(feature = "rgb-native")]
fn validate_transaction_candidate(
    transaction: &UpdateTransaction,
    contract_id: rgb::ContractId,
    testnet: bool,
) -> ConxianResult<()> {
    if !path_exists(&transaction.staged_contract) {
        return Err(ConxianError::Rgb(
            "verified RGB update candidate is missing".to_string(),
        ));
    }
    sync_directory_tree(&transaction.staged_contract).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to sync verified RGB update candidate: {error}"
        ))
    })?;
    sync_parent_directory(&transaction.staged_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to sync RGB update staging directory: {error}"
        ))
    })?;
    validate_stockpile_contract(&transaction.staged_dir, contract_id, testnet)
}

#[cfg(feature = "rgb-native")]
fn promote_update_transaction(
    transaction: &UpdateTransaction,
    contract_id: rgb::ContractId,
    testnet: bool,
) -> ConxianResult<()> {
    fs::rename(&transaction.live_contract, &transaction.backup_contract).map_err(|error| {
        ConxianError::Rgb(format!("failed to back up live RGB contract: {error}"))
    })?;
    sync_parent_directory(&transaction.backup_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to sync RGB contract backup directory: {error}"
        ))
    })?;
    let stockpile_dir = transaction.live_contract.parent().ok_or_else(|| {
        ConxianError::Rgb("RGB live contract has no stockpile directory".to_string())
    })?;
    sync_parent_directory(stockpile_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to sync backed-up RGB stockpile state: {error}"
        ))
    })?;
    let mut journal = transaction.journal.clone();
    journal.phase = UpdatePhase::BackedUp;
    persist_update_journal(&transaction.transaction_dir, &journal)?;

    fs::rename(&transaction.staged_contract, &transaction.live_contract).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to promote verified RGB contract update: {error}"
        ))
    })?;
    sync_parent_directory(&transaction.staged_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to sync promoted RGB staging source: {error}"
        ))
    })?;
    sync_parent_directory(stockpile_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to sync promoted RGB stockpile state: {error}"
        ))
    })?;
    journal.phase = UpdatePhase::Promoted;
    persist_update_journal(&transaction.transaction_dir, &journal)?;
    validate_stockpile_contract(stockpile_dir, contract_id, testnet)?;

    fs::remove_dir_all(&transaction.backup_contract).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to remove committed RGB contract backup: {error}"
        ))
    })?;
    sync_parent_directory(&transaction.backup_dir).map_err(|error| {
        ConxianError::Rgb(format!("failed to sync RGB backup cleanup: {error}"))
    })?;
    fs::remove_dir_all(&transaction.transaction_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to clean committed RGB update transaction: {error}"
        ))
    })?;
    sync_parent_directory(stockpile_dir).map_err(|error| {
        ConxianError::Rgb(format!("failed to sync RGB transaction cleanup: {error}"))
    })?;
    Ok(())
}

#[cfg(feature = "rgb-native")]
fn recover_failed_update(
    error: ConxianError,
    transaction_dir: &Path,
    stockpile_dir: &Path,
    testnet: bool,
    stockpile: &mut StockpileDir<bp::seals::TxoSeal>,
) -> ConxianResult<()> {
    let committed = load_update_journal(transaction_dir)
        .map(|journal| journal.phase == UpdatePhase::Promoted)
        .unwrap_or(false);
    match recover_update_transaction(transaction_dir, stockpile_dir, testnet) {
        Ok(()) => {
            *stockpile = load_stockpile(stockpile_dir, testnet)?;
            if committed {
                Err(ConxianError::Rgb(format!(
                    "RGB update committed, but post-commit cleanup required recovery: {error}"
                )))
            } else {
                Err(error)
            }
        }
        Err(recovery_error) => Err(ConxianError::Rgb(format!(
            "{error}; RGB update recovery failed closed: {recovery_error}"
        ))),
    }
}

#[cfg(feature = "rgb-native")]
fn recover_update_transactions(stockpile_dir: &Path, testnet: bool) -> ConxianResult<()> {
    let mut transactions = Vec::new();
    for entry in fs::read_dir(stockpile_dir)
        .map_err(|_| ConxianError::Rgb("failed to inspect RGB update transactions".to_string()))?
    {
        let entry = entry.map_err(|_| {
            ConxianError::Rgb("failed to inspect RGB update transactions".to_string())
        })?;
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
            && entry
                .file_name()
                .to_str()
                .map(|name| name.starts_with(UPDATE_TRANSACTION_PREFIX))
                .unwrap_or(false)
        {
            transactions.push(entry.path());
        }
    }
    transactions.sort();
    for transaction in transactions {
        recover_update_transaction(&transaction, stockpile_dir, testnet)?;
    }
    Ok(())
}

#[cfg(feature = "rgb-native")]
fn recover_update_transaction(
    transaction_dir: &Path,
    stockpile_dir: &Path,
    testnet: bool,
) -> ConxianResult<()> {
    let journal = load_update_journal(transaction_dir)?;
    let contract_id = rgb::ContractId::from_str(&journal.contract_id)
        .map_err(|_| ConxianError::Rgb("corrupt RGB update journal contract ID".to_string()))?;
    validate_contract_dir_name(&journal.contract_dir, contract_id)?;
    let live_contract = stockpile_dir.join(&journal.contract_dir);
    let staged_dir = transaction_dir.join("staged");
    let backup_dir = transaction_dir.join("backup");
    let staged_contract = staged_dir.join(&journal.contract_dir);
    let backup_contract = backup_dir.join(&journal.contract_dir);

    match journal.phase {
        UpdatePhase::Promoted if path_exists(&live_contract) => {
            validate_stockpile_contract(stockpile_dir, contract_id, testnet)?;
        }
        UpdatePhase::Promoted if path_exists(&backup_contract) => {
            restore_backup(
                &live_contract,
                &staged_contract,
                &backup_contract,
                &staged_dir,
                &backup_dir,
                stockpile_dir,
            )?;
            validate_stockpile_contract(stockpile_dir, contract_id, testnet)?;
        }
        UpdatePhase::Promoted => {
            return Err(ConxianError::Rgb(
                "RGB promoted update has neither live nor backup contract".to_string(),
            ));
        }
        UpdatePhase::Prepared | UpdatePhase::BackedUp if path_exists(&backup_contract) => {
            restore_backup(
                &live_contract,
                &staged_contract,
                &backup_contract,
                &staged_dir,
                &backup_dir,
                stockpile_dir,
            )?;
            validate_stockpile_contract(stockpile_dir, contract_id, testnet)?;
        }
        UpdatePhase::Prepared | UpdatePhase::BackedUp if path_exists(&live_contract) => {
            validate_stockpile_contract(stockpile_dir, contract_id, testnet)?;
        }
        UpdatePhase::Prepared | UpdatePhase::BackedUp => {
            return Err(ConxianError::Rgb(
                "RGB update recovery cannot prove a live or backed-up contract".to_string(),
            ));
        }
    }

    fs::remove_dir_all(transaction_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to clean recovered RGB update transaction: {error}"
        ))
    })?;
    sync_parent_directory(stockpile_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to sync recovered RGB update cleanup: {error}"
        ))
    })?;
    Ok(())
}

#[cfg(feature = "rgb-native")]
fn restore_backup(
    live_contract: &Path,
    staged_contract: &Path,
    backup_contract: &Path,
    staged_dir: &Path,
    backup_dir: &Path,
    stockpile_dir: &Path,
) -> ConxianResult<()> {
    if path_exists(live_contract) {
        if path_exists(staged_contract) {
            fs::remove_dir_all(staged_contract).map_err(|error| {
                ConxianError::Rgb(format!(
                    "failed to clear stale RGB update candidate: {error}"
                ))
            })?;
        }
        fs::rename(live_contract, staged_contract).map_err(|error| {
            ConxianError::Rgb(format!(
                "failed to preserve interrupted RGB promotion: {error}"
            ))
        })?;
        sync_parent_directory(staged_dir).map_err(|error| {
            ConxianError::Rgb(format!("failed to sync preserved RGB candidate: {error}"))
        })?;
        sync_parent_directory(stockpile_dir).map_err(|error| {
            ConxianError::Rgb(format!("failed to sync interrupted RGB promotion: {error}"))
        })?;
    }
    fs::rename(backup_contract, live_contract).map_err(|error| {
        ConxianError::Rgb(format!("failed to restore backed-up RGB contract: {error}"))
    })?;
    sync_parent_directory(backup_dir).map_err(|error| {
        ConxianError::Rgb(format!("failed to sync RGB backup restoration: {error}"))
    })?;
    sync_parent_directory(stockpile_dir).map_err(|error| {
        ConxianError::Rgb(format!(
            "failed to sync restored RGB stockpile state: {error}"
        ))
    })?;
    Ok(())
}

#[cfg(feature = "rgb-native")]
fn persist_update_journal(transaction_dir: &Path, journal: &UpdateJournal) -> ConxianResult<()> {
    let data = serde_json::to_vec_pretty(journal)
        .map_err(|_| ConxianError::Rgb("failed to serialize RGB update journal".to_string()))?;
    persist_atomic_file(
        &transaction_dir.join(UPDATE_JOURNAL_FILE),
        &data,
        "RGB update journal",
    )
}

#[cfg(feature = "rgb-native")]
fn load_update_journal(transaction_dir: &Path) -> ConxianResult<UpdateJournal> {
    let data = fs::read(transaction_dir.join(UPDATE_JOURNAL_FILE))
        .map_err(|_| ConxianError::Rgb("missing RGB update recovery journal".to_string()))?;
    let journal = serde_json::from_slice::<UpdateJournal>(&data)
        .map_err(|_| ConxianError::Rgb("corrupt RGB update recovery journal".to_string()))?;
    if journal.version != 1 {
        return Err(ConxianError::Rgb(
            "unsupported RGB update recovery journal version".to_string(),
        ));
    }
    Ok(journal)
}

#[cfg(feature = "rgb-native")]
fn validate_contract_dir_name(name: &str, contract_id: rgb::ContractId) -> ConxianResult<()> {
    let path = Path::new(name);
    if path.components().count() != 1
        || path.file_name().and_then(|value| value.to_str()) != Some(name)
        || path.extension().and_then(|value| value.to_str()) != Some("contract")
    {
        return Err(ConxianError::Rgb(
            "unsafe RGB update journal contract directory".to_string(),
        ));
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ConxianError::Rgb("invalid RGB update contract directory".to_string()))?;
    let (_, id) = stem
        .split_once('.')
        .ok_or_else(|| ConxianError::Rgb("invalid RGB update contract directory".to_string()))?;
    if rgb::ContractId::from_str(id).ok() != Some(contract_id) {
        return Err(ConxianError::Rgb(
            "RGB update journal directory does not match its contract ID".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "rgb-native")]
fn validate_stockpile_contract(
    root: &Path,
    contract_id: rgb::ContractId,
    testnet: bool,
) -> ConxianResult<()> {
    let stockpile = load_stockpile(root, testnet)?;
    if !rgb::Stockpile::has_contract(&stockpile, contract_id)
        || rgb::Stockpile::contract(&stockpile, contract_id).is_none()
    {
        return Err(ConxianError::Rgb(
            "RGB update state does not contain a valid expected contract".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "rgb-native")]
fn find_contract_directory(root: &Path, expected: rgb::ContractId) -> ConxianResult<PathBuf> {
    let mut found = None;
    for entry in fs::read_dir(root)
        .map_err(|_| ConxianError::Rgb("failed to inspect RGB stockpile contracts".to_string()))?
    {
        let entry = entry.map_err(|_| {
            ConxianError::Rgb("failed to inspect RGB stockpile contracts".to_string())
        })?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
            || entry.path().extension().and_then(|value| value.to_str()) != Some("contract")
        {
            continue;
        }
        let entry_path = entry.path();
        let stem = entry_path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| ConxianError::Rgb("invalid RGB contract directory name".to_string()))?;
        let (_, id) = stem
            .split_once('.')
            .ok_or_else(|| ConxianError::Rgb("invalid RGB contract directory name".to_string()))?;
        if rgb::ContractId::from_str(id).ok() == Some(expected)
            && found.replace(entry_path).is_some()
        {
            return Err(ConxianError::Rgb(
                "duplicate RGB contract persistence detected".to_string(),
            ));
        }
    }
    found.ok_or_else(|| ConxianError::Rgb("RGB live contract directory is missing".to_string()))
}

#[cfg(feature = "rgb-native")]
fn copy_directory_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir(destination)?;
    restrict_directory(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_directory_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
            restrict_file(&destination_path)?;
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "RGB contract persistence contains a non-file entry",
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "rgb-native")]
fn fail_update_setup(
    error: ConxianError,
    transaction_dir: &Path,
    stockpile_dir: &Path,
) -> ConxianResult<UpdateTransaction> {
    match fs::remove_dir_all(transaction_dir) {
        Ok(()) => match sync_parent_directory(stockpile_dir) {
            Ok(()) => Err(error),
            Err(sync_error) => Err(ConxianError::Rgb(format!(
                "{error}; RGB update setup cleanup sync failed: {sync_error}"
            ))),
        },
        Err(cleanup_error) => Err(ConxianError::Rgb(format!(
            "{error}; RGB update setup cleanup failed: {cleanup_error}"
        ))),
    }
}

#[cfg(feature = "rgb-native")]
fn create_import_staging_dir(root: &Path) -> ConxianResult<PathBuf> {
    for _ in 0..32 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("{IMPORT_STAGING_PREFIX}{}-{id}", process::id()));
        match fs::create_dir(&path) {
            Ok(()) => {
                if let Err(error) = restrict_directory(&path) {
                    let _ = fs::remove_dir(&path);
                    return Err(ConxianError::Rgb(format!(
                        "failed to restrict RGB import staging directory: {error}"
                    )));
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(ConxianError::Rgb(format!(
                    "failed to create RGB import staging directory: {error}"
                )))
            }
        }
    }

    Err(ConxianError::Rgb(
        "unable to allocate a unique RGB import staging directory".to_string(),
    ))
}

#[cfg(feature = "rgb-native")]
fn find_staged_contract(path: &Path, expected: rgb::ContractId) -> ConxianResult<PathBuf> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(path)
        .map_err(|_| ConxianError::Rgb("failed to inspect staged RGB contract".to_string()))?
    {
        let entry = entry
            .map_err(|_| ConxianError::Rgb("failed to inspect staged RGB contract".to_string()))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|_| ConxianError::Rgb("failed to inspect staged RGB contract".to_string()))?;
        if !file_type.is_dir()
            || entry_path.extension().and_then(|value| value.to_str()) != Some("contract")
        {
            continue;
        }
        let Some(stem) = entry_path.file_stem().and_then(|value| value.to_str()) else {
            return Err(ConxianError::Rgb(
                "staged RGB contract has an invalid name".to_string(),
            ));
        };
        let Some((_, contract_id)) = stem.split_once('.') else {
            return Err(ConxianError::Rgb(
                "staged RGB contract has an invalid name".to_string(),
            ));
        };
        let parsed = rgb::ContractId::from_str(contract_id).map_err(|_| {
            ConxianError::Rgb("staged RGB contract has an invalid contract ID".to_string())
        })?;
        if parsed != expected {
            return Err(ConxianError::Rgb(
                "staged RGB contract ID does not match the consignment".to_string(),
            ));
        }
        matches.push(entry_path);
    }

    match matches.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(ConxianError::Rgb(
            "successful RGB import did not create a contract directory".to_string(),
        )),
        _ => Err(ConxianError::Rgb(
            "successful RGB import created multiple contract directories".to_string(),
        )),
    }
}

#[cfg(feature = "rgb-native")]
fn fail_import_with_cleanup(error: ConxianError, staging_dir: &Path) -> ConxianResult<()> {
    match fs::remove_dir_all(staging_dir) {
        Ok(()) => Err(error),
        Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => Err(error),
        Err(cleanup_error) => Err(ConxianError::Rgb(format!(
            "{error}; cleanup of staged RGB import artifacts failed: {cleanup_error}"
        ))),
    }
}

#[cfg(feature = "rgb-native")]
fn rollback_promoted_import(
    error: ConxianError,
    live_contract: &Path,
    staging_dir: &Path,
    stockpile_dir: &Path,
) -> ConxianResult<()> {
    let mut cleanup_errors = Vec::new();
    if let Err(cleanup_error) = fs::remove_dir_all(live_contract) {
        if cleanup_error.kind() != std::io::ErrorKind::NotFound {
            cleanup_errors.push(format!("promoted contract cleanup failed: {cleanup_error}"));
        }
    }
    if let Err(cleanup_error) = fs::remove_dir_all(staging_dir) {
        if cleanup_error.kind() != std::io::ErrorKind::NotFound {
            cleanup_errors.push(format!("staging cleanup failed: {cleanup_error}"));
        }
    }
    if cleanup_errors.is_empty() {
        if let Err(sync_error) = sync_parent_directory(stockpile_dir) {
            cleanup_errors.push(format!(
                "post-cleanup stockpile directory sync failed: {sync_error}"
            ));
        }
    }

    if cleanup_errors.is_empty() {
        Err(error)
    } else {
        Err(ConxianError::Rgb(format!(
            "{error}; {}",
            cleanup_errors.join("; ")
        )))
    }
}

#[cfg(feature = "rgb-native")]
fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

#[cfg(feature = "rgb-native")]
fn persist_atomic_file(path: &Path, data: &[u8], label: &str) -> ConxianResult<()> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("rgb-persistence");
    let temp_path = path.with_file_name(format!(
        ".{file_name}.tmp-{}-{}",
        process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        restrict_file(&temp_path)?;
        fs::rename(&temp_path, path)?;
        sync_parent_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
        Ok(())
    })();

    match result {
        Ok(()) => Ok(()),
        Err(error) => match fs::remove_file(&temp_path) {
            Ok(()) => Err(ConxianError::Rgb(format!(
                "failed to persist {label}: {error}"
            ))),
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => Err(
                ConxianError::Rgb(format!("failed to persist {label}: {error}")),
            ),
            Err(cleanup_error) => Err(ConxianError::Rgb(format!(
                "failed to persist {label}: {error}; temporary cleanup failed: {cleanup_error}"
            ))),
        },
    }
}

#[cfg(feature = "rgb-native")]
fn sync_directory_tree(path: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            sync_directory_tree(&entry_path)?;
        } else if file_type.is_file() {
            File::open(&entry_path)?.sync_all()?;
        }
    }
    sync_parent_directory(path)
}

#[cfg(feature = "rgb-native")]
fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(feature = "rgb-native")]
fn restrict_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(feature = "rgb-native")]
fn restrict_file(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(feature = "rgb-native")]
fn load_stockpile(path: &Path, testnet: bool) -> ConxianResult<StockpileDir<bp::seals::TxoSeal>> {
    for entry in fs::read_dir(path)
        .map_err(|_| ConxianError::Rgb("corrupt RGB stockpile persistence".to_string()))?
    {
        let entry = entry
            .map_err(|_| ConxianError::Rgb("corrupt RGB stockpile persistence".to_string()))?;
        let entry_path = entry.path();
        let extension = entry_path.extension().and_then(|value| value.to_str());
        let stem = entry_path.file_stem().and_then(|value| value.to_str());
        match (extension, stem, entry.file_type().ok()) {
            (Some("contract"), Some(stem), Some(file_type)) if file_type.is_dir() => {
                let Some((_, contract_id)) = stem.split_once('.') else {
                    return Err(ConxianError::Rgb(
                        "corrupt RGB stockpile contract name".to_string(),
                    ));
                };
                if rgb::ContractId::from_str(contract_id).is_err() {
                    return Err(ConxianError::Rgb(
                        "corrupt RGB stockpile contract name".to_string(),
                    ));
                }
            }
            (Some("issuer"), Some(stem), Some(file_type)) if file_type.is_file() => {
                let Some((_, codex_id)) = stem.split_once('.') else {
                    return Err(ConxianError::Rgb(
                        "corrupt RGB stockpile issuer name".to_string(),
                    ));
                };
                if rgb::CodexId::from_str(codex_id).is_err() {
                    return Err(ConxianError::Rgb(
                        "corrupt RGB stockpile issuer name".to_string(),
                    ));
                }
            }
            _ => {}
        }
    }
    let stockpile = StockpileDir::load(path.to_path_buf(), rgb::Consensus::Bitcoin, testnet)
        .map_err(|_| ConxianError::Rgb("corrupt RGB stockpile persistence".to_string()))?;
    for contract_id in rgb::Stockpile::contract_ids(&stockpile).collect::<Vec<_>>() {
        if rgb::Stockpile::contract(&stockpile, contract_id).is_none() {
            return Err(ConxianError::Rgb(
                "corrupt RGB stockpile contract persistence".to_string(),
            ));
        }
    }
    Ok(stockpile)
}

#[cfg(feature = "rgb-native")]
fn resolve_registered_seals(
    registry: &RwLock<HashMap<rgb::AuthToken, bp::seals::WTxoSeal>>,
    operation: &rgb::Operation,
    mut is_usable: impl FnMut(&bp::seals::WTxoSeal) -> bool,
) -> BTreeMap<u16, bp::seals::WTxoSeal> {
    let Ok(registry) = registry.read() else {
        return BTreeMap::new();
    };
    operation
        .destructible_out
        .iter()
        .enumerate()
        .filter_map(|(position, cell)| {
            registry
                .get(&cell.auth)
                .copied()
                .filter(|seal| is_usable(seal))
                .map(|seal| (position as u16, seal))
        })
        .collect()
}

#[cfg(feature = "rgb-native")]
fn preflight_consignment<V: IssuerSignatureValidator>(
    path: &Path,
    expected_contract_id: &str,
    signature_validator: &V,
) -> ConxianResult<rgb::ContractId> {
    let file = BinFile::<{ rgb::CONSIGN_MAGIC_NUMBER }, { rgb::CONSIGN_VERSION }>::open(path)
        .map_err(|_| ConxianError::Rgb("malformed RGB consignment envelope".to_string()))?;
    let mut reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(file));
    let parsed_contract_id = rgb::parse_consignment(&mut reader)
        .map_err(|_| ConxianError::Rgb("malformed RGB consignment header".to_string()))?;
    let expected = rgb::ContractId::from_str(expected_contract_id)
        .map_err(|_| ConxianError::Rgb("invalid expected RGB contract ID".to_string()))?;
    if parsed_contract_id != expected {
        return Err(ConxianError::Rgb(
            "RGB consignment contract ID mismatch".to_string(),
        ));
    }

    let consignment = rgb::Consignment::<bp::seals::TxoSeal>::strict_decode(&mut reader)
        .map_err(|_| ConxianError::Rgb("malformed RGB consignment body".to_string()))?;
    let articles = consignment
        .articles(
            |hash: StrictHash, issuer: &rgb::Identity, signature: &rgb::SigBlob| {
                signature_validator.validate(
                    &hash.to_byte_array(),
                    &issuer.to_string(),
                    signature.as_slice(),
                )
            },
        )
        .map_err(|_| ConxianError::Rgb("invalid RGB issuer signature".to_string()))?;
    if !articles.is_signed() {
        return Err(ConxianError::Rgb(
            "unsigned RGB consignment rejected".to_string(),
        ));
    }
    if articles.contract_id() != expected {
        return Err(ConxianError::Rgb(
            "RGB consignment articles contract ID mismatch".to_string(),
        ));
    }
    Ok(expected)
}

#[cfg(feature = "rgb-native")]
fn load_seal_registry(path: &Path) -> ConxianResult<HashMap<rgb::AuthToken, bp::seals::WTxoSeal>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let data = fs::read_to_string(path)
        .map_err(|_| ConxianError::Rgb("failed to read RGB seal registry".to_string()))?;
    let records = serde_json::from_str::<Vec<SealRegistryRecord>>(&data)
        .map_err(|_| ConxianError::Rgb("corrupt RGB seal registry".to_string()))?;
    let mut registry = HashMap::with_capacity(records.len());
    for record in records {
        let token = rgb::AuthToken::from_str(&record.auth_token)
            .map_err(|_| ConxianError::Rgb("corrupt RGB seal registry auth token".to_string()))?;
        let bytes = hex::decode(record.seal_hex)
            .map_err(|_| ConxianError::Rgb("corrupt RGB seal registry seal".to_string()))?;
        let mut reader = StrictReader::in_memory::<4096>(bytes.clone());
        let seal = bp::seals::WTxoSeal::strict_decode(&mut reader)
            .map_err(|_| ConxianError::Rgb("corrupt RGB seal registry seal".to_string()))?;
        if reader.into_cursor().position() as usize != bytes.len() {
            return Err(ConxianError::Rgb(
                "corrupt RGB seal registry seal".to_string(),
            ));
        }
        if seal.auth_token() != token || registry.insert(token, seal).is_some() {
            return Err(ConxianError::Rgb(
                "invalid or duplicate RGB seal registry binding".to_string(),
            ));
        }
    }
    Ok(registry)
}

#[cfg(feature = "rgb-native")]
fn persist_seal_registry(
    path: &Path,
    registry: &HashMap<rgb::AuthToken, bp::seals::WTxoSeal>,
) -> ConxianResult<()> {
    let mut records = registry
        .iter()
        .map(|(token, seal)| {
            let writer = seal
                .strict_encode(StrictWriter::in_memory::<4096>())
                .map_err(|_| ConxianError::Rgb("failed to encode RGB seal".to_string()))?;
            let encoded = writer.unbox().unconfine();
            Ok(SealRegistryRecord {
                auth_token: token.to_string(),
                seal_hex: hex::encode(encoded),
            })
        })
        .collect::<ConxianResult<Vec<_>>>()?;
    records.sort_by(|left, right| left.auth_token.cmp(&right.auth_token));
    let json = serde_json::to_vec_pretty(&records)
        .map_err(|_| ConxianError::Rgb("failed to serialize RGB seal registry".to_string()))?;
    persist_atomic_file(path, &json, "RGB seal registry")
}

#[cfg(feature = "rgb-native")]
fn canonical_contract_id(input: &str) -> ConxianResult<String> {
    crate::bitcoin::rgb_native::normalize_contract_id(input)
}

#[cfg(feature = "rgb-native")]
fn load_cache(path: &Path) -> ConxianResult<HashMap<String, ContractMeta>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let data = fs::read_to_string(path)
        .map_err(|_| ConxianError::Rgb("failed to read RGB stash metadata".to_string()))?;
    let cache = serde_json::from_str::<HashMap<String, ContractMeta>>(&data)
        .map_err(|_| ConxianError::Rgb("corrupt RGB stash metadata".to_string()))?;

    for (key, meta) in &cache {
        let canonical = canonical_contract_id(key)?;
        if canonical != *key || meta.contract_id != *key {
            return Err(ConxianError::Rgb(
                "RGB stash metadata has a non-canonical contract ID".to_string(),
            ));
        }
    }

    Ok(cache)
}

#[cfg(feature = "rgb-native")]
fn validate_endpoint(raw: &str) -> ConxianResult<()> {
    let url = url::Url::parse(raw)
        .map_err(|_| ConxianError::Rgb("invalid RGB Esplora URL".to_string()))?;
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(ConxianError::Rgb(
            "RGB Esplora URL must use http or https".to_string(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ConxianError::Rgb(
            "RGB Esplora URL must not contain credentials".to_string(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ConxianError::Rgb("RGB Esplora URL must include a host".to_string()))?;
    if scheme == "http" && !is_local_host(host) {
        return Err(ConxianError::Rgb(
            "plain HTTP is only allowed for local RGB development".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "rgb-native")]
fn is_local_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

// ── Non-native fallback ────────────────────────────────────────────────

#[cfg(not(feature = "rgb-native"))]
pub struct StashResolver;

#[cfg(not(feature = "rgb-native"))]
impl StashResolver {
    pub fn new(_db_path: impl AsRef<std::path::Path>, _esplora_url: &str) -> ConxianResult<Self> {
        Err(ConxianError::Rgb(
            "rgb-native feature not enabled".to_string(),
        ))
    }

    pub fn lookup_contract(&self, _contract_id: &str) -> ConxianResult<Option<()>> {
        Err(ConxianError::Rgb(
            "rgb-native feature not enabled".to_string(),
        ))
    }

    pub fn verify_transition(&self, _transition_id: &str) -> ConxianResult<bool> {
        Err(ConxianError::Rgb(
            "rgb-native feature not enabled".to_string(),
        ))
    }

    pub fn check_utxo(&self, _txid: &str, _vout: u32) -> ConxianResult<UtxoStatus> {
        Err(ConxianError::Rgb(
            "rgb-native feature not enabled".to_string(),
        ))
    }

    pub async fn check_utxo_async(&self, _txid: &str, _vout: u32) -> ConxianResult<UtxoStatus> {
        Err(ConxianError::Rgb(
            "rgb-native feature not enabled".to_string(),
        ))
    }
}

#[cfg(all(test, feature = "rgb-native"))]
mod tests {
    use super::*;
    use commit_verify::{DigestExt, Sha256};
    use rgb_persist_fs::{PileFs, StockFs};
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use std::convert::Infallible;
    use std::sync::atomic::{AtomicU64, Ordering};
    use strict_encoding::{
        StreamWriter, StrictDecode, StrictDumb, StrictEncode, StrictReader, StrictWriter,
    };

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    const VALID_ID: &str = "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg";
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    fn temp_path(label: &str) -> PathBuf {
        let id = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("conxian-rgb-{label}-{id}"))
    }

    fn metadata() -> ContractMeta {
        ContractMeta {
            contract_id: VALID_ID.to_string(),
            ticker: Some("CONX".to_string()),
            name: Some("Conxian".to_string()),
            supply: Some(1_000_000),
            precision: Some(8),
            last_transition: None,
        }
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn parses_canonical_contract_id_and_rejects_legacy_rgb_ids() {
        assert_eq!(canonical_contract_id(VALID_ID).unwrap(), VALID_ID);
        assert_eq!(
            canonical_contract_id(
                "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg#fractal-fashion-capsule"
            )
            .unwrap(),
            VALID_ID
        );
        assert!(
            canonical_contract_id("rgb:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg").is_err()
        );
        assert!(
            canonical_contract_id("contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCg!")
                .is_err()
        );
        assert!(canonical_contract_id(
            "contractx:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg"
        )
        .is_err());
    }

    #[test]
    fn stores_and_reloads_metadata_atomically() {
        let path = temp_path("reload");
        cleanup(&path);
        let resolver = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        resolver.store_contract(metadata()).unwrap();
        assert_eq!(
            resolver.lookup_contract(VALID_ID).unwrap(),
            Some(metadata())
        );
        assert!(!path.join("contract-metadata.tmp").exists());

        let reloaded = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        assert_eq!(
            reloaded.lookup_contract(VALID_ID).unwrap(),
            Some(metadata())
        );
        cleanup(&path);
    }

    #[test]
    fn corrupt_metadata_is_an_error() {
        let path = temp_path("corrupt");
        cleanup(&path);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("contract-metadata.json"), b"not-json").unwrap();
        let result = StashResolver::new(&path, "https://blockstream.info/api");
        assert!(matches!(result, Err(ConxianError::Rgb(message)) if message.contains("corrupt")));
        cleanup(&path);
    }

    #[test]
    fn rejects_unsafe_esplora_urls() {
        let path = temp_path("url");
        cleanup(&path);
        assert!(StashResolver::new(&path, "https://user:pass@example.com/api").is_err());
        assert!(StashResolver::new(&path, "http://example.com/api").is_err());
        cleanup(&path);
    }

    #[test]
    fn invalid_ids_are_rejected_by_lookup_and_store() {
        let path = temp_path("invalid");
        cleanup(&path);
        let resolver = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        assert!(resolver.lookup_contract("rgb:not-a-contract").is_err());
        let mut invalid = metadata();
        invalid.contract_id = "rgb:not-a-contract".to_string();
        assert!(resolver.store_contract(invalid).is_err());
        cleanup(&path);
    }

    #[test]
    fn invalid_txids_are_rejected_without_a_network_request() {
        let path = temp_path("txid");
        cleanup(&path);
        let resolver = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        assert!(matches!(
            resolver.check_utxo("not-a-txid", 0),
            Err(ConxianError::Rgb(message)) if message.contains("transaction ID")
        ));
        cleanup(&path);
    }

    fn fixture_seal(nonce: u64) -> bp::seals::WTxoSeal {
        let txid = bp::Txid::coinbase();
        bp::seals::WTxoSeal::no_fallback(
            bp::Outpoint::new(txid, 12u32),
            commit_verify::Sha256::default(),
            nonce,
        )
    }

    #[test]
    fn auth_token_registry_is_idempotent_and_rejects_overwrite() {
        let path = temp_path("registry");
        cleanup(&path);
        let resolver = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        let seal = fixture_seal(1);
        let token = seal.auth_token().to_string();

        resolver.register_auth_token(&token, seal).unwrap();
        resolver.register_auth_token(&token, seal).unwrap();
        assert_eq!(resolver.resolve_auth_token(&token).unwrap(), Some(seal));

        #[cfg(unix)]
        {
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(path.join("seal-registry.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        let overwrite = fixture_seal(2);
        assert!(matches!(
            resolver.register_auth_token(&token, overwrite),
            Err(ConxianError::Rgb(message)) if message.contains("overwrite")
        ));
        cleanup(&path);
    }

    #[test]
    fn auth_token_registry_rejects_mismatched_and_unknown_tokens() {
        let path = temp_path("registry-mismatch");
        cleanup(&path);
        let resolver = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        let seal = fixture_seal(3);
        let wrong_token = fixture_seal(4).auth_token().to_string();
        assert!(matches!(
            resolver.register_auth_token(&wrong_token, seal),
            Err(ConxianError::Rgb(message)) if message.contains("does not match")
        ));
        assert!(resolver.resolve_auth_token(&wrong_token).unwrap().is_none());
        cleanup(&path);
    }

    #[test]
    fn corrupted_seal_registry_fails_closed() {
        let path = temp_path("registry-corrupt");
        cleanup(&path);
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("seal-registry.json"),
            b"[{\"auth_token\":\"bad\"}]",
        )
        .unwrap();
        let result = StashResolver::new(&path, "https://blockstream.info/api");
        assert!(matches!(result, Err(ConxianError::Rgb(message)) if message.contains("corrupt")));
        cleanup(&path);
    }

    #[test]
    fn unknown_auth_tokens_resolve_to_no_seal_definition() {
        let path = temp_path("registry-unknown");
        cleanup(&path);
        let resolver = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        let unknown = fixture_seal(5).auth_token().to_string();
        assert!(resolver.resolve_auth_token(&unknown).unwrap().is_none());
        cleanup(&path);
    }

    #[test]
    fn registered_seal_is_rejected_when_bitcoin_resolver_marks_it_spent() {
        let seal = fixture_seal(6);
        let token = seal.auth_token();
        let registry = RwLock::new(HashMap::from([(token, seal)]));
        let mut operation = rgb::Operation::strict_dumb();
        operation
            .destructible_out
            .push(rgb::StateCell {
                data: rgb::StateValue::strict_dumb(),
                auth: token,
                lock: None,
            })
            .unwrap();

        let resolved = resolve_registered_seals(&registry, &operation, |_| false);
        assert!(resolved.is_empty());
    }

    fn write_consignment_header(path: &Path, contract_id: &str) {
        let file = BinFile::<{ rgb::CONSIGN_MAGIC_NUMBER }, { rgb::CONSIGN_VERSION }>::create(path)
            .unwrap();
        let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(file));
        let writer = 0u8.strict_encode(writer).unwrap();
        let contract_id = rgb::ContractId::from_str(contract_id).unwrap();
        contract_id.strict_encode(writer).unwrap();
    }

    fn write_consignment(root: &Path) -> (PathBuf, String) {
        let issuer_path = root.join("Test.issuer");
        fs::write(&issuer_path, include_bytes!("testdata/Test.issuer")).unwrap();
        let issuer =
            rgb::Issuer::load(&issuer_path, |_, _, _| -> Result<_, Infallible> { Ok(()) }).unwrap();

        let mut noise = Sha256::default();
        noise.input_raw(b"conxian-rgb-transactional-import");
        let mut params = rgb::CreateParams::new_bitcoin_testnet(issuer.codex_id(), "ConxianTest");
        params.push_owned_unlocked(
            "amount",
            rgb::Assignment::new_internal(bp::Outpoint::strict_dumb(), 100u64),
        );
        let contract_path = root.join("source.contract");
        fs::create_dir_all(&contract_path).unwrap();
        let contract = rgb::Contract::<StockFs, PileFs<bp::seals::TxoSeal>>::issue(
            issuer,
            params.transform(noise.clone()),
            |_| Ok(contract_path.clone()),
        )
        .unwrap();

        let consignment_path = root.join("fixture.rgb");
        let terminal = *contract.full_state().raw.auth.keys().next().unwrap();
        contract
            .consign_to_file(&consignment_path, [terminal])
            .unwrap();
        add_article_signature(&consignment_path);
        (consignment_path, contract.contract_id().to_string())
    }

    fn write_semantically_invalid_consignment(root: &Path) -> (PathBuf, String) {
        let result = write_consignment(root);
        corrupt_genesis_seal(&result.0);
        result
    }

    fn import_fixture(root: &Path) -> (PathBuf, String, PathBuf) {
        let fixture_root = root.join("fixture");
        fs::create_dir_all(&fixture_root).unwrap();
        let (consignment, contract_id) = write_consignment(&fixture_root);
        let stash_path = root.join("stash");
        let resolver =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        let validator = |_: &[u8], _: &str, _: &[u8]| Ok::<(), String>(());
        resolver
            .import_consignment(&consignment, &contract_id, &validator)
            .unwrap();
        drop(resolver);
        (consignment, contract_id, stash_path)
    }

    fn transaction_residue(root: &Path) -> Vec<PathBuf> {
        let mut residue = fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| {
                        name.starts_with(UPDATE_TRANSACTION_PREFIX)
                            || name.starts_with(IMPORT_STAGING_PREFIX)
                    })
                    .unwrap_or(false)
            })
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        residue.sort();
        residue
    }

    fn add_article_signature(path: &Path) {
        let mut bytes = fs::read(path).unwrap();
        let mut reader = StrictReader::in_memory::<{ usize::MAX }>(&bytes[10..]);
        rgb::parse_consignment(&mut reader).unwrap();
        u8::strict_decode(&mut reader).unwrap();
        rgb::Semantics::strict_decode(&mut reader).unwrap();
        let offset = 10 + reader.into_cursor().position() as usize;
        assert_eq!(bytes[offset], 0, "fixture must start unsigned");

        let signature = rgb::SigBlob::from_slice_checked([0xA5; 32]);
        let encoded = signature
            .strict_encode(StrictWriter::in_memory::<4096>())
            .unwrap()
            .unbox()
            .unconfine();
        bytes[offset] = 1;
        bytes.splice(offset + 1..offset + 1, encoded);
        fs::write(path, bytes).unwrap();
    }

    fn corrupt_genesis_seal(path: &Path) {
        let mut bytes = fs::read(path).unwrap();
        let mut reader = StrictReader::in_memory::<{ usize::MAX }>(&bytes[10..]);
        rgb::parse_consignment(&mut reader).unwrap();
        u8::strict_decode(&mut reader).unwrap();
        rgb::Semantics::strict_decode(&mut reader).unwrap();
        Option::<rgb::SigBlob>::strict_decode(&mut reader).unwrap();
        rgb::Issue::strict_decode(&mut reader).unwrap();
        let seal_count = u16::strict_decode(&mut reader).unwrap();
        assert_eq!(seal_count, 1, "fixture must have one genesis seal");
        u16::strict_decode(&mut reader).unwrap();
        bp::seals::WTxoSeal::strict_decode(&mut reader).unwrap();
        let end = 10 + reader.into_cursor().position() as usize;
        assert!(end > 10, "fixture must contain a genesis seal");
        bytes[end - 1] ^= 0x01;
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn malformed_and_mismatched_consignments_fail_closed_before_import() {
        let path = temp_path("consignment-boundary");
        cleanup(&path);
        let resolver = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        let malformed = path.join("malformed.rgb");
        fs::write(&malformed, b"not-an-rgb-consignment").unwrap();
        let validator = RejectIssuerSignatures;
        assert!(matches!(
            resolver.import_consignment(&malformed, VALID_ID, &validator),
            Err(ConxianError::Rgb(message)) if message.contains("malformed")
        ));

        let mismatched = path.join("mismatched.rgb");
        write_consignment_header(
            &mismatched,
            "contract:AAAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA",
        );
        assert!(matches!(
            resolver.import_consignment(&mismatched, VALID_ID, &validator),
            Err(ConxianError::Rgb(message)) if message.contains("mismatch")
        ));
        cleanup(&path);
    }

    #[test]
    fn semantically_invalid_unknown_import_cleans_orphans_and_reloads() {
        let path = temp_path("transactional-import");
        cleanup(&path);
        let fixture_root = path.join("fixture");
        fs::create_dir_all(&fixture_root).unwrap();
        let (consignment, contract_id) = write_semantically_invalid_consignment(&fixture_root);
        let stash_path = path.join("stash");
        let resolver =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        let validator = |_: &[u8], _: &str, _: &[u8]| Ok::<(), String>(());

        let result = resolver.import_consignment(&consignment, &contract_id, &validator);
        assert!(matches!(
            result,
            Err(ConxianError::Rgb(message))
                if message.contains("consignment consensus verification")
                    || message.contains("importer panicked")
        ));

        let contract_dirs = fs::read_dir(&stash_path)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                    && entry.path().extension().and_then(|value| value.to_str()) == Some("contract")
            })
            .count();
        assert_eq!(contract_dirs, 0, "failed import left a contract directory");

        let staging_dirs = fs::read_dir(&stash_path)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                    && entry
                        .file_name()
                        .to_str()
                        .map(|name| name.starts_with(IMPORT_STAGING_PREFIX))
                        .unwrap_or(false)
            })
            .count();
        assert_eq!(staging_dirs, 0, "failed import left a staging directory");

        let reloaded =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        assert!(!reloaded.verify_transition(&contract_id).unwrap());
        cleanup(&path);
    }

    #[test]
    fn successful_existing_contract_update_persists_across_restart() {
        let path = temp_path("transactional-success");
        cleanup(&path);
        let (consignment, contract_id, stash_path) = import_fixture(&path);
        let resolver =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        let validator = |_: &[u8], _: &str, _: &[u8]| Ok::<(), String>(());
        assert!(resolver.verify_transition(&contract_id).unwrap());

        let second_import = resolver.import_consignment(&consignment, &contract_id, &validator);
        second_import.unwrap();
        assert!(resolver.verify_transition(&contract_id).unwrap());
        assert!(transaction_residue(&stash_path).is_empty());

        let reloaded =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        assert!(reloaded.verify_transition(&contract_id).unwrap());
        cleanup(&path);
    }

    #[test]
    fn invalid_existing_contract_update_preserves_prior_state() {
        let path = temp_path("transactional-invalid-update");
        cleanup(&path);
        let (consignment, contract_id, stash_path) = import_fixture(&path);
        let invalid_consignment = path.join("invalid-update.rgb");
        fs::copy(&consignment, &invalid_consignment).unwrap();
        corrupt_genesis_seal(&invalid_consignment);
        let resolver =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        let validator = |_: &[u8], _: &str, _: &[u8]| Ok::<(), String>(());

        assert!(resolver
            .import_consignment(&invalid_consignment, &contract_id, &validator)
            .is_err());
        assert!(resolver.verify_transition(&contract_id).unwrap());
        assert!(transaction_residue(&stash_path).is_empty());

        let reloaded =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        assert!(reloaded.verify_transition(&contract_id).unwrap());
        cleanup(&path);
    }

    #[test]
    fn signature_rejection_for_existing_contract_preserves_prior_state() {
        let path = temp_path("transactional-signature-rejection");
        cleanup(&path);
        let (consignment, contract_id, stash_path) = import_fixture(&path);
        let resolver =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();

        let result =
            resolver.import_consignment(&consignment, &contract_id, &RejectIssuerSignatures);
        assert!(matches!(
            result,
            Err(ConxianError::Rgb(message)) if message.contains("issuer signature")
        ));
        assert!(resolver.verify_transition(&contract_id).unwrap());
        assert!(transaction_residue(&stash_path).is_empty());
        cleanup(&path);
    }

    #[test]
    fn verified_update_transaction_persists_and_leaves_unrelated_files_unchanged() {
        let path = temp_path("transactional-promotion");
        cleanup(&path);
        let (_, contract_id, stash_path) = import_fixture(&path);
        let contract_id = rgb::ContractId::from_str(&contract_id).unwrap();
        let issuer_path = stash_path.join("unrelated.issuer-note");
        let registry_path = stash_path.join("seal-registry.json");
        fs::write(&issuer_path, b"issuer-bytes-must-not-change").unwrap();
        fs::write(&registry_path, b"[]").unwrap();

        let transaction = create_update_transaction(&stash_path, contract_id, true).unwrap();
        fs::write(transaction.staged_contract.join("generation"), b"new").unwrap();
        validate_transaction_candidate(&transaction, contract_id, true).unwrap();
        promote_update_transaction(&transaction, contract_id, true).unwrap();

        assert_eq!(
            fs::read(
                find_contract_directory(&stash_path, contract_id)
                    .unwrap()
                    .join("generation")
            )
            .unwrap(),
            b"new"
        );
        assert_eq!(
            fs::read(&issuer_path).unwrap(),
            b"issuer-bytes-must-not-change"
        );
        assert_eq!(fs::read(&registry_path).unwrap(), b"[]");
        assert!(transaction_residue(&stash_path).is_empty());

        let reloaded =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        assert!(reloaded
            .verify_transition(&contract_id.to_string())
            .unwrap());
        assert_eq!(
            fs::read(
                find_contract_directory(&stash_path, contract_id)
                    .unwrap()
                    .join("generation")
            )
            .unwrap(),
            b"new"
        );
        cleanup(&path);
    }

    #[test]
    fn overlapping_same_contract_transaction_acquisition_fails_closed() {
        let path = temp_path("transactional-overlap");
        cleanup(&path);
        let (_, contract_id, stash_path) = import_fixture(&path);
        let contract_id = rgb::ContractId::from_str(&contract_id).unwrap();
        let transaction = create_update_transaction(&stash_path, contract_id, true).unwrap();

        assert!(matches!(
            create_update_transaction(&stash_path, contract_id, true),
            Err(ConxianError::Rgb(message)) if message.contains("already in progress")
        ));
        recover_update_transaction(&transaction.transaction_dir, &stash_path, true).unwrap();
        assert!(transaction_residue(&stash_path).is_empty());
        cleanup(&path);
    }

    #[test]
    fn startup_recovers_prepared_and_backed_up_transactions_to_old_contract() {
        for phase in [UpdatePhase::Prepared, UpdatePhase::BackedUp] {
            let path = temp_path("transactional-rollback-recovery");
            cleanup(&path);
            let (_, contract_id, stash_path) = import_fixture(&path);
            let contract_id = rgb::ContractId::from_str(&contract_id).unwrap();
            let live_contract = find_contract_directory(&stash_path, contract_id).unwrap();
            fs::write(live_contract.join("generation"), b"old").unwrap();
            let transaction = create_update_transaction(&stash_path, contract_id, true).unwrap();
            fs::write(transaction.staged_contract.join("generation"), b"new").unwrap();

            if phase == UpdatePhase::BackedUp {
                fs::rename(&transaction.live_contract, &transaction.backup_contract).unwrap();
                let mut journal = transaction.journal.clone();
                journal.phase = UpdatePhase::BackedUp;
                persist_update_journal(&transaction.transaction_dir, &journal).unwrap();
            }

            let reloaded =
                StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true)
                    .unwrap();
            assert!(reloaded
                .verify_transition(&contract_id.to_string())
                .unwrap());
            assert_eq!(
                fs::read(
                    find_contract_directory(&stash_path, contract_id)
                        .unwrap()
                        .join("generation")
                )
                .unwrap(),
                b"old"
            );
            assert!(transaction_residue(&stash_path).is_empty());
            cleanup(&path);
        }
    }

    #[test]
    fn startup_restores_old_contract_when_backup_rename_precedes_journal_update() {
        let path = temp_path("transactional-prepared-backup-recovery");
        cleanup(&path);
        let (_, contract_id, stash_path) = import_fixture(&path);
        let contract_id = rgb::ContractId::from_str(&contract_id).unwrap();
        let live_contract = find_contract_directory(&stash_path, contract_id).unwrap();
        fs::write(live_contract.join("generation"), b"old").unwrap();
        let transaction = create_update_transaction(&stash_path, contract_id, true).unwrap();
        fs::write(transaction.staged_contract.join("generation"), b"new").unwrap();

        fs::rename(&transaction.live_contract, &transaction.backup_contract).unwrap();
        assert_eq!(
            load_update_journal(&transaction.transaction_dir)
                .unwrap()
                .phase,
            UpdatePhase::Prepared
        );

        let reloaded =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        assert!(reloaded
            .verify_transition(&contract_id.to_string())
            .unwrap());
        assert_eq!(
            fs::read(
                find_contract_directory(&stash_path, contract_id)
                    .unwrap()
                    .join("generation")
            )
            .unwrap(),
            b"old"
        );
        assert!(transaction_residue(&stash_path).is_empty());
        cleanup(&path);
    }

    #[test]
    fn startup_restores_old_contract_when_promotion_precedes_journal_update() {
        let path = temp_path("transactional-backed-up-promotion-recovery");
        cleanup(&path);
        let (_, contract_id, stash_path) = import_fixture(&path);
        let contract_id = rgb::ContractId::from_str(&contract_id).unwrap();
        let live_contract = find_contract_directory(&stash_path, contract_id).unwrap();
        fs::write(live_contract.join("generation"), b"old").unwrap();
        let transaction = create_update_transaction(&stash_path, contract_id, true).unwrap();
        fs::write(transaction.staged_contract.join("generation"), b"new").unwrap();

        fs::rename(&transaction.live_contract, &transaction.backup_contract).unwrap();
        let mut journal = transaction.journal.clone();
        journal.phase = UpdatePhase::BackedUp;
        persist_update_journal(&transaction.transaction_dir, &journal).unwrap();
        fs::rename(&transaction.staged_contract, &transaction.live_contract).unwrap();

        let reloaded =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        assert!(reloaded
            .verify_transition(&contract_id.to_string())
            .unwrap());
        assert_eq!(
            fs::read(
                find_contract_directory(&stash_path, contract_id)
                    .unwrap()
                    .join("generation")
            )
            .unwrap(),
            b"old"
        );
        assert!(transaction_residue(&stash_path).is_empty());
        cleanup(&path);
    }

    #[test]
    fn startup_completes_promoted_transaction_cleanup() {
        let path = temp_path("transactional-promoted-recovery");
        cleanup(&path);
        let (_, contract_id, stash_path) = import_fixture(&path);
        let contract_id = rgb::ContractId::from_str(&contract_id).unwrap();
        let live_contract = find_contract_directory(&stash_path, contract_id).unwrap();
        fs::write(live_contract.join("generation"), b"old").unwrap();
        let transaction = create_update_transaction(&stash_path, contract_id, true).unwrap();
        fs::write(transaction.staged_contract.join("generation"), b"new").unwrap();
        validate_transaction_candidate(&transaction, contract_id, true).unwrap();

        fs::rename(&transaction.live_contract, &transaction.backup_contract).unwrap();
        let mut journal = transaction.journal.clone();
        journal.phase = UpdatePhase::BackedUp;
        persist_update_journal(&transaction.transaction_dir, &journal).unwrap();
        fs::rename(&transaction.staged_contract, &transaction.live_contract).unwrap();
        journal.phase = UpdatePhase::Promoted;
        persist_update_journal(&transaction.transaction_dir, &journal).unwrap();

        let reloaded =
            StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true).unwrap();
        assert!(reloaded
            .verify_transition(&contract_id.to_string())
            .unwrap());
        assert_eq!(
            fs::read(
                find_contract_directory(&stash_path, contract_id)
                    .unwrap()
                    .join("generation")
            )
            .unwrap(),
            b"new"
        );
        assert!(transaction_residue(&stash_path).is_empty());
        cleanup(&path);
    }

    #[test]
    fn corrupt_or_unprovable_update_journal_fails_startup_closed() {
        let path = temp_path("transactional-fail-closed");
        cleanup(&path);
        let (_, contract_id, stash_path) = import_fixture(&path);
        let contract_id = rgb::ContractId::from_str(&contract_id).unwrap();
        let transaction = create_update_transaction(&stash_path, contract_id, true).unwrap();
        fs::remove_dir_all(&transaction.live_contract).unwrap();
        fs::remove_dir_all(&transaction.staged_contract).unwrap();

        let result = StashResolver::new_with_network(&stash_path, "http://127.0.0.1:1/api", true);
        assert!(matches!(
            result,
            Err(ConxianError::Rgb(message)) if message.contains("cannot prove")
        ));
        cleanup(&path);
    }

    #[test]
    fn unknown_contract_import_does_not_invoke_seal_resolver() {
        let path = temp_path("unknown-import-resolver");
        cleanup(&path);
        let fixture_root = path.join("fixture");
        fs::create_dir_all(&fixture_root).unwrap();
        let (consignment, _) = write_consignment(&fixture_root);
        let import_path = path.join("import");
        fs::create_dir_all(&import_path).unwrap();
        let stockpile =
            StockpileDir::<bp::seals::TxoSeal>::load(import_path, rgb::Consensus::Bitcoin, true)
                .unwrap();
        let mut contracts: rgb::Contracts<StockpileDir<bp::seals::TxoSeal>> =
            rgb::Contracts::load(stockpile);
        let resolver_calls = Cell::new(0);

        contracts
            .consume_from_file(
                true,
                &consignment,
                |_: &rgb::Operation| {
                    resolver_calls.set(resolver_calls.get() + 1);
                    BTreeMap::new()
                },
                |_, _, _| Ok::<(), Infallible>(()),
            )
            .unwrap();
        assert_eq!(
            resolver_calls.get(),
            0,
            "allow_unknown=true unexpectedly invoked the seal resolver"
        );
        drop(contracts);
        cleanup(&path);
    }

    #[test]
    fn default_signature_policy_rejects_invalid_signature_bytes() {
        let validator = RejectIssuerSignatures;
        assert!(validator.validate(&[0xAA; 32], "issuer", &[0x01]).is_err());
    }
}
