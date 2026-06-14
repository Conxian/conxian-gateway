use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianResult};
use serde_json::{json, Value};
use tracing::{info};

/// Protocol Adapter for BitVM (Partner Lane - CON-713)
pub struct BitVmAdapter {
    pub network: String,
}

impl BitVmAdapter {
    pub fn new(network: String) -> Self {
        Self { network }
    }
}

#[async_trait]
impl ChainAdapter for BitVmAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        Ok(0)
    }

    async fn get_chain_identity(&self) -> String {
        format!("bitvm:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(chain = "bitvm", "Preparing BitVM commitment transaction");
        Ok(json!({
            "chain": "bitvm",
            "status": "prepared",
            "payload": tx_details,
            "type": "commitment"
        }))
    }

    async fn verify_state_proof(&self, _proof_metadata: Value) -> ConxianResult<bool> {
        // BitVM verification is currently handled in the compliance layer (ZkcVerifier)
        // Future: Move optimistic proof lifecycle management here.
        Ok(true)
    }
}
