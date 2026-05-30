use async_trait::async_trait;
use conxian_core::{ContractState, ConxianResult, RgbAdapter, RolloutMode};
use serde_json::json;
use tracing::{info, warn};

pub struct NodeRgbAdapter {
    pub mode: RolloutMode,
    pub node_url: String,
}

impl NodeRgbAdapter {
    pub fn new(mode: RolloutMode, node_url: String) -> Self {
        Self { mode, node_url }
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

                // Simulate node-backed lookup
                // In a real implementation, this would perform an HTTP call to the RGB node
                if contract_id.starts_with("rgb:") {
                    let state = ContractState {
                        contract_id: contract_id.to_string(),
                        schema_id: "urn:rgb:schema:fungible".to_string(),
                        state_data: json!({
                            "ticker": "CONX",
                            "supply": 1000000,
                            "precision": 8
                        }),
                    };

                    if matches!(self.mode, RolloutMode::Shadow) {
                        info!("Shadow mode: contract found but result will be ignored by execution path");
                    }

                    Ok(Some(state))
                } else {
                    Ok(None)
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
                // Simulate verification
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rgb_lookup_shadow_mode() {
        let adapter = NodeRgbAdapter::new(RolloutMode::Shadow, "http://localhost:8080".to_string());
        let result = adapter.lookup_contract("rgb:test").await.unwrap();
        assert!(result.is_some());
        let state = result.unwrap();
        assert_eq!(
            state.state_data.get("ticker").and_then(|t| t.as_str()),
            Some("CONX")
        );
    }
}
