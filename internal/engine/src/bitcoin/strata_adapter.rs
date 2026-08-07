use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianResult};
use serde_json::{json, Value};
use tracing::{info, warn};

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

        // G-ST1: Validate batch_root is a well-formed 32-byte hex hash
        let batch_root = proof_metadata["batch_root"].as_str().unwrap_or("");
        if batch_root.is_empty() {
            warn!(chain = "strata", "Missing batch_root in metadata");
            return Ok(false);
        }
        let root_bytes: Vec<u8> = match <Vec<u8> as bitcoin::hex::FromHex>::from_hex(batch_root) {
            Ok(b) if b.len() == 32 => b,
            _ => {
                warn!(chain = "strata", "Invalid batch_root hex format");
                return Ok(false);
            }
        };
        info!(
            chain = "strata",
            batch_root_len = root_bytes.len(),
            "Batch root structurally valid"
        );
        Ok(true)
    }
}
