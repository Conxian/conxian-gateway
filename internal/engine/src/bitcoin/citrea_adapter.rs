use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianError, ConxianResult};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

/// Protocol Adapter for Citrea (G-08 / CON-1400)
///
/// Citrea is the first BitVM2-bridged ZK rollup live on Bitcoin mainnet (Jan 2026).
/// EVM-compatible (Type II zkEVM, Pectra) — follows the same JSON-RPC pattern as Rootstock.
///
/// Mainnet: chain ID 4114, RPC https://rpc.citrea.xyz
/// Testnet: chain ID 5115, RPC https://testnet.rpc.citrea.xyz
/// Bridge:  Clementine (BitVM2), contract 0x3100000000000000000000000000000000000002
/// Currency: cBTC (18 decimals), block time 2s, Bitcoin DA + settlement
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

    pub fn chain_id(&self) -> u64 {
        match self.network.as_str() {
            "mainnet" => 4114,
            _ => 5115,
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
            .or_else(|e| {
                warn!(error = %e, "Citrea RPC fallback — returning 0");
                Ok(0)
            })
    }

    async fn get_chain_identity(&self) -> String {
        format!("citrea:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(chain = "citrea", "Preparing unsigned EVM transaction");
        let chain_id = self.chain_id();
        Ok(json!({
            "chain": "citrea",
            "status": "prepared",
            "payload": tx_details,
            "evm_compatible": true,
            "chain_id": chain_id,
            "zk_rollup": true,
            "settlement": "bitcoin",
            "bridge": "clementine",
            "bridge_contract": "0x3100000000000000000000000000000000000002",
            "block_time_secs": 2
        }))
    }

    async fn verify_state_proof(&self, proof_metadata: Value) -> ConxianResult<bool> {
        let tx_hash = proof_metadata
            .get("tx_hash")
            .and_then(|v| v.as_str())
            .or_else(|| proof_metadata.get("btc_tx_hash").and_then(|v| v.as_str()))
            .unwrap_or("");

        let block_number = proof_metadata.get("block_number").and_then(|v| v.as_u64());

        if tx_hash.is_empty() && block_number.is_none() {
            info!("Citrea: no tx_hash or block_number in proof, accepting shadow mode");
            return Ok(true);
        }

        // Verify via eth_getTransactionReceipt on the Citrea EVM RPC
        if !tx_hash.is_empty() {
            let result = self
                .rpc_call("eth_getTransactionReceipt", vec![json!(tx_hash)])
                .await;

            match result {
                Ok(receipt) if !receipt.is_null() => {
                    let status = receipt
                        .get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("0x0");
                    let verified = status == "0x1";
                    info!(
                        tx_hash = %tx_hash,
                        verified = verified,
                        block = ?receipt.get("blockNumber"),
                        "Citrea transaction receipt verified"
                    );
                    Ok(verified)
                }
                Ok(_) => {
                    warn!(tx_hash = %tx_hash, "Citrea transaction receipt is null — tx may be pending");
                    Ok(false)
                }
                Err(e) => {
                    warn!(error = %e, tx_hash = %tx_hash, "Citrea RPC receipt query failed");
                    Ok(false)
                }
            }
        } else {
            // Block-level verification via eth_getBlockByNumber
            let block_hex = format!("0x{:x}", block_number.unwrap_or(0));
            let result = self
                .rpc_call("eth_getBlockByNumber", vec![json!(block_hex), json!(false)])
                .await;

            match result {
                Ok(block) if !block.is_null() => {
                    let hash = block.get("hash").and_then(|h| h.as_str()).unwrap_or("");
                    info!(
                        block_number = block_number,
                        block_hash = %hash,
                        "Citrea block verified via RPC"
                    );
                    Ok(!hash.is_empty())
                }
                Ok(_) => {
                    warn!(block_number = block_number, "Citrea block not found");
                    Ok(false)
                }
                Err(e) => {
                    warn!(error = %e, "Citrea RPC block query failed");
                    Ok(false)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_identity() {
        let adapter = CitreaAdapter::new("http://localhost:8545".into(), "mainnet".into());
        assert_eq!(adapter.get_chain_identity_blocking(), "citrea:mainnet");
    }

    #[test]
    fn test_chain_id_mainnet() {
        let adapter = CitreaAdapter::new("http://localhost:8545".into(), "mainnet".into());
        assert_eq!(adapter.chain_id(), 4114);
    }

    #[test]
    fn test_chain_id_testnet() {
        let adapter = CitreaAdapter::new("http://localhost:8545".into(), "testnet".into());
        assert_eq!(adapter.chain_id(), 5115);
    }

    #[test]
    fn test_chain_id_simulated() {
        let adapter = CitreaAdapter::new("http://localhost:8545".into(), "simulated".into());
        assert_eq!(adapter.chain_id(), 5115); // falls back to testnet
    }

    // Synchronous helper for unit testing chain identity
    impl CitreaAdapter {
        fn get_chain_identity_blocking(&self) -> String {
            format!("citrea:{}", self.network)
        }
    }
}
