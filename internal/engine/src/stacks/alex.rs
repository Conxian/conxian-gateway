use crate::stacks::rpc::StacksRpc;
use async_trait::async_trait;
pub use conxian_core::AlexSwapRequest;
use conxian_core::{ConxianError, ConxianResult};
use serde::{Deserialize, Serialize};

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

    async fn execute_swap(
        &self,
        _request: AlexSwapRequest,
        _signer_key: &str,
    ) -> ConxianResult<String> {
        // Production implementation: construct Clarity contract-call (swap-helper)
        // using stacks-transactions logic and broadcast via self.rpc.
        Err(ConxianError::Internal(
            "ALEX swap execution (contract-call) requires signer integration".to_string(),
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
}
