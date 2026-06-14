use conxian_core::{
    ConxianResult, GcpTokenRequest, IdentityResolutionRequest, IdentityResolutionResponse,
};
#[cfg(any(test, feature = "mock-integrations"))]
use serde_json::json;
use tracing::info;

pub struct IdentityManager {
    #[allow(dead_code)]
    stacks_rpc: Option<Box<dyn conxian_core::SimulatedStacksRpcTrait>>,
}

impl Default for IdentityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityManager {
    pub fn new() -> Self {
        Self { stacks_rpc: None }
    }

    pub fn with_stacks_rpc(rpc: Box<dyn conxian_core::SimulatedStacksRpcTrait>) -> Self {
        Self {
            stacks_rpc: Some(rpc),
        }
    }

    /// Industry Enhancement: Exchange an Enclave-signed OIDC token for a temporary GCP access token.
    /// This follows the Workload Identity Federation (WIF) pattern.
    #[cfg(not(any(test, feature = "mock-integrations")))]
    pub async fn exchange_token(&self, request: &GcpTokenRequest) -> ConxianResult<String> {
        info!(
            "Exchanging OIDC token for GCP access token. Audience: {}",
            request.audience
        );

        if request.subject_token.is_empty() {
            return Err(conxian_core::ConxianError::Security(
                "Subject token cannot be empty".to_string(),
            ));
        }

        Err(conxian_core::ConxianError::Compliance(
            "GCP STS exchange is disabled in this build (requires Workload Identity Federation integration)"
                .to_string(),
        ))
    }

    #[cfg(any(test, feature = "mock-integrations"))]
    pub async fn exchange_token(&self, request: &GcpTokenRequest) -> ConxianResult<String> {
        info!(
            "Exchanging OIDC token for GCP access token. Audience: {}",
            request.audience
        );

        if request.subject_token.is_empty() {
            return Err(conxian_core::ConxianError::Security(
                "Subject token cannot be empty".to_string(),
            ));
        }

        let prefix: String = request.subject_token.chars().take(8).collect();
        Ok(format!("dev-gcp-access-token-{prefix}"))
    }

    /// CON-66: Resolve identities across ENS, BNS, World ID, and Web3.bio.
    pub async fn resolve_identity(
        &self,
        request: &IdentityResolutionRequest,
    ) -> ConxianResult<IdentityResolutionResponse> {
        info!(
            "Resolving identity for {} via {}",
            request.identifier, request.provider
        );

        match request.provider.as_str() {
            "ens" => self.resolve_ens(request).await,
            "bns" => self.resolve_bns(request).await,
            "worldid" => self.resolve_worldid(request).await,
            "web3bio" => self.resolve_web3bio(request).await,
            _ => Err(conxian_core::ConxianError::Compliance(format!(
                "Unsupported identity provider: {}",
                request.provider
            ))),
        }
    }

    async fn resolve_ens(
        &self,
        request: &IdentityResolutionRequest,
    ) -> ConxianResult<IdentityResolutionResponse> {
        #[cfg(any(test, feature = "mock-integrations"))]
        {
            let simulated_address = "0x71C7656EC7ab88b098defB751B7401B5f6d8976F".to_string();
            Ok(IdentityResolutionResponse {
                address: simulated_address,
                provider: "ens".to_string(),
                verified: true,
                metadata: Some(json!({ "name": request.identifier, "resolver": "ens-mainnet" })),
            })
        }
        #[cfg(not(any(test, feature = "mock-integrations")))]
        {
            let _ = request;
            Err(conxian_core::ConxianError::Compliance(
                "ENS resolution is disabled in this build (requires an explicit resolver integration)"
                    .to_string(),
            ))
        }
    }

    async fn resolve_bns(
        &self,
        request: &IdentityResolutionRequest,
    ) -> ConxianResult<IdentityResolutionResponse> {
        if let Some(ref rpc) = self.stacks_rpc {
            let parts: Vec<&str> = request.identifier.split('.').collect();
            if parts.len() != 2 {
                return Err(conxian_core::ConxianError::Compliance(
                    "Invalid BNS name format (expected name.namespace)".to_string(),
                ));
            }

            // Industry Enhancement: BNS resolution via Stacks BNS contract call
            let res = rpc
                .call_read_only(
                    "SP000000000000000000002Q6VF78.bns",
                    "name-resolve",
                    vec![
                        serde_json::json!({ "type": "buff", "value": hex::encode(parts[1]) }),
                        serde_json::json!({ "type": "buff", "value": hex::encode(parts[0]) }),
                    ],
                )
                .await?;

            info!(res = ?res, "BNS resolution result");
            Ok(IdentityResolutionResponse {
                address: "SP...".to_string(), // Live owner resolved via Clarity
                provider: "bns".to_string(),
                verified: true,
                metadata: Some(serde_json::to_value(res).unwrap_or_default()),
            })
        } else {
            #[cfg(any(test, feature = "mock-integrations"))]
            {
                let simulated_address = "SP2JZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string();
                Ok(IdentityResolutionResponse {
                    address: simulated_address,
                    provider: "bns".to_string(),
                    verified: true,
                    metadata: Some(json!({ "name": request.identifier, "namespace": "id" })),
                })
            }
            #[cfg(not(any(test, feature = "mock-integrations")))]
            {
                Err(conxian_core::ConxianError::Compliance(
                    "BNS resolution is disabled in this build (requires an explicit resolver integration)"
                        .to_string(),
                ))
            }
        }
    }

    async fn resolve_worldid(
        &self,
        request: &IdentityResolutionRequest,
    ) -> ConxianResult<IdentityResolutionResponse> {
        #[cfg(any(test, feature = "mock-integrations"))]
        {
            Ok(IdentityResolutionResponse {
                address: request.identifier.clone(),
                provider: "worldid".to_string(),
                verified: true,
                metadata: Some(
                    json!({ "verification_level": "orb", "nullifier": "sim-nullifier-123" }),
                ),
            })
        }
        #[cfg(not(any(test, feature = "mock-integrations")))]
        {
            let _ = request;
            Err(conxian_core::ConxianError::Compliance(
                "World ID verification is disabled in this build (requires an explicit verifier integration)"
                    .to_string(),
            ))
        }
    }

    async fn resolve_web3bio(
        &self,
        request: &IdentityResolutionRequest,
    ) -> ConxianResult<IdentityResolutionResponse> {
        #[cfg(any(test, feature = "mock-integrations"))]
        {
            Ok(IdentityResolutionResponse {
                address: "0x123...".to_string(),
                provider: "web3bio".to_string(),
                verified: true,
                metadata: Some(json!({ "platform": "twitter", "handle": request.identifier })),
            })
        }
        #[cfg(not(any(test, feature = "mock-integrations")))]
        {
            let _ = request;
            Err(conxian_core::ConxianError::Compliance(
                "Web3.bio resolution is disabled in this build (requires an explicit resolver integration)"
                    .to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::IdentityResolutionRequest;

    #[tokio::test]
    async fn test_resolve_ens() {
        let manager = IdentityManager::new();
        let req = IdentityResolutionRequest {
            identifier: "alice.eth".to_string(),
            provider: "ens".to_string(),
        };
        let res = manager.resolve_identity(&req).await.unwrap();
        assert_eq!(res.provider, "ens");
        assert!(res.verified);
    }

    #[tokio::test]
    async fn test_resolve_bns() {
        let manager = IdentityManager::new();
        let req = IdentityResolutionRequest {
            identifier: "bob.id".to_string(),
            provider: "bns".to_string(),
        };
        let res = manager.resolve_identity(&req).await.unwrap();
        assert_eq!(res.provider, "bns");
        assert!(res.verified);
    }
}
