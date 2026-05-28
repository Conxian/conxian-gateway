use async_trait::async_trait;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use conxian_core::{BlockInfo, ConxianError, ConxianResult};
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
    client: Arc<Client>,
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
}
