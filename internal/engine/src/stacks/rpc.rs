use async_trait::async_trait;
use conxian_core::{ConxianError, ConxianResult};
use serde::Deserialize;

#[async_trait]
pub trait StacksRpc: Send + Sync {
    async fn get_block_count(&self) -> ConxianResult<u64>;
}

pub struct SimulatedStacksRpc {
    pub initial_height: u64,
}

#[async_trait]
impl StacksRpc for SimulatedStacksRpc {
    async fn get_block_count(&self) -> ConxianResult<u64> {
        Ok(self.initial_height)
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
}

#[async_trait]
impl StacksRpc for StacksRpcClient {
    async fn get_block_count(&self) -> ConxianResult<u64> {
        let url = format!("{}/v2/info", self.url);

        tokio::task::spawn_blocking(move || {
            let res = minreq::get(&url).send()
                .map_err(|e| ConxianError::Stacks(e.to_string()))?;

            if res.status_code != 200 {
                return Err(ConxianError::Stacks(format!("Stacks RPC error: status {}", res.status_code)));
            }

            let body = res.as_str()
                .map_err(|e| ConxianError::Stacks(e.to_string()))?;

            let info: StacksInfo = serde_json::from_str(body)
                .map_err(|e| ConxianError::Stacks(e.to_string()))?;

            Ok(info.stacks_tip_height)
        })
        .await
        .map_err(|e| ConxianError::Internal(e.to_string()))?
    }
}
