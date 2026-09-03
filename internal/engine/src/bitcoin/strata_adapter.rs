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

    async fn verify_state_proof(&self, _proof_metadata: Value) -> ConxianResult<bool> {
        warn!(
            chain = "strata",
            network = %self.network,
            "Strata state-proof verification is disabled until a trusted ZK-proof backend is wired"
        );
        // Caller-supplied metadata is not a cryptographic proof or a trusted
        // rollup observation, so production verification remains fail-closed.
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn verify_state_proof_fails_closed() {
        let adapter = StrataAdapter::new("regtest".to_string());

        // Even a structurally well-formed 32-byte batch_root must not be
        // accepted as verified until a real ZK-proof backend is wired.
        for metadata in [
            json!({}),
            json!({"verified": true, "claim": "accepted"}),
            json!({"batch_root": "11".repeat(32)}),
        ] {
            assert!(!adapter.verify_state_proof(metadata).await.unwrap());
        }
    }
}
