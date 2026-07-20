use conxian_core::{ConxianError, ConxianResult};

#[cfg(feature = "rgb-native")]
use std::collections::HashMap;
#[cfg(feature = "rgb-native")]
use std::fs;
#[cfg(feature = "rgb-native")]
use std::net::IpAddr;
#[cfg(feature = "rgb-native")]
use std::path::{Path, PathBuf};
#[cfg(feature = "rgb-native")]
use std::str::FromStr;
#[cfg(feature = "rgb-native")]
use std::sync::RwLock;

#[cfg(feature = "rgb-native")]
use serde::{Deserialize, Serialize};

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

/// StashResolver provides contract lookup and transition verification using
/// the locked RGB parser, bp-esplora for Bitcoin UTXO queries, and a
/// file-backed JSON cache for contract metadata.
///
/// The cache is intentionally only a Phase 1.5 metadata boundary. Full RGB
/// consensus verification, consignment handling, and Stockpile integration
/// remain Phase 2 work for issue #228.
#[cfg(feature = "rgb-native")]
pub struct StashResolver {
    cache: RwLock<HashMap<String, ContractMeta>>,
    db_path: PathBuf,
    #[allow(dead_code)]
    esplora_url: String,
    esplora_client: esplora::BlockingClient,
}

#[cfg(feature = "rgb-native")]
impl StashResolver {
    /// Creates a new stash resolver and loads existing metadata.
    ///
    /// Missing metadata is treated as an empty cache. Read, parse, validation,
    /// and client-construction failures are returned to the caller so startup
    /// cannot silently continue with corrupted state.
    pub fn new(db_path: impl AsRef<Path>, esplora_url: &str) -> ConxianResult<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        if db_path.as_os_str().is_empty() {
            return Err(ConxianError::Rgb(
                "RGB stash path must not be empty".to_string(),
            ));
        }

        validate_endpoint(esplora_url)?;
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|_| {
                    ConxianError::Rgb("failed to create RGB stash directory".to_string())
                })?;
            }
        }

        let cache = load_cache(&db_path)?;
        let esplora_client = esplora::Builder::new(esplora_url)
            .timeout(5)
            .max_retries(0)
            .build_blocking()
            .map_err(|_| ConxianError::Rgb("failed to initialize Esplora client".to_string()))?;

        Ok(Self {
            cache: RwLock::new(cache),
            db_path,
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

    /// Performs the Phase 1.5 stash-presence check for a transition ID.
    ///
    /// This is deliberately not a full `ContractVerify` implementation: a
    /// valid ID that is absent from the local stash returns `false` rather than
    /// being treated as consensus-valid.
    pub fn verify_transition(&self, transition_id: &str) -> ConxianResult<bool> {
        let key = canonical_contract_id(transition_id)?;
        let cache = self
            .cache
            .read()
            .map_err(|_| ConxianError::Rgb("RGB stash lock is poisoned".to_string()))?;
        Ok(cache.contains_key(&key))
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
        let temp_path = self.db_path.with_extension("tmp");

        if fs::write(&temp_path, json).is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(ConxianError::Rgb(
                "failed to write RGB stash metadata".to_string(),
            ));
        }
        if fs::rename(&temp_path, &self.db_path).is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(ConxianError::Rgb(
                "failed to commit RGB stash metadata".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(feature = "rgb-native")]
fn canonical_contract_id(input: &str) -> ConxianResult<String> {
    let parsed = input.parse::<rgb::ContractId>().map_err(|_| {
        ConxianError::Rgb("invalid RGB contract ID; expected contract: Baid64".to_string())
    })?;
    let canonical = parsed.to_string();
    if !input.starts_with("contract:") || !canonical.starts_with("contract:") {
        return Err(ConxianError::Rgb(
            "invalid RGB contract ID; expected contract: Baid64".to_string(),
        ));
    }
    Ok(canonical)
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

    const VALID_ID: &str = "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg";
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    fn temp_path(label: &str) -> PathBuf {
        let id = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("conxian-rgb-{label}-{id}.json"))
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
        let _ = fs::remove_file(path.with_extension("tmp"));
    }

    #[test]
    fn parses_canonical_contract_id_and_rejects_legacy_rgb_ids() {
        assert_eq!(canonical_contract_id(VALID_ID).unwrap(), VALID_ID);
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
        assert!(!path.with_extension("tmp").exists());

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
        fs::write(&path, b"not-json").unwrap();
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
}
