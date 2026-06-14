use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianResult};
use serde_json::{json, Value};
use tracing::{info};

/// Protocol Adapter for Babylon (Partner Lane - CON-712)
pub struct BabylonAdapter {
    pub network: String,
}

impl BabylonAdapter {
    pub fn new(network: String) -> Self {
        Self { network }
    }
}

#[async_trait]
impl ChainAdapter for BabylonAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        Ok(0)
    }

    async fn get_chain_identity(&self) -> String {
        format!("babylon:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(chain = "babylon", "Preparing staking transaction");
        Ok(json!({
            "chain": "babylon",
            "status": "prepared",
            "payload": tx_details,
            "type": "staking"
        }))
    }

    async fn verify_state_proof(&self, _proof_metadata: Value) -> ConxianResult<bool> {
        Ok(true)
    }
}
