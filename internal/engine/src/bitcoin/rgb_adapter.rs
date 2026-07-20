use std::sync::Arc;

use async_trait::async_trait;
use conxian_core::{ContractState, ConxianError, ConxianResult, RgbAdapter, RolloutMode};
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

/// RGB Protocol adapter with optional rgb-core v0.12 native integration.
///
/// - Disabled: no RGB work is performed.
/// - Shadow: node/native failures may use an explicitly simulated response.
/// - Active: native and HTTP failures are returned; unknown contracts do not
///   become simulated successes.
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
                if matches!(self.mode, RolloutMode::Active) {
                    #[cfg(feature = "rgb-native")]
                    rgb_native::validate_contract_id_native(contract_id)?;
                    if let Ok(Some(data)) =
                        rgb_native::lookup_contract_native(contract_id, &self.stash)
                    {
                        return Ok(Some(ContractState {
                            contract_id: contract_id.to_string(),
                            schema_id: data["schema_id"]
                                .as_str()
                                .unwrap_or("urn:rgb:schema:fungible")
                                .to_string(),
                            state_data: data,
                        }));
                    }
                }

                match self
                    .fetch_from_node(&format!("/contract/{contract_id}"))
                    .await
                {
                    Ok(Some(data)) => {
                        let state_data = data.get("state").cloned().unwrap_or_else(|| data.clone());
                        Ok(Some(ContractState {
                            contract_id: contract_id.to_string(),
                            schema_id: data["schema_id"].as_str().unwrap_or("unknown").to_string(),
                            state_data,
                        }))
                    }
                    Ok(None) if matches!(self.mode, RolloutMode::Shadow) => {
                        Ok(self.get_simulated_state(contract_id))
                    }
                    Ok(None) => Ok(None),
                    Err(error) if matches!(self.mode, RolloutMode::Shadow) => {
                        let _ = error;
                        Ok(self.get_simulated_state(contract_id))
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
                if matches!(self.mode, RolloutMode::Active) {
                    #[cfg(feature = "rgb-native")]
                    rgb_native::validate_contract_id_native(transition_id)?;
                    match rgb_native::verify_transition_native(transition_id, &self.stash) {
                        Ok(true) => return Ok(true),
                        Ok(false) => return Ok(false),
                        Err(_) => {}
                    }
                }

                match self
                    .fetch_from_node(&format!("/verify/{transition_id}"))
                    .await
                {
                    Ok(Some(data)) => Ok(data["valid"].as_bool().unwrap_or(false)),
                    Ok(None) if matches!(self.mode, RolloutMode::Shadow) => {
                        Ok(self.get_simulated_state(transition_id).is_some())
                    }
                    Ok(None) => Ok(false),
                    Err(error) if matches!(self.mode, RolloutMode::Shadow) => {
                        let _ = error;
                        Ok(self.get_simulated_state(transition_id).is_some())
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

    const VALID_ID: &str = "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg";

    struct TestNodeClient {
        response: Result<Option<Value>, &'static str>,
    }

    #[async_trait]
    impl RgbNodeClient for TestNodeClient {
        async fn fetch(&self, _path: &str) -> ConxianResult<Option<Value>> {
            self.response
                .clone()
                .map_err(|message| ConxianError::Rgb(message.to_string()))
        }
    }

    fn adapter(mode: RolloutMode, response: Result<Option<Value>, &'static str>) -> NodeRgbAdapter {
        NodeRgbAdapter::new(mode, "http://127.0.0.1:1".to_string())
            .with_node_client(Arc::new(TestNodeClient { response }))
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

    #[cfg(feature = "rgb-native")]
    #[tokio::test]
    async fn active_lookup_rejects_mutated_contract_before_http_fallback() {
        let adapter = adapter(
            RolloutMode::Active,
            Ok(Some(json!({"schema_id": "test", "state": {}}))),
        );
        let result = adapter
            .lookup_contract("contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCg!")
            .await;
        assert!(matches!(result, Err(ConxianError::Rgb(_))));
    }

    #[tokio::test]
    async fn active_verify_fails_closed_for_unknown_contract() {
        let adapter = adapter(RolloutMode::Active, Ok(None));
        assert!(!adapter.verify_transition(VALID_ID).await.unwrap());
    }

    #[tokio::test]
    async fn shadow_verify_may_fallback_when_node_is_unknown() {
        let adapter = adapter(RolloutMode::Shadow, Ok(None));
        assert!(adapter.verify_transition(VALID_ID).await.unwrap());
    }

    #[tokio::test]
    async fn disabled_mode_is_a_noop() {
        let adapter = adapter(RolloutMode::Disabled, Err("must not be called"));
        assert!(adapter.lookup_contract(VALID_ID).await.unwrap().is_none());
        assert!(!adapter.verify_transition(VALID_ID).await.unwrap());
    }
}
