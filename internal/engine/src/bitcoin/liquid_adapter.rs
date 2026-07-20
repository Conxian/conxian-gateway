use crate::bitcoin::rpc::BitcoinRpc;
use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianResult};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Protocol Adapter for Liquid Network (Pilot Lane - CON-710)
pub struct LiquidAdapter {
    pub rpc: Arc<dyn BitcoinRpc>,
    pub network: String,
}

impl LiquidAdapter {
    pub fn new(rpc: Arc<dyn BitcoinRpc>, network: String) -> Self {
        Self { rpc, network }
    }
}

#[async_trait]
impl ChainAdapter for LiquidAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        debug!("Fetching latest height for Liquid");
        self.rpc.get_block_count().await
    }

    async fn get_chain_identity(&self) -> String {
        format!("liquid:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(chain = "liquid", "Preparing unsigned transaction");
        // Pilot implementation: return a prepared skeleton for Elements-based UTXO
        Ok(json!({
            "chain": "liquid",
            "status": "prepared",
            "payload": tx_details,
            "confidential": true,
            "version": "elements-v1"
        }))
    }

    async fn verify_state_proof(&self, _proof_metadata: Value) -> ConxianResult<bool> {
        warn!(
            chain = "liquid",
            network = %self.network,
            "Liquid state-proof verification is disabled until a trusted Elements/parent-chain proof backend is wired"
        );
        // Caller-supplied metadata is not a cryptographic proof or a trusted
        // chain observation, so production verification remains fail-closed.
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitcoin::rpc::BitcoinRpc;
    use async_trait::async_trait;
    use conxian_core::BlockInfo;

    struct UnusedRpc;

    #[async_trait]
    impl BitcoinRpc for UnusedRpc {
        async fn get_block_count(&self) -> ConxianResult<u64> {
            Ok(0)
        }

        async fn get_block_info(&self, _height: u64) -> ConxianResult<BlockInfo> {
            Ok(BlockInfo {
                hash: String::new(),
                height: 0,
                timestamp: 0,
            })
        }

        async fn get_network_info(&self) -> ConxianResult<String> {
            Ok("regtest".to_string())
        }
    }

    #[tokio::test]
    async fn arbitrary_state_proof_metadata_is_rejected_fail_closed() {
        let adapter = LiquidAdapter::new(Arc::new(UnusedRpc), "elementsregtest".to_string());

        for metadata in [json!({}), json!({"verified": true, "claim": "accepted"})] {
            assert!(!adapter.verify_state_proof(metadata).await.unwrap());
        }
    }
}
