use async_trait::async_trait;
use conxian_core::{ConxianError, ConxianResult};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct StacksNetworkInfo {
    pub height: u64,
    pub network: String,
    pub epoch: String,
    pub burn_block_height: u64,
}

#[async_trait]
pub trait StacksRpc: Send + Sync {
    async fn get_block_count(&self) -> ConxianResult<u64>;
    async fn get_network_info(&self) -> ConxianResult<StacksNetworkInfo>;
}

pub struct SimulatedStacksRpc {
    pub initial_height: u64,
}

#[async_trait]
impl StacksRpc for SimulatedStacksRpc {
    async fn get_block_count(&self) -> ConxianResult<u64> {
        Ok(self.initial_height)
    }

    async fn get_network_info(&self) -> ConxianResult<StacksNetworkInfo> {
        Ok(StacksNetworkInfo {
            height: self.initial_height,
            network: "simulated".to_string(),
            epoch: "3.0".to_string(),
            burn_block_height: self.initial_height / 10,
        })
    }
}

pub struct StacksRpcClient {
    url: String,
}

impl StacksRpcClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct StacksInfo {
    stacks_tip_height: u64,
    mode: String,
    stacks_tip_epoch: String,
    burn_block_height: u64,
}

#[async_trait]
impl StacksRpc for StacksRpcClient {
    async fn get_block_count(&self) -> ConxianResult<u64> {
        self.get_network_info().await.map(|info| info.height)
    }

    async fn get_network_info(&self) -> ConxianResult<StacksNetworkInfo> {
        let url = format!("{}/v2/info", self.url);

        tokio::task::spawn_blocking(move || {
            let res = minreq::get(&url)
                .send()
                .map_err(|e| ConxianError::Stacks(e.to_string()))?;

            if res.status_code != 200 {
                return Err(ConxianError::Stacks(format!(
                    "Stacks RPC error: status {}",
                    res.status_code
                )));
            }

            let body = res
                .as_str()
                .map_err(|e| ConxianError::Stacks(e.to_string()))?;

            let info: StacksInfo =
                serde_json::from_str(body).map_err(|e| ConxianError::Stacks(e.to_string()))?;

            Ok(StacksNetworkInfo {
                height: info.stacks_tip_height,
                network: info.mode,
                epoch: info.stacks_tip_epoch,
                burn_block_height: info.burn_block_height,
            })
        })
        .await
        .map_err(|e| ConxianError::Internal(e.to_string()))?
    }
}
