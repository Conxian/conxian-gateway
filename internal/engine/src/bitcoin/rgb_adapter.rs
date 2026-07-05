use async_trait::async_trait;
use conxian_core::{ContractState, ConxianResult, RgbAdapter, RolloutMode};
use serde_json::{json, Value};
use tracing::{error, info, warn};

use super::rgb_native;

/// RGB Protocol adapter with optional rgb-core v0.12 native integration.
///
/// Three rollout modes:
/// - Disabled: All operations return None/false
/// - Shadow:  Tries HTTP node → falls back to simulation on failure
/// - Active:  Tries rgb-native (if enabled) → HTTP node → simulation fallback
///
/// Enable native verification with: `cargo build --features rgb-native`
pub struct NodeRgbAdapter {
    pub mode: RolloutMode,
    pub node_url: String,
}

impl NodeRgbAdapter {
    pub fn new(mode: RolloutMode, node_url: String) -> Self {
        Self { mode, node_url }
    }

    async fn fetch_from_node(&self, path: &str) -> ConxianResult<Option<Value>> {
        let url = format!("{}{}", self.node_url, path);

        tokio::task::spawn_blocking(move || {
            let res = minreq::get(&url).with_timeout(2).send();

            match res {
                Ok(res) if res.status_code == 200 => {
                    let body = res
                        .as_str()
                        .map_err(|e| conxian_core::ConxianError::Bitcoin(e.to_string()))?;
                    let val: Value = serde_json::from_str(body)
                        .map_err(|e| conxian_core::ConxianError::Bitcoin(e.to_string()))?;
                    Ok(Some(val))
                }
                Ok(res) if res.status_code == 404 => Ok(None),
                Ok(res) => {
                    warn!(
                        status = res.status_code,
                        "RGB node returned unexpected status"
                    );
                    Err(conxian_core::ConxianError::Bitcoin(format!(
                        "RGB node error: status {}",
                        res.status_code
                    )))
                }
                Err(e) => {
                    warn!(error = %e, "Failed to connect to RGB node");
                    Err(conxian_core::ConxianError::Bitcoin(format!(
                        "RGB node connection failed: {}",
                        e
                    )))
                }
            }
        })
        .await
        .map_err(|e| conxian_core::ConxianError::Internal(e.to_string()))?
    }

    fn get_simulated_state(&self, contract_id: &str) -> Option<ContractState> {
        if contract_id.starts_with("rgb:") {
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
            RolloutMode::Disabled => {
                warn!("RGB lookup attempted while adapter is Disabled");
                Ok(None)
            }
            RolloutMode::Shadow | RolloutMode::Active => {
                info!(
                    "RGB lookup (mode={:?}) for contract: {}",
                    self.mode, contract_id
                );

                // In Active mode, try native rgb-core lookup first
                if matches!(self.mode, RolloutMode::Active) {
                    match rgb_native::lookup_contract_native(contract_id) {
                        Ok(Some(data)) => {
                            info!("RGB native lookup succeeded for: {}", contract_id);
                            return Ok(Some(ContractState {
                                contract_id: contract_id.to_string(),
                                schema_id: data["schema_id"]
                                    .as_str()
                                    .unwrap_or("urn:rgb:schema:fungible")
                                    .to_string(),
                                state_data: data,
                            }));
                        }
                        Ok(None) => {
                            info!("RGB native lookup returned None for: {}", contract_id);
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                "RGB native lookup unavailable, falling back to HTTP node"
                            );
                        }
                    }
                }

                // Attempt real node lookup
                let path = format!("/contract/{}", contract_id);
                let node_result = self.fetch_from_node(&path).await;

                match node_result {
                    Ok(Some(data)) => {
                        let state = ContractState {
                            contract_id: contract_id.to_string(),
                            schema_id: data["schema_id"].as_str().unwrap_or("unknown").to_string(),
                            state_data: data["state"].clone(),
                        };
                        Ok(Some(state))
                    }
                    Ok(None) => {
                        // Fallback to simulation if node returns 404
                        if let Some(state) = self.get_simulated_state(contract_id) {
                            if matches!(self.mode, RolloutMode::Shadow) {
                                info!("Shadow mode: node returned 404, using simulated state");
                            }
                            Ok(Some(state))
                        } else {
                            Ok(None)
                        }
                    }
                    Err(e) => {
                        // In shadow mode, we fallback to simulation even on connection error
                        if matches!(self.mode, RolloutMode::Shadow) {
                            error!(
                                error = %e,
                                "RGB node lookup failed in shadow mode; falling back to simulation"
                            );
                            Ok(self.get_simulated_state(contract_id))
                        } else {
                            Err(e)
                        }
                    }
                }
            }
        }
    }

    async fn verify_transition(&self, transition_id: &str) -> ConxianResult<bool> {
        match self.mode {
            RolloutMode::Disabled => Ok(false),
            RolloutMode::Shadow | RolloutMode::Active => {
                info!(
                    "RGB transition verification (mode={:?}) for: {}",
                    self.mode, transition_id
                );

                // In Active mode, try native rgb-core verification first
                if matches!(self.mode, RolloutMode::Active) {
                    match rgb_native::verify_transition_native(transition_id) {
                        Ok(true) => {
                            info!("RGB native verification passed for: {}", transition_id);
                            return Ok(true);
                        }
                        Ok(false) => {
                            warn!(
                                "RGB native verification returned false for: {}",
                                transition_id
                            );
                            return Ok(false);
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                "RGB native verification unavailable, falling back to HTTP node"
                            );
                        }
                    }
                }

                let path = format!("/verify/{}", transition_id);
                match self.fetch_from_node(&path).await {
                    Ok(Some(data)) => Ok(data["valid"].as_bool().unwrap_or(false)),
                    Ok(None) => Ok(true), // Simulation fallback
                    Err(e) => {
                        if matches!(self.mode, RolloutMode::Shadow) {
                            error!(error = %e, "RGB verification failed in shadow mode");
                            Ok(true)
                        } else {
                            Err(e)
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rgb_lookup_shadow_mode_simulation_fallback() {
        let adapter = NodeRgbAdapter::new(RolloutMode::Shadow, "http://localhost:8080".to_string());
        // This will attempt a fetch and likely fail (or return 404 if no node), triggering fallback
        let result = adapter.lookup_contract("rgb:test").await.unwrap();
        assert!(result.is_some());
        let state = result.unwrap();
        assert_eq!(
            state.state_data.get("ticker").and_then(|t| t.as_str()),
            Some("CONX")
        );
    }
}
