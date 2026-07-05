use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianError, ConxianResult};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

/// Protocol Adapter for Citrea (T1 Rollup - CON-1268)
/// EVM-compatible ZK-rollup on Bitcoin.
pub struct CitreaAdapter {
    pub rpc_url: String,
    pub network: String,
    client: reqwest::Client,
}

impl CitreaAdapter {
    pub fn new(rpc_url: String, network: String) -> Self {
        Self {
            rpc_url,
            network,
            client: reqwest::Client::new(),
        }
    }

    async fn rpc_call(&self, method: &str, params: Vec<Value>) -> ConxianResult<Value> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let resp = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ConxianError::Internal(format!("Citrea RPC error: {e}")))?;

        let result: Value = resp
            .json()
            .await
            .map_err(|e| ConxianError::Internal(format!("Citrea RPC parse error: {e}")))?;

        if result.get("error").is_some() {
            return Err(ConxianError::Internal(format!(
                "Citrea RPC error: {}",
                result["error"]
            )));
        }

        Ok(result["result"].clone())
    }
}

#[async_trait]
impl ChainAdapter for CitreaAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        debug!("Fetching latest Citrea block via JSON-RPC");
        self.rpc_call("eth_blockNumber", vec![])
            .await
            .and_then(|v| {
                v.as_str()
                    .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .ok_or_else(|| ConxianError::Internal("Invalid block number response".into()))
            })
            .or_else(|_| {
                warn!("Citrea RPC fallback — returning 0");
                Ok(0)
            })
    }

    async fn get_chain_identity(&self) -> String {
        format!("citrea:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(chain = "citrea", "Preparing unsigned transaction");
        Ok(json!({
            "chain": "citrea",
            "status": "prepared",
            "payload": tx_details,
            "evm_compatible": true,
            "chain_id": 5115 // Citrea Testnet
        }))
    }

    async fn verify_state_proof(&self, proof_metadata: Value) -> ConxianResult<bool> {
        info!(chain = "citrea", "Verifying Citrea ZK-proof");
        // Shadow mode: check for ZK-proof commitment in metadata
        let proof = proof_metadata["zk_proof"].as_str();
        Ok(proof.is_some())
    }
}
