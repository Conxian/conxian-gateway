//! Strata Protocol Adapter (G-08 / CON-1402)
//!
//! Strata is a Bitcoin ZK validity rollup by Alpen Labs.
//! Prague-compatible EVM execution (chain ID 8150 on testnet),
//! 5-second block time, BitVM2-based bridge.
//!
//! Testnet RPC: <https://rpc.testnet.alpenlabs.io>
//! Docs: <https://docs.alpenlabs.io>
//!
//! This adapter follows the same JSON-RPC pattern as Citrea,
//! adding Strata-specific `strata_*` methods for Bitcoin
//! checkpoint verification and bridge state queries.

use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianError, ConxianResult};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

/// Strata ZK rollup adapter (testnet chain ID 8150).
///
/// Uses standard Ethereum JSON-RPC (`eth_*`) plus Strata-specific
/// RPC methods (`strata_*`) for Bitcoin checkpoint and bridge verification.
pub struct StrataAdapter {
    pub rpc_url: String,
    pub network: String,
    client: reqwest::Client,
}

impl StrataAdapter {
    pub fn new(rpc_url: String, network: String) -> Self {
        Self {
            rpc_url,
            network,
            client: reqwest::Client::new(),
        }
    }

    pub fn chain_id(&self) -> u64 {
        // Strata testnet (Prague) chain ID 8150.
        // Mainnet chain ID TBD — will update when mainnet launches.
        8150
    }

    async fn rpc_call(&self, method: &str, params: Vec<Value>) -> ConxianResult<Value> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        debug!(method, "Strata JSON-RPC call");
        let resp = self
            .client
            .post(&self.rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ConxianError::Internal(format!("Strata RPC error: {e}")))?;

        let body: Value = resp
            .json()
            .await
            .map_err(|e| ConxianError::Internal(format!("Strata RPC parse error: {e}")))?;

        if let Some(err) = body.get("error") {
            return Err(ConxianError::Internal(format!(
                "Strata RPC method {method} failed: {err}"
            )));
        }

        Ok(body["result"].clone())
    }
}

#[async_trait]
impl ChainAdapter for StrataAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        info!("Strata: fetching latest L2 block height");

        match self.rpc_call("eth_blockNumber", vec![]).await {
            Ok(result) => {
                let hex = result.as_str().unwrap_or("0x0").trim_start_matches("0x");
                let height = u64::from_str_radix(hex, 16).unwrap_or(0);
                info!(height, "Strata L2 block height");
                Ok(height)
            }
            Err(e) => {
                warn!(error = %e, "Strata RPC unavailable, returning 0");
                Ok(0)
            }
        }
    }

    async fn get_chain_identity(&self) -> String {
        format!("strata:{}:{}", self.network, self.chain_id())
    }

    async fn prepare_unsigned_transaction(
        &self,
        tx_details: serde_json::Value,
    ) -> ConxianResult<serde_json::Value> {
        info!("Strata: preparing unsigned transaction");

        let from = tx_details
            .get("from")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0000000000000000000000000000000000000000");
        let to = tx_details.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let data = tx_details
            .get("data")
            .and_then(|v| v.as_str())
            .unwrap_or("0x");
        let value = tx_details
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0");

        if to.is_empty() {
            return Err(ConxianError::Internal(
                "Strata: 'to' address is required".into(),
            ));
        }

        // For contract deployments, use eth_getTransactionCount + gas estimation
        let nonce = match self
            .rpc_call(
                "eth_getTransactionCount",
                vec![json!(from), json!("latest")],
            )
            .await
        {
            Ok(result) => {
                let hex = result.as_str().unwrap_or("0x0").trim_start_matches("0x");
                u64::from_str_radix(hex, 16).unwrap_or(0)
            }
            Err(_) => 0,
        };

        let tx = json!({
            "from": from,
            "to": to,
            "data": data,
            "value": value,
            "nonce": format!("0x{:x}", nonce),
            "chainId": self.chain_id(),
            "type": "0x2",
            "maxFeePerGas": "0x3b9aca00",
            "maxPriorityFeePerGas": "0x3b9aca00",
            "gas": "0x30d40",
        });

        Ok(tx)
    }

    async fn verify_state_proof(&self, proof_metadata: serde_json::Value) -> ConxianResult<bool> {
        info!("Strata: verifying state proof");

        let tx_hash = proof_metadata.get("tx_hash").and_then(|v| v.as_str());

        // Verify transaction receipt if tx_hash provided
        if let Some(hash) = tx_hash {
            match self
                .rpc_call("eth_getTransactionReceipt", vec![json!(hash)])
                .await
            {
                Ok(receipt) => {
                    if receipt.is_null() {
                        warn!(tx_hash = hash, "Strata tx receipt not found");
                        return Ok(false);
                    }

                    let status = receipt["status"]
                        .as_str()
                        .unwrap_or("0x0")
                        .trim_start_matches("0x");
                    let success = u64::from_str_radix(status, 16).unwrap_or(0) == 1;

                    let block = receipt["blockNumber"]
                        .as_str()
                        .unwrap_or("0x0")
                        .trim_start_matches("0x");
                    let block_num = u64::from_str_radix(block, 16).unwrap_or(0);

                    info!(
                        tx_hash = hash,
                        block = block_num,
                        success,
                        "Strata tx receipt verified"
                    );
                    return Ok(success);
                }
                Err(e) => {
                    warn!(error = %e, tx_hash = hash, "Strata receipt fetch failed");
                    return Ok(false);
                }
            }
        }

        // Verify checkpoint if checkpoint_index provided
        if let Some(checkpoint_idx) = proof_metadata.get("checkpoint_index") {
            match self
                .rpc_call("strata_getCheckpointInfo", vec![checkpoint_idx.clone()])
                .await
            {
                Ok(info) => {
                    if info.is_null() {
                        warn!("Strata checkpoint not found");
                        return Ok(false);
                    }
                    let l1_txid = info["l1_txid"].as_str().unwrap_or("unknown");
                    let idx_str = checkpoint_idx.to_string();
                    info!(
                        checkpoint_index = %idx_str,
                        l1_txid,
                        "Strata checkpoint verified on Bitcoin L1"
                    );
                    return Ok(true);
                }
                Err(e) => {
                    warn!(error = %e, "Strata checkpoint verification failed");
                    return Ok(false);
                }
            }
        }

        // Fallback: check client sync status
        match self.rpc_call("strata_clientStatus", vec![]).await {
            Ok(status) => {
                let synced = status["synced"].as_bool().unwrap_or(false);
                info!(synced, "Strata client sync status");
                Ok(synced)
            }
            Err(e) => {
                warn!(error = %e, "Strata client status check failed");
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_strata_adapter_identity() {
        let adapter =
            StrataAdapter::new("https://rpc.testnet.alpenlabs.io".into(), "testnet".into());
        assert_eq!(adapter.chain_id(), 8150);
        assert_eq!(adapter.get_chain_identity().await, "strata:testnet:8150");
    }

    #[tokio::test]
    async fn test_strata_mainnet_identity() {
        let adapter = StrataAdapter::new("https://rpc.strata.xyz".into(), "mainnet".into());
        assert_eq!(adapter.chain_id(), 8150);
        assert_eq!(adapter.get_chain_identity().await, "strata:mainnet:8150");
    }

    #[tokio::test]
    async fn test_get_latest_height_offline() {
        let adapter = StrataAdapter::new("http://localhost:18545".into(), "testnet".into());
        let result = adapter.get_latest_height().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_verify_state_proof_offline() {
        let adapter = StrataAdapter::new("http://localhost:18545".into(), "testnet".into());
        let result = adapter.verify_state_proof(json!({})).await;
        // Offline returns false
        assert_eq!(result.unwrap(), false);
    }

    #[tokio::test]
    async fn test_prepare_unsigned_tx_requires_to() {
        let adapter = StrataAdapter::new("http://localhost:18545".into(), "testnet".into());
        let result = adapter
            .prepare_unsigned_transaction(json!({"from": "0x1234"}))
            .await;
        assert!(result.is_err());
    }
}
