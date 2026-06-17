use async_trait::async_trait;
use conxian_core::{AttestationRequest, ConxianResult};
use serde_json::Value;
use std::sync::Arc;

#[async_trait]
pub trait CoreVerifier: Send + Sync {
    async fn verify_attestation_v2(&self, request: &AttestationRequest) -> ConxianResult<bool>;
}

pub struct UniversalVerifier {
    core: Arc<dyn CoreVerifier>,
    adapters: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>>,
}

impl UniversalVerifier {
    pub fn new(
        core: Arc<dyn CoreVerifier>,
        adapters: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>>,
    ) -> Self {
        Self { core, adapters }
    }

    pub async fn verify_attestation(&self, request: &AttestationRequest) -> ConxianResult<bool> {
        self.core.verify_attestation_v2(request).await
    }

    pub async fn verify_state_proof(
        &self,
        chain: &str,
        proof_metadata: Value,
    ) -> ConxianResult<bool> {
        if let Some(adapter) = self.adapters.get(chain) {
            adapter.verify_state_proof(proof_metadata).await
        } else {
            Err(conxian_core::ConxianError::Compliance(format!(
                "No adapter found for chain: {}",
                chain
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::ConxianResult;

    struct MockVerifier;
    #[async_trait]
    impl CoreVerifier for MockVerifier {
        async fn verify_attestation_v2(
            &self,
            _request: &AttestationRequest,
        ) -> ConxianResult<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_universal_verifier_delegation() {
        let adapters: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>> =
            std::collections::HashMap::new();
        let verifier = UniversalVerifier::new(Arc::new(MockVerifier), adapters);

        let result = verifier
            .verify_state_proof("unknown", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }
}
