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
pub trait StacksRpc: conxian_core::SimulatedStacksRpcTrait + Send + Sync {
    async fn get_block_count(&self) -> ConxianResult<u64>;
    async fn get_network_info(&self) -> ConxianResult<StacksNetworkInfo>;
}

pub struct SimulatedStacksRpc {
    pub initial_height: u64,
}

#[async_trait]
impl conxian_core::SimulatedStacksRpcTrait for SimulatedStacksRpc {
    async fn call_read_only(
        &self,
        _contract: &str,
        _function: &str,
        _args: Vec<serde_json::Value>,
    ) -> ConxianResult<serde_json::Value> {
        Ok(serde_json::json!({ "okay": true, "result": "simulated" }))
    }
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

#[derive(Clone)]
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
impl conxian_core::SimulatedStacksRpcTrait for StacksRpcClient {
    async fn call_read_only(
        &self,
        contract: &str,
        function: &str,
        args: Vec<serde_json::Value>,
    ) -> ConxianResult<serde_json::Value> {
        let (contract_address, contract_name) = contract
            .split_once('.')
            .ok_or_else(|| ConxianError::Stacks("Invalid contract format".into()))?;
        let url = format!(
            "{}/v2/contracts/call-read/{}/{}/{}",
            self.url, contract_address, contract_name, function
        );

        let payload = serde_json::json!({
            "sender": "SP000000000000000000002Q6VF78", // Burn address for read-only calls
            "arguments": args
        });

        tokio::task::spawn_blocking(move || {
            let res = minreq::post(&url)
                .with_json(&payload)
                .map_err(|e| ConxianError::Stacks(e.to_string()))?
                .send()
                .map_err(|e| ConxianError::Stacks(e.to_string()))?;

            if res.status_code != 200 {
                return Err(ConxianError::Stacks(format!(
                    "Stacks read-only call error: status {}",
                    res.status_code
                )));
            }

            let body = res
                .as_str()
                .map_err(|e| ConxianError::Stacks(e.to_string()))?;
            serde_json::from_str(body).map_err(|e| ConxianError::Stacks(e.to_string()))
        })
        .await
        .map_err(|e| ConxianError::Internal(e.to_string()))?
    }
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
