use async_trait::async_trait;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use conxian_core::{BlockInfo, ConxianError, ConxianResult};
use serde_json::{json, Value};
use std::sync::Arc;

#[async_trait]
pub trait BitcoinRpc: Send + Sync {
    async fn get_block_count(&self) -> ConxianResult<u64>;
    async fn get_block_info(&self, height: u64) -> ConxianResult<BlockInfo>;
    async fn get_network_info(&self) -> ConxianResult<String>;

    async fn submit_rbf_replacement(
        &self,
        _txid: &str,
        _target_fee_rate_sat_vb: u64,
    ) -> ConxianResult<Option<String>> {
        Ok(None)
    }

    async fn submit_cpfp_child(
        &self,
        _parent_txid: &str,
        _target_fee_rate_sat_vb: u64,
    ) -> ConxianResult<Option<String>> {
        Ok(None)
    }
}

pub struct BitcoinRpcClient {
    pub(super) client: Arc<Client>,
}

impl BitcoinRpcClient {
    pub fn new(url: &str, user: &str, pass: &str) -> ConxianResult<Self> {
        let url = url.trim();
        let user = user.trim();

        if url.is_empty() {
            return Err(ConxianError::Bitcoin(
                "Invalid Bitcoin RPC URL: URL is empty".to_string(),
            ));
        }

        let auth = match (user.is_empty(), pass.is_empty()) {
            (true, true) => Auth::None,
            (false, false) => Auth::UserPass(user.to_string(), pass.to_string()),
            (true, false) => {
                return Err(ConxianError::Bitcoin(
                    "Invalid Bitcoin RPC auth: password set but username is empty".to_string(),
                ));
            }
            (false, true) => {
                return Err(ConxianError::Bitcoin(
                    "Invalid Bitcoin RPC auth: username set but password is empty".to_string(),
                ));
            }
        };
        let client = Client::new(url, auth)
            .map_err(|e: bitcoincore_rpc::Error| ConxianError::Bitcoin(e.to_string()))?;
        Ok(Self {
            client: Arc::new(client),
        })
    }
}

#[async_trait]
impl BitcoinRpc for BitcoinRpcClient {
    async fn get_block_count(&self) -> ConxianResult<u64> {
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || {
            client
                .get_block_count()
                .map_err(|e: bitcoincore_rpc::Error| ConxianError::Bitcoin(e.to_string()))
        })
        .await
        .map_err(|e: tokio::task::JoinError| ConxianError::Internal(e.to_string()))?
    }

    async fn get_block_info(&self, height: u64) -> ConxianResult<BlockInfo> {
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || {
            let hash = client
                .get_block_hash(height)
                .map_err(|e: bitcoincore_rpc::Error| ConxianError::Bitcoin(e.to_string()))?;
            let header = client
                .get_block_header(&hash)
                .map_err(|e: bitcoincore_rpc::Error| ConxianError::Bitcoin(e.to_string()))?;

            Ok(BlockInfo {
                hash: hash.to_string(),
                height,
                timestamp: header.time as u64,
            })
        })
        .await
        .map_err(|e: tokio::task::JoinError| ConxianError::Internal(e.to_string()))?
    }

    async fn get_network_info(&self) -> ConxianResult<String> {
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || {
            let info = client
                .get_blockchain_info()
                .map_err(|e: bitcoincore_rpc::Error| ConxianError::Bitcoin(e.to_string()))?;
            Ok(info.chain.to_string())
        })
        .await
        .map_err(|e: tokio::task::JoinError| ConxianError::Internal(e.to_string()))?
    }

    async fn submit_rbf_replacement(
        &self,
        txid: &str,
        target_fee_rate_sat_vb: u64,
    ) -> ConxianResult<Option<String>> {
        let txid = txid.trim().to_string();
        if txid.is_empty() {
            return Err(ConxianError::Bitcoin(
                "Cannot bump fee: txid is empty".to_string(),
            ));
        }

        if target_fee_rate_sat_vb == 0 {
            return Err(ConxianError::Bitcoin(format!(
                "Cannot bump fee for tx {}: target fee rate must be > 0 sat/vB",
                txid
            )));
        }

        let client = self.client.clone();
        tokio::task::spawn_blocking(move || {
            let params = vec![
                json!(txid.clone()),
                json!({
                    "fee_rate": target_fee_rate_sat_vb,
                }),
            ];

            let response: Value =
                client
                    .call("bumpfee", &params)
                    .map_err(|e: bitcoincore_rpc::Error| {
                        ConxianError::Bitcoin(format!(
                            "Bitcoin Core bumpfee failed for tx {} at {} sat/vB: {}",
                            txid, target_fee_rate_sat_vb, e
                        ))
                    })?;

            let replacement_txid = response
                .get("txid")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ConxianError::Bitcoin(format!(
                        "Bitcoin Core bumpfee returned no replacement txid for tx {}",
                        txid
                    ))
                })?
                .to_string();

            Ok(Some(replacement_txid))
        })
        .await
        .map_err(|e: tokio::task::JoinError| ConxianError::Internal(e.to_string()))?
    }

    async fn submit_cpfp_child(
        &self,
        parent_txid: &str,
        target_fee_rate_sat_vb: u64,
    ) -> ConxianResult<Option<String>> {
        let parent_txid = parent_txid.trim();
        if parent_txid.is_empty() {
            return Err(ConxianError::Bitcoin(
                "CPFP child submission is not supported: parent txid is empty".to_string(),
            ));
        }

        Err(ConxianError::Bitcoin(format!(
            "CPFP child submission is not supported for parent tx {} at {} sat/vB: this adapter cannot safely construct and sign child transactions without wallet UTXO and key context",
            parent_txid,
            target_fee_rate_sat_vb
        )))
    }
}
