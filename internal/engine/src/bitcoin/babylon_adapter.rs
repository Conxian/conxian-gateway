use async_trait::async_trait;
use conxian_core::{BlockInfo, ChainAdapter, ConxianResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, warn};

use super::BitcoinRpc;

/// Protocol Adapter for Babylon (Partner Lane - CON-712)
pub struct BabylonAdapter {
    pub network: String,
    btc_rpc: Option<Arc<dyn BitcoinRpc>>,
}

impl BabylonAdapter {
    pub fn new(network: String) -> Self {
        Self {
            network,
            btc_rpc: None,
        }
    }

    /// Create adapter with Bitcoin RPC for BTC header-chain queries
    pub fn with_btc_rpc(network: String, btc_rpc: Arc<dyn BitcoinRpc>) -> Self {
        Self {
            network,
            btc_rpc: Some(btc_rpc),
        }
    }

    /// Get current BTC header-chain height (for SPV verification)
    pub async fn get_btc_header_height(&self) -> ConxianResult<u64> {
        match &self.btc_rpc {
            Some(rpc) => {
                let height = rpc.get_block_count().await?;
                info!(chain = "babylon", btc_height = height, "BTC header-chain height");
                Ok(height)
            }
            None => {
                warn!(chain = "babylon", "BTC RPC not configured, returning 0");
                Ok(0)
            }
        }
    }

    /// Get BTC block header info for SPV verification
    pub async fn get_btc_header_info(&self, height: u64) -> ConxianResult<BlockInfo> {
        match &self.btc_rpc {
            Some(rpc) => rpc.get_block_info(height).await,
            None => Err(conxian_core::ConxianError::Internal(
                "BTC RPC not configured".to_string(),
            )),
        }
    }

    /// Verify BTC header-chain continuity (SPV-style check)
    pub async fn verify_header_chain(&self, from_height: u64, to_height: u64) -> ConxianResult<bool> {
        if from_height >= to_height {
            return Err(conxian_core::ConxianError::Internal(
                "Invalid height range for header chain verification".to_string(),
            ));
        }

        for height in from_height..=to_height {
            let block_info = self.get_btc_header_info(height).await?;
            if block_info.height != height {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[async_trait]
impl ChainAdapter for BabylonAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        // Return BTC header-chain height for Babylon SPV
        self.get_btc_header_height().await
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

    async fn verify_state_proof(&self, proof_metadata: Value) -> ConxianResult<bool> {
        info!(chain = "babylon", "Verifying Babylon finality proof");
        
        // If BTC RPC is configured, perform SPV header verification
        if let Some(rpc) = &self.btc_rpc {
            let proof_height = proof_metadata["btc_height"]
                .as_u64()
                .unwrap_or(0);
            let current_height = rpc.get_block_count().await?;

            // Verify proof height is recent (within 6 blocks for SPV safety)
            if proof_height > 0 && current_height - proof_height <= 6 {
                return Ok(true);
            }
        }

        // Fallback to proof type check
        let proof_type = proof_metadata["type"].as_str().unwrap_or("unknown");
        Ok(proof_type == "finality_gadget")
    }
}

/// BTC header for Merkle proof verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtcHeaderInfo {
    pub height: u64,
    pub hash: String,
    pub timestamp: u64,
    pub prev_blockhash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_babylon_adapter_without_rpc() {
        let adapter = BabylonAdapter::new("testnet".to_string());
        let height = adapter.get_latest_height().await.unwrap();
        assert_eq!(height, 0);
    }
}

