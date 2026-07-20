use std::sync::Arc;

use async_trait::async_trait;
use conxian_core::{ContractState, ConxianError, ConxianResult, RgbAdapter, RolloutMode};
use serde::Deserialize;
use serde_json::{json, Value};

use super::rgb_native;
use super::StashResolver;

/// Injectable RGB node client used to keep adapter tests deterministic.
#[async_trait]
pub trait RgbNodeClient: Send + Sync {
    async fn fetch(&self, path: &str) -> ConxianResult<Option<Value>>;
}

struct HttpRgbNodeClient {
    base_url: String,
}

impl HttpRgbNodeClient {
    fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

#[async_trait]
impl RgbNodeClient for HttpRgbNodeClient {
    async fn fetch(&self, path: &str) -> ConxianResult<Option<Value>> {
        let url = format!("{}{}", self.base_url, path);
        tokio::task::spawn_blocking(move || {
            let response = minreq::get(&url)
                .with_timeout(2)
                .send()
                .map_err(|_| ConxianError::Rgb("RGB node request failed".to_string()))?;

            match response.status_code {
                200 => {
                    let body = response.as_str().map_err(|_| {
                        ConxianError::Rgb("RGB node returned invalid text".to_string())
                    })?;
                    serde_json::from_str(body).map(Some).map_err(|_| {
                        ConxianError::Rgb("RGB node returned invalid JSON".to_string())
                    })
                }
                404 => Ok(None),
                _ => Err(ConxianError::Rgb(
                    "RGB node returned an unexpected status".to_string(),
                )),
            }
        })
        .await
        .map_err(|_| ConxianError::Rgb("RGB node worker failed".to_string()))?
    }
}

#[derive(Debug, Deserialize)]
struct ContractLookupResponse {
    contract_id: String,
    schema_id: String,
    state: Value,
}

#[derive(Debug, Deserialize)]
struct VerifyResponse {
    valid: bool,
    #[serde(default)]
    contract_id: Option<String>,
    #[serde(default)]
    transition_id: Option<String>,
}

fn parse_contract_lookup_response(
    data: Value,
    expected_contract_id: &str,
) -> ConxianResult<ContractState> {
    let response: ContractLookupResponse = serde_json::from_value(data)
        .map_err(|_| ConxianError::Rgb("RGB node returned malformed contract state".to_string()))?;
    let response_contract_id = rgb_native::normalize_contract_id(&response.contract_id)?;
    if response_contract_id != expected_contract_id {
        return Err(ConxianError::Rgb(
            "RGB node returned a mismatched contract ID".to_string(),
        ));
    }
    if response.schema_id.trim().is_empty() || !response.state.is_object() {
        return Err(ConxianError::Rgb(
            "RGB node returned an invalid contract state shape".to_string(),
        ));
    }

    Ok(ContractState {
        contract_id: expected_contract_id.to_string(),
        schema_id: response.schema_id,
        state_data: response.state,
    })
}

fn parse_verify_response(data: Value, expected_id: &str) -> ConxianResult<bool> {
    let response: VerifyResponse = serde_json::from_value(data).map_err(|_| {
        ConxianError::Rgb("RGB node returned malformed verification response".to_string())
    })?;

    for response_id in [response.contract_id, response.transition_id]
        .into_iter()
        .flatten()
    {
        if rgb_native::normalize_contract_id(&response_id)? != expected_id {
            return Err(ConxianError::Rgb(
                "RGB node returned a mismatched verification ID".to_string(),
            ));
        }
    }

    Ok(response.valid)
}

/// RGB Protocol adapter with optional rgb-core v0.12 native integration.
///
/// - Disabled: no RGB work is performed.
/// - Shadow: node/native failures may use an explicitly simulated response.
/// - Active: the native stockpile is authoritative for verification and no
///   HTTP/simulation fallback is used as proof.
pub struct NodeRgbAdapter {
    pub mode: RolloutMode,
    pub node_url: String,
    pub stash: Option<Arc<StashResolver>>,
    node_client: Arc<dyn RgbNodeClient>,
}

impl NodeRgbAdapter {
    pub fn new(mode: RolloutMode, node_url: String) -> Self {
        let node_client = Arc::new(HttpRgbNodeClient::new(node_url.clone()));
        Self {
            mode,
            node_url,
            stash: None,
            node_client,
        }
    }

    pub fn with_stash(mut self, stash: Arc<StashResolver>) -> Self {
        self.stash = Some(stash);
        self
    }

    pub fn with_node_client(mut self, node_client: Arc<dyn RgbNodeClient>) -> Self {
        self.node_client = node_client;
        self
    }

    async fn fetch_from_node(&self, path: &str) -> ConxianResult<Option<Value>> {
        self.node_client.fetch(path).await
    }

    fn get_simulated_state(&self, contract_id: &str) -> Option<ContractState> {
        // Simulation is intentionally limited to the canonical HRI. It is not
        // a consensus parser and is never used by Active mode.
        if contract_id.starts_with("contract:") && contract_id.len() > "contract:".len() {
            Some(ContractState {
                contract_id: contract_id.to_string(),
                schema_id: "urn:rgb:schema:fungible".to_string(),
                state_data: json!({
                    "ticker": "CONX",
                    "supply": 1000000,
                    "precision": 8,
                    "simulated": true
                }),
            })
        } else {
            None
        }
    }
}

#[async_trait]
impl RgbAdapter for NodeRgbAdapter {
    async fn lookup_contract(&self, contract_id: &str) -> ConxianResult<Option<ContractState>> {
        match self.mode {
            RolloutMode::Disabled => Ok(None),
            RolloutMode::Shadow | RolloutMode::Active => {
                let canonical_id = rgb_native::normalize_contract_id(contract_id)?;
                if matches!(self.mode, RolloutMode::Active) {
                    if !rgb_native::verify_transition_native(&canonical_id, &self.stash)? {
                        return Ok(None);
                    }
                    return rgb_native::lookup_contract_native(&canonical_id, &self.stash).map(
                        |data| {
                            data.map(|data| ContractState {
                                contract_id: canonical_id.clone(),
                                schema_id: data["schema_id"]
                                    .as_str()
                                    .unwrap_or("urn:rgb:schema:fungible")
                                    .to_string(),
                                state_data: data,
                            })
                        },
                    );
                }

                match self
                    .fetch_from_node(&format!("/contract/{canonical_id}"))
                    .await
                {
                    Ok(Some(data)) => match parse_contract_lookup_response(data, &canonical_id) {
                        Ok(state) => Ok(Some(state)),
                        Err(error) if matches!(self.mode, RolloutMode::Shadow) => {
                            let _ = error;
                            Ok(self.get_simulated_state(&canonical_id))
                        }
                        Err(error) => Err(error),
                    },
                    Ok(None) if matches!(self.mode, RolloutMode::Shadow) => {
                        Ok(self.get_simulated_state(&canonical_id))
                    }
                    Ok(None) => Ok(None),
                    Err(error) if matches!(self.mode, RolloutMode::Shadow) => {
                        let _ = error;
                        Ok(self.get_simulated_state(&canonical_id))
                    }
                    Err(error) => Err(error),
                }
            }
        }
    }

    async fn verify_transition(&self, transition_id: &str) -> ConxianResult<bool> {
        match self.mode {
            RolloutMode::Disabled => Ok(false),
            RolloutMode::Shadow | RolloutMode::Active => {
                let canonical_id = rgb_native::normalize_contract_id(transition_id)?;
                if matches!(self.mode, RolloutMode::Active) {
                    return rgb_native::verify_transition_native(&canonical_id, &self.stash);
                }

                match self
                    .fetch_from_node(&format!("/verify/{canonical_id}"))
                    .await
                {
                    Ok(Some(data)) => match parse_verify_response(data, &canonical_id) {
                        Ok(valid) => Ok(valid),
                        Err(error) if matches!(self.mode, RolloutMode::Shadow) => {
                            let _ = error;
                            Ok(self.get_simulated_state(&canonical_id).is_some())
                        }
                        Err(error) => Err(error),
                    },
                    Ok(None) if matches!(self.mode, RolloutMode::Shadow) => {
                        Ok(self.get_simulated_state(&canonical_id).is_some())
                    }
                    Ok(None) => Ok(false),
                    Err(error) if matches!(self.mode, RolloutMode::Shadow) => {
                        let _ = error;
                        Ok(self.get_simulated_state(&canonical_id).is_some())
                    }
                    Err(error) => Err(error),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const VALID_ID: &str = "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg";
    const MNEMONIC_ID: &str =
        "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg#fractal-fashion-capsule";
    const OTHER_ID: &str = "contract:AAAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA";

    struct TestNodeClient {
        response: Result<Option<Value>, &'static str>,
        paths: Option<Arc<Mutex<Vec<String>>>>,
    }

    #[async_trait]
    impl RgbNodeClient for TestNodeClient {
        async fn fetch(&self, path: &str) -> ConxianResult<Option<Value>> {
            if let Some(paths) = &self.paths {
                paths.lock().expect("test path lock").push(path.to_string());
            }
            self.response
                .clone()
                .map_err(|message| ConxianError::Rgb(message.to_string()))
        }
    }

    fn adapter(mode: RolloutMode, response: Result<Option<Value>, &'static str>) -> NodeRgbAdapter {
        NodeRgbAdapter::new(mode, "http://127.0.0.1:1".to_string()).with_node_client(Arc::new(
            TestNodeClient {
                response,
                paths: None,
            },
        ))
    }

    fn adapter_with_paths(
        mode: RolloutMode,
        response: Result<Option<Value>, &'static str>,
    ) -> (NodeRgbAdapter, Arc<Mutex<Vec<String>>>) {
        let paths = Arc::new(Mutex::new(Vec::new()));
        let adapter = NodeRgbAdapter::new(mode, "http://127.0.0.1:1".to_string()).with_node_client(
            Arc::new(TestNodeClient {
                response,
                paths: Some(paths.clone()),
            }),
        );
        (adapter, paths)
    }

    fn valid_lookup_response(contract_id: &str) -> Value {
        json!({
            "contract_id": contract_id,
            "schema_id": "urn:rgb:schema:fungible",
            "state": {
                "ticker": "CONX",
                "supply": 1_000_000,
                "precision": 8
            }
        })
    }

    #[tokio::test]
    async fn shadow_lookup_uses_simulation_only_after_node_failure() {
        let adapter = adapter(RolloutMode::Shadow, Err("offline"));
        let result = adapter.lookup_contract(VALID_ID).await.unwrap();
        assert_eq!(result.unwrap().state_data["simulated"], true);
    }

    #[tokio::test]
    async fn active_lookup_returns_node_failure_instead_of_simulation() {
        let adapter = adapter(RolloutMode::Active, Err("offline"));
        assert!(adapter.lookup_contract(VALID_ID).await.is_err());
    }

    #[tokio::test]
    async fn active_lookup_rejects_empty_legacy_and_mutated_contracts_before_http() {
        let adapter = adapter(
            RolloutMode::Active,
            Ok(Some(valid_lookup_response(VALID_ID))),
        );
        for contract_id in [
            "",
            "rgb:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg",
            "contract:",
            "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCg!",
        ] {
            let result = adapter.lookup_contract(contract_id).await;
            assert!(
                matches!(result, Err(ConxianError::Rgb(_))),
                "{contract_id:?}"
            );
        }
    }

    #[tokio::test]
    async fn active_lookup_requires_native_stockpile_before_any_http_lookup() {
        let adapter = adapter(RolloutMode::Active, Ok(Some(json!({}))));
        assert!(matches!(
            adapter.lookup_contract(VALID_ID).await,
            Err(ConxianError::Rgb(_))
        ));
    }

    #[tokio::test]
    async fn shadow_lookup_simulates_after_malformed_http_success() {
        let adapter = adapter(RolloutMode::Shadow, Ok(Some(json!({}))));
        let result = adapter.lookup_contract(VALID_ID).await.unwrap().unwrap();
        assert_eq!(result.contract_id, VALID_ID);
        assert_eq!(result.state_data["simulated"], true);
    }

    #[tokio::test]
    async fn active_lookup_rejects_missing_native_stockpile_before_http_contract_id() {
        let adapter = adapter(
            RolloutMode::Active,
            Ok(Some(valid_lookup_response(OTHER_ID))),
        );
        assert!(matches!(
            adapter.lookup_contract(VALID_ID).await,
            Err(ConxianError::Rgb(_))
        ));
    }

    #[tokio::test]
    async fn active_lookup_does_not_use_http_as_native_proof() {
        let (adapter, paths) = adapter_with_paths(
            RolloutMode::Active,
            Ok(Some(valid_lookup_response(MNEMONIC_ID))),
        );
        assert!(adapter.lookup_contract(MNEMONIC_ID).await.is_err());
        assert!(paths.lock().expect("test path lock").is_empty());
    }

    #[tokio::test]
    async fn active_verify_fails_closed_for_unknown_contract() {
        let adapter = adapter(RolloutMode::Active, Ok(None));
        assert!(matches!(
            adapter.verify_transition(VALID_ID).await,
            Err(ConxianError::Rgb(_))
        ));
    }

    #[tokio::test]
    async fn shadow_verify_may_fallback_when_node_is_unknown() {
        let adapter = adapter(RolloutMode::Shadow, Ok(None));
        assert!(adapter.verify_transition(VALID_ID).await.unwrap());
    }

    #[tokio::test]
    async fn active_verify_does_not_accept_malformed_http_success_as_proof() {
        let adapter = adapter(RolloutMode::Active, Ok(Some(json!({}))));
        assert!(matches!(
            adapter.verify_transition(VALID_ID).await,
            Err(ConxianError::Rgb(_))
        ));
    }

    #[tokio::test]
    async fn shadow_verify_simulates_after_malformed_http_success() {
        let adapter = adapter(RolloutMode::Shadow, Ok(Some(json!({}))));
        assert!(adapter.verify_transition(VALID_ID).await.unwrap());
    }

    #[tokio::test]
    async fn active_verify_does_not_accept_mismatched_http_id_as_proof() {
        let adapter = adapter(
            RolloutMode::Active,
            Ok(Some(json!({"valid": true, "contract_id": OTHER_ID}))),
        );
        assert!(matches!(
            adapter.verify_transition(VALID_ID).await,
            Err(ConxianError::Rgb(_))
        ));
    }

    #[tokio::test]
    async fn active_verify_does_not_use_canonical_id_for_http() {
        let (adapter, paths) = adapter_with_paths(
            RolloutMode::Active,
            Ok(Some(json!({"valid": true, "contract_id": MNEMONIC_ID}))),
        );
        assert!(adapter.verify_transition(MNEMONIC_ID).await.is_err());
        assert!(paths.lock().expect("test path lock").is_empty());
    }

    #[cfg(feature = "rgb-native")]
    #[tokio::test]
    async fn active_metadata_cache_never_counts_as_consensus_proof() {
        let path = std::env::temp_dir().join("conxian-rgb-adapter-cache-only");
        let _ = std::fs::remove_dir_all(&path);
        let resolver = StashResolver::new(&path, "https://blockstream.info/api").unwrap();
        resolver
            .store_contract(crate::bitcoin::rgb_stash::ContractMeta {
                contract_id: VALID_ID.to_string(),
                ticker: Some("CONX".to_string()),
                name: Some("Conxian".to_string()),
                supply: Some(1_000_000),
                precision: Some(8),
                last_transition: None,
            })
            .unwrap();
        let (adapter, paths) = adapter_with_paths(RolloutMode::Active, Ok(None));
        let adapter = adapter.with_stash(Arc::new(resolver));
        assert!(!adapter.verify_transition(VALID_ID).await.unwrap());
        assert!(paths.lock().expect("test path lock").is_empty());
        let _ = std::fs::remove_dir_all(path);
    }

    #[tokio::test]
    async fn disabled_mode_is_a_noop() {
        let adapter = adapter(RolloutMode::Disabled, Err("must not be called"));
        assert!(adapter.lookup_contract(VALID_ID).await.unwrap().is_none());
        assert!(!adapter.verify_transition(VALID_ID).await.unwrap());
    }
}
