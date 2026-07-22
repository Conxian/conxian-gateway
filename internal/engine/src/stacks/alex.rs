use crate::stacks::rpc::StacksRpc;
use async_trait::async_trait;
pub use conxian_core::AlexSwapRequest;
use conxian_core::{
    AlexNetwork, AlexPolicyRejection, AlexPrincipal, AlexSourceClass, ConxianError, ConxianResult,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

/// Compatibility path retained for observation only. The endpoint contract is
/// unverified; no replacement endpoint is inferred here.
pub const ALEX_UNVERIFIED_QUOTE_PATH: &str = "/v1/quote";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlexQuoteStatus {
    UnverifiedEndpoint,
    Fixture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlexQuoteObservation {
    pub amount_out: u128,
    pub source: AlexSourceClass,
    pub status: AlexQuoteStatus,
    pub endpoint: String,
}

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

    /// Return the quote together with its explicit source classification. The
    /// default keeps existing implementations compatible while making the
    /// legacy quote path visibly unverified to callers that opt in.
    async fn get_swap_quote_observation(
        &self,
        request: AlexSwapRequest,
    ) -> ConxianResult<AlexQuoteObservation> {
        let amount_out = self.get_swap_quote(request).await?;
        Ok(AlexQuoteObservation {
            amount_out,
            source: AlexSourceClass::Unverified,
            status: AlexQuoteStatus::UnverifiedEndpoint,
            endpoint: ALEX_UNVERIFIED_QUOTE_PATH.to_string(),
        })
    }

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
    network: Option<AlexNetwork>,
    helper: Option<AlexPrincipal>,
}

impl AlexRpcClient {
    /// Construct a read-only client with unsigned payload preparation disabled.
    /// Call `with_helper` only when an exact network-qualified helper principal
    /// has been explicitly configured and independently approved.
    pub fn new(rpc: Box<dyn StacksRpc>, alex_api_url: &str) -> Self {
        Self {
            rpc,
            alex_api_url: alex_api_url.to_string(),
            network: None,
            helper: None,
        }
    }

    pub fn new_for_network(
        rpc: Box<dyn StacksRpc>,
        alex_api_url: &str,
        network: Option<AlexNetwork>,
    ) -> Self {
        Self {
            rpc,
            alex_api_url: alex_api_url.to_string(),
            network,
            helper: None,
        }
    }

    pub fn with_helper(
        rpc: Box<dyn StacksRpc>,
        alex_api_url: &str,
        network: AlexNetwork,
        helper_principal: &str,
    ) -> ConxianResult<Self> {
        let helper = AlexPrincipal::new(network, helper_principal)
            .map_err(|error| ConxianError::Security(error.to_string()))?;
        if helper.contract_name().is_none() {
            return Err(ConxianError::Security(
                "ALEX helper configuration requires a contract principal".to_string(),
            ));
        }

        Ok(Self {
            rpc,
            alex_api_url: alex_api_url.to_string(),
            network: Some(network),
            helper: Some(helper),
        })
    }

    async fn fetch_unverified_quote(
        &self,
        request: AlexSwapRequest,
    ) -> ConxianResult<AlexQuoteObservation> {
        let url = format!(
            "{}{ALEX_UNVERIFIED_QUOTE_PATH}?token-x={}&token-y={}&amount={}",
            self.alex_api_url.trim_end_matches('/'),
            request.token_x,
            request.token_y,
            request.amount
        );

        let amount_out = tokio::task::spawn_blocking(move || {
            let res = minreq::get(&url).send().map_err(|e| {
                ConxianError::Stacks(format!(
                    "ALEX quote source is unverified; compatibility request failed: {}",
                    e
                ))
            })?;

            if res.status_code != 200 {
                return Err(ConxianError::Stacks(format!(
                    "ALEX quote source is unverified; {ALEX_UNVERIFIED_QUOTE_PATH} returned status {}",
                    res.status_code
                )));
            }

            let body = res.as_str().map_err(|e| {
                ConxianError::Stacks(format!(
                    "ALEX quote source is unverified; response body was unreadable: {}",
                    e
                ))
            })?;

            let quote: AlexQuoteResponse = serde_json::from_str(body).map_err(|e| {
                ConxianError::Stacks(format!(
                    "ALEX quote source is unverified; response schema was not verified: {}",
                    e
                ))
            })?;

            Ok(quote.swap_amount_out)
        })
        .await
        .map_err(|e| ConxianError::Internal(e.to_string()))??;

        Ok(AlexQuoteObservation {
            amount_out,
            source: AlexSourceClass::Unverified,
            status: AlexQuoteStatus::UnverifiedEndpoint,
            endpoint: ALEX_UNVERIFIED_QUOTE_PATH.to_string(),
        })
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
        Ok(self.fetch_unverified_quote(request).await?.amount_out)
    }

    async fn get_swap_quote_observation(
        &self,
        request: AlexSwapRequest,
    ) -> ConxianResult<AlexQuoteObservation> {
        self.fetch_unverified_quote(request).await
    }

    async fn build_swap_payload(
        &self,
        request: AlexSwapRequest,
    ) -> ConxianResult<serde_json::Value> {
        let network = self.network.ok_or_else(|| {
            ConxianError::Security(
                "ALEX unsigned payload preparation is disabled: a supported network is not configured"
                    .to_string(),
            )
        })?;
        let helper = self.helper.as_ref().ok_or_else(|| {
            ConxianError::Security(
                "ALEX unsigned payload preparation is disabled: configure ALEX_HELPER_PRINCIPAL with an exact network-qualified principal"
                    .to_string(),
            )
        })?;

        let token_x = AlexPrincipal::new(network, request.token_x.as_str())
            .map_err(|error| map_policy_rejection(error, "token_x"))?;
        let token_y = AlexPrincipal::new(network, request.token_y.as_str())
            .map_err(|error| map_policy_rejection(error, "token_y"))?;
        if request.amount == 0 {
            return Err(ConxianError::Security(
                "ALEX unsigned payload rejected: amount must be nonzero".to_string(),
            ));
        }
        let min_dy = request.min_dy.ok_or_else(|| {
            ConxianError::Security(
                "ALEX unsigned payload rejected: min_dy is required and has no default".to_string(),
            )
        })?;
        if min_dy == 0 {
            return Err(ConxianError::Security(
                "ALEX unsigned payload rejected: min_dy must be nonzero".to_string(),
            ));
        }

        Ok(json!({
            "network": network.as_str(),
            "contract_address": helper.address(),
            "contract_name": helper.contract_name(),
            "helper_principal": helper.value,
            "function_name": "swap-helper",
            "status": "unsigned_preparation",
            "signing": "disabled",
            "broadcast": "disabled",
            "function_args": [
                { "type": "principal", "value": token_x.value },
                { "type": "principal", "value": token_y.value },
                { "type": "uint", "value": request.amount.to_string() },
                { "type": "uint", "value": min_dy.to_string() }
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

        // Current status: this boundary never accepts a signer key, signs,
        // broadcasts, obtains a receipt, or reconciles a settlement.
        warn!("ALEX swap execution is disabled: signer and broadcast gates are closed");

        Err(ConxianError::Internal(
            "ALEX swap execution is disabled: no signer, broadcast, receipt, or reconciliation path is configured"
                .to_string(),
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

    async fn get_swap_quote_observation(
        &self,
        _request: AlexSwapRequest,
    ) -> ConxianResult<AlexQuoteObservation> {
        Ok(AlexQuoteObservation {
            amount_out: 100,
            source: AlexSourceClass::Fixture,
            status: AlexQuoteStatus::Fixture,
            endpoint: "simulated".to_string(),
        })
    }

    async fn execute_swap(
        &self,
        _request: AlexSwapRequest,
        _signer_key: &str,
    ) -> ConxianResult<String> {
        Err(ConxianError::Internal(
            "ALEX simulated client cannot produce a settlement receipt or transaction ID"
                .to_string(),
        ))
    }

    async fn build_swap_payload(
        &self,
        _request: AlexSwapRequest,
    ) -> ConxianResult<serde_json::Value> {
        Err(ConxianError::Internal(
            "ALEX simulated client cannot produce an unsigned settlement payload".to_string(),
        ))
    }
}

fn map_policy_rejection(error: AlexPolicyRejection, field: &str) -> ConxianError {
    ConxianError::Security(format!(
        "ALEX unsigned payload rejected for {field}: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stacks::rpc::SimulatedStacksRpc;

    const ASSET_IN: &str = "SP000000000000000000002Q6VF78.usdcx";
    const ASSET_OUT: &str = "SP000000000000000000002Q6VF78.sbtc";
    const HELPER: &str = "SP000000000000000000002Q6VF78.alex-helper";

    fn request(min_dy: Option<u128>) -> AlexSwapRequest {
        AlexSwapRequest {
            token_x: ASSET_IN.to_string(),
            token_y: ASSET_OUT.to_string(),
            factor: 100_000_000,
            amount: 100,
            min_dy,
        }
    }

    fn client_without_helper() -> AlexRpcClient {
        AlexRpcClient::new_for_network(
            Box::new(SimulatedStacksRpc { initial_height: 1 }),
            "http://localhost:3010",
            Some(AlexNetwork::Mainnet),
        )
    }

    fn client_with_helper() -> AlexRpcClient {
        AlexRpcClient::with_helper(
            Box::new(SimulatedStacksRpc { initial_height: 1 }),
            "http://localhost:3010",
            AlexNetwork::Mainnet,
            HELPER,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn unsigned_payload_requires_explicit_helper_configuration() {
        let error = client_without_helper()
            .build_swap_payload(request(Some(90)))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("ALEX_HELPER_PRINCIPAL"));
    }

    #[tokio::test]
    async fn unsigned_payload_rejects_missing_or_zero_min_dy() {
        let client = client_with_helper();
        let missing = client.build_swap_payload(request(None)).await.unwrap_err();
        assert!(missing.to_string().contains("min_dy is required"));

        let zero = client
            .build_swap_payload(request(Some(0)))
            .await
            .unwrap_err();
        assert!(zero.to_string().contains("min_dy must be nonzero"));
    }

    #[tokio::test]
    async fn unsigned_payload_rejects_ticker_only_assets() {
        let client = client_with_helper();
        let mut value = request(Some(90));
        value.token_x = "sBTC".to_string();
        let error = client.build_swap_payload(value).await.unwrap_err();
        assert!(error.to_string().contains("token_x"));
    }

    #[tokio::test]
    async fn unsigned_payload_uses_only_injected_helper_and_exact_assets() {
        let payload = client_with_helper()
            .build_swap_payload(request(Some(90)))
            .await
            .unwrap();
        assert_eq!(payload["helper_principal"], HELPER);
        assert_eq!(payload["contract_name"], "alex-helper");
        assert_eq!(payload["function_args"][0]["value"], ASSET_IN);
        assert_eq!(payload["function_args"][1]["value"], ASSET_OUT);
        assert_eq!(payload["function_args"][3]["value"], "90");
        assert_eq!(payload["status"], "unsigned_preparation");
        assert_eq!(payload["signing"], "disabled");
        assert_eq!(payload["broadcast"], "disabled");
    }

    #[tokio::test]
    async fn simulated_client_never_returns_receipt_or_unsigned_payload() {
        let client = SimulatedAlexClient;
        let request = request(Some(90));
        assert!(client
            .execute_swap(request.clone(), "ignored")
            .await
            .is_err());
        assert!(client.build_swap_payload(request).await.is_err());

        let observation = client
            .get_swap_quote_observation(AlexSwapRequest {
                token_x: "sBTC".to_string(),
                token_y: "STX".to_string(),
                factor: 1,
                amount: 1,
                min_dy: None,
            })
            .await
            .unwrap();
        assert_eq!(observation.source, AlexSourceClass::Fixture);
        assert_eq!(observation.status, AlexQuoteStatus::Fixture);
    }
}
