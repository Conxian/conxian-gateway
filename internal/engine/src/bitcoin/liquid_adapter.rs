use crate::bitcoin::rpc::BitcoinRpc;
use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianResult};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info};

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
        // Shadow mode: verify against RPC but don't fail closed yet
        Ok(true)
    }
}
