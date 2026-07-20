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
    use conxian_core::{ChainAdapter, ConxianError, ConxianResult};
    use std::sync::{Arc, Mutex};

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

    enum AdapterOutcome {
        Return(bool),
        Fail(String),
    }

    struct RecordingAdapter {
        calls: Arc<Mutex<Vec<Value>>>,
        outcome: AdapterOutcome,
    }

    #[async_trait]
    impl ChainAdapter for RecordingAdapter {
        async fn get_latest_height(&self) -> ConxianResult<u64> {
            Ok(0)
        }

        async fn get_chain_identity(&self) -> String {
            "test-chain".to_string()
        }

        async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
            Ok(tx_details)
        }

        async fn verify_state_proof(&self, proof_metadata: Value) -> ConxianResult<bool> {
            self.calls.lock().unwrap().push(proof_metadata);
            match &self.outcome {
                AdapterOutcome::Return(result) => Ok(*result),
                AdapterOutcome::Fail(message) => Err(ConxianError::Compliance(message.clone())),
            }
        }
    }

    fn verifier_with_adapter(
        chain: &str,
        adapter: RecordingAdapter,
    ) -> (UniversalVerifier, Arc<Mutex<Vec<Value>>>) {
        let calls = adapter.calls.clone();
        let mut adapters: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>> =
            std::collections::HashMap::new();
        adapters.insert(chain.to_string(), Arc::new(adapter));
        (
            UniversalVerifier::new(Arc::new(MockVerifier), adapters),
            calls,
        )
    }

    #[tokio::test]
    async fn delegates_exact_chain_and_payload_and_propagates_true() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (verifier, recorded_calls) = verifier_with_adapter(
            "liquid",
            RecordingAdapter {
                calls,
                outcome: AdapterOutcome::Return(true),
            },
        );
        let payload = serde_json::json!({"proof": "0xabc", "nested": {"height": 42}});

        assert!(verifier
            .verify_state_proof("liquid", payload.clone())
            .await
            .unwrap());
        assert_eq!(*recorded_calls.lock().unwrap(), vec![payload]);
    }

    #[tokio::test]
    async fn propagates_false_result_without_reinterpretation() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (verifier, recorded_calls) = verifier_with_adapter(
            "liquid",
            RecordingAdapter {
                calls,
                outcome: AdapterOutcome::Return(false),
            },
        );
        let payload = serde_json::json!({"verified": true});

        assert!(!verifier
            .verify_state_proof("liquid", payload.clone())
            .await
            .unwrap());
        assert_eq!(*recorded_calls.lock().unwrap(), vec![payload]);
    }

    #[tokio::test]
    async fn propagates_adapter_error() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (verifier, recorded_calls) = verifier_with_adapter(
            "liquid",
            RecordingAdapter {
                calls,
                outcome: AdapterOutcome::Fail("adapter unavailable".to_string()),
            },
        );
        let payload = serde_json::json!({"proof": "invalid"});

        let error = verifier
            .verify_state_proof("liquid", payload.clone())
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "Compliance error: adapter unavailable");
        assert_eq!(*recorded_calls.lock().unwrap(), vec![payload]);
    }

    #[tokio::test]
    async fn returns_unknown_chain_error_without_adapter_call() {
        let adapters: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>> =
            std::collections::HashMap::new();
        let verifier = UniversalVerifier::new(Arc::new(MockVerifier), adapters);

        let result = verifier
            .verify_state_proof("unknown", serde_json::json!({"proof": "missing"}))
            .await;
        assert_eq!(
            result.unwrap_err().to_string(),
            "Compliance error: No adapter found for chain: unknown"
        );
    }
}
