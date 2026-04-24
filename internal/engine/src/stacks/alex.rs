use crate::stacks::rpc::StacksRpc;
use async_trait::async_trait;
pub use conxian_core::AlexSwapRequest;
use conxian_core::{ConxianError, ConxianResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

/// ALEX DEX Pair information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlexPair {
    pub token_x: String,
    pub token_y: String,
    pub contract_id: String,
}

/// Client for interacting with the ALEX Protocol on Stacks.
#[async_trait]
pub trait AlexClient: Send + Sync {
    async fn get_swap_quote(&self, request: AlexSwapRequest) -> ConxianResult<u128>;
    async fn execute_swap(
        &self,
        request: AlexSwapRequest,
        signer_key: &str,
    ) -> ConxianResult<String>;
    async fn build_swap_payload(
        &self,
        request: AlexSwapRequest,
    ) -> ConxianResult<serde_json::Value>;
}

/// Production implementation of the AlexClient.
/// Interaction path: Direct Contract Calls via Stacks RPC.
pub struct AlexRpcClient {
    pub rpc: Box<dyn StacksRpc>,
    pub alex_api_url: String,
}

impl AlexRpcClient {
    pub fn new(rpc: Box<dyn StacksRpc>, alex_api_url: &str) -> Self {
        Self {
            rpc,
            alex_api_url: alex_api_url.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct AlexQuoteResponse {
    #[serde(rename = "swap-amount-out")]
    swap_amount_out: u128,
}

#[async_trait]
impl AlexClient for AlexRpcClient {
    async fn get_swap_quote(&self, request: AlexSwapRequest) -> ConxianResult<u128> {
        let url = format!(
            "{}/v1/quote?token-x={}&token-y={}&amount={}",
            self.alex_api_url, request.token_x, request.token_y, request.amount
        );

        tokio::task::spawn_blocking(move || {
            let res = minreq::get(&url)
                .send()
                .map_err(|e| ConxianError::Stacks(format!("ALEX API error: {}", e)))?;

            if res.status_code != 200 {
                return Err(ConxianError::Stacks(format!(
                    "ALEX API returned status {}",
                    res.status_code
                )));
            }

            let body = res
                .as_str()
                .map_err(|e| ConxianError::Stacks(e.to_string()))?;

            let quote: AlexQuoteResponse =
                serde_json::from_str(body).map_err(|e| ConxianError::Stacks(e.to_string()))?;

            Ok(quote.swap_amount_out)
        })
        .await
        .map_err(|e| ConxianError::Internal(e.to_string()))?
    }

    async fn build_swap_payload(
        &self,
        request: AlexSwapRequest,
    ) -> ConxianResult<serde_json::Value> {
        Ok(json!({
            "contract_address": "SP3K8BC0PPEVCV7NZ6QSRWPQ2JE9E5B6N3PA0XBHT",
            "contract_name": "alex-swap-helper-v1",
            "function_name": "swap-helper",
            "function_args": [
                { "type": "principal", "value": request.token_x },
                { "type": "principal", "value": request.token_y },
                { "type": "uint", "value": request.amount.to_string() },
                { "type": "uint", "value": request.min_dy.unwrap_or(1).to_string() }
            ]
        }))
    }

    async fn execute_swap(
        &self,
        request: AlexSwapRequest,
        _signer_key: &str,
    ) -> ConxianResult<String> {
        info!(
            "Preparing ALEX swap: {} {} -> {}",
            request.amount, request.token_x, request.token_y
        );

        let _payload = self.build_swap_payload(request).await?;

        // Current status: logic is structured but requires secure signer integration
        // for transaction signing and broadcasting to Stacks mainnet.
        warn!("ALEX swap execution structured but waiting for signer-enclave cutover");

        Err(ConxianError::Internal(
            "ALEX swap execution requires secure signer-enclave integration".to_string(),
        ))
    }
}

/// Simulated ALEX client for testing and readiness verification.
pub struct SimulatedAlexClient;

#[async_trait]
impl AlexClient for SimulatedAlexClient {
    async fn get_swap_quote(&self, _request: AlexSwapRequest) -> ConxianResult<u128> {
        Ok(100)
    }

    async fn execute_swap(
        &self,
        _request: AlexSwapRequest,
        _signer_key: &str,
    ) -> ConxianResult<String> {
        Ok("txid_alex_simulated_swap_rehearsal".to_string())
    }

    async fn build_swap_payload(
        &self,
        request: AlexSwapRequest,
    ) -> ConxianResult<serde_json::Value> {
        Ok(json!({
            "token_x": request.token_x,
            "token_y": request.token_y,
            "amount": request.amount.to_string(),
            "simulated": true
        }))
    }
}
