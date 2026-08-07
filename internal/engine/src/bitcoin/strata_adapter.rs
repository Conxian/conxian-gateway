use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianResult};
use serde_json::{json, Value};
use tracing::info;

/// Protocol Adapter for Strata (T1 Rollup - CON-1268)
/// ZK-rollup for Bitcoin scalability.
pub struct StrataAdapter {
    pub network: String,
}

impl StrataAdapter {
    pub fn new(network: String) -> Self {
        Self { network }
    }
}

#[async_trait]
impl ChainAdapter for StrataAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        Ok(0)
    }

    async fn get_chain_identity(&self) -> String {
        format!("strata:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(chain = "strata", "Preparing Strata transaction");
        Ok(json!({
            "chain": "strata",
            "status": "prepared",
            "payload": tx_details,
            "type": "rollup_tx"
        }))
    }

    async fn verify_state_proof(&self, proof_metadata: Value) -> ConxianResult<bool> {
        info!(chain = "strata", "Verifying Strata state proof");

        // G-ST1: Structural batch root validation
        let batch_root = match proof_metadata["batch_root"].as_str() {
            Some(r) => r,
            None => {
                info!("Strata: missing batch_root in metadata");
                return Ok(false);
            }
        };

        // Reject non-hex batch root
        if hex::decode(batch_root).is_err() {
            info!("Strata: batch_root is not valid hex");
            return Ok(false);
        }

        // Exact length check: 32 bytes = 64 hex chars (standard Merkle root)
        if batch_root.len() != 64 {
            info!(len = batch_root.len(), "Strata: batch_root not 32 bytes (64 hex chars)");
            return Ok(false);
        }

        Ok(true)
    }
}
