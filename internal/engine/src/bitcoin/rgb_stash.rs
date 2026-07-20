use conxian_core::{ConxianError, ConxianResult};

#[cfg(feature = "rgb-native")]
use std::collections::{BTreeMap, HashMap};
#[cfg(feature = "rgb-native")]
use std::fs;
#[cfg(feature = "rgb-native")]
use std::net::IpAddr;
#[cfg(feature = "rgb-native")]
use std::path::{Path, PathBuf};
#[cfg(feature = "rgb-native")]
use std::str::FromStr;
#[cfg(feature = "rgb-native")]
use std::sync::{Mutex, RwLock};

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

/// StashResolver owns the RGB filesystem stockpile, a wallet-owned auth-token
/// registry, and a non-consensus metadata cache.
///
/// The stockpile is the only source used for native consensus verification.
/// The JSON cache is retained for backwards-compatible descriptive lookup, but
/// it can never make a contract or transition consensus-valid.
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

        let metadata_path = stockpile_dir.join("contract-metadata.json");
        let seal_registry_path = stockpile_dir.join("seal-registry.json");
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
    pub fn import_consignment<V: IssuerSignatureValidator>(
        &self,
        path: impl AsRef<Path>,
        expected_contract_id: &str,
        signature_validator: &V,
    ) -> ConxianResult<()> {
        let expected = canonical_contract_id(expected_contract_id)?;
        preflight_consignment(path.as_ref(), &expected, signature_validator)?;

        let mut stockpile = self
            .stockpile
            .lock()
            .map_err(|_| ConxianError::Rgb("RGB stockpile lock is poisoned".to_string()))?;
        let persistence = stockpile.clone();
        let mut contracts: rgb::Contracts<StockpileDir<bp::seals::TxoSeal>> =
            rgb::Contracts::load(persistence);
        let registry = &self.seal_registry;
        let result = contracts.consume_from_file(
            true,
            path,
            |operation| {
                resolve_registered_seals(registry, operation, |seal| {
                    let Some(source) = seal.to_src() else {
                        // Witness-relative seals are checked by RGB's
                        // consensus verifier against the consignment witness.
                        return true;
                    };
                    matches!(
                        self.check_utxo(
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
        );
        result.map_err(|_| {
            ConxianError::Rgb("RGB consignment consensus verification failed".to_string())
        })?;

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
        let temp_path = self.metadata_path.with_extension("tmp");

        if fs::write(&temp_path, json).is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(ConxianError::Rgb(
                "failed to write RGB stash metadata".to_string(),
            ));
        }
        if fs::rename(&temp_path, &self.metadata_path).is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(ConxianError::Rgb(
                "failed to commit RGB stash metadata".to_string(),
            ));
        }
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
) -> ConxianResult<()> {
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
    Ok(())
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
    let temp_path = path.with_extension("tmp");
    if fs::write(&temp_path, json).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Err(ConxianError::Rgb(
            "failed to write RGB seal registry".to_string(),
        ));
    }
    if fs::rename(&temp_path, path).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Err(ConxianError::Rgb(
            "failed to commit RGB seal registry".to_string(),
        ));
    }
    Ok(())
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use strict_encoding::{StreamWriter, StrictDumb};

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
    fn default_signature_policy_rejects_invalid_signature_bytes() {
        let validator = RejectIssuerSignatures;
        assert!(validator.validate(&[0xAA; 32], "issuer", &[0x01]).is_err());
    }
}
