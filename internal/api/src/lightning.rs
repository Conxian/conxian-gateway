use crate::x402::X402PaymentPayload;
use async_trait::async_trait;
use axum::http::StatusCode;
use conxian_core::{FailureTaxonomy, PaymentIntent, PaymentLifecycle};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::timeout;
use tracing::{info, warn};

const DEFAULT_BACKEND_TIMEOUT: Duration = Duration::from_millis(250);
const DEFAULT_MAX_RETRIES: usize = 1;
const SUPPORTED_ASSETS: &[&str] = &["BTC", "sBTC"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightningSettlementRequest {
    pub challenge: String,
    pub amount: u128,
    pub asset: String,
    pub expiry: u64,
    pub proof_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightningSettlementResponse {
    pub settled_amount: u128,
    pub preimage: String,
    pub proof: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LightningBackendError {
    Unavailable,
    Retryable { detail: String },
    Rejected { detail: String },
    PartialFailure { detail: String },
}

#[async_trait]
pub trait LightningBackend: Send + Sync {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightningExecutionReceipt {
    pub challenge: String,
    pub settled_amount: u128,
    pub preimage: String,
    pub proof: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LightningAdapterError {
    ExpiredInvoice { expiry: u64, now: u64 },
    UnsupportedAsset { asset: String },
    ReplayDetected { challenge: String },
    AmountMismatch { expected: u128, settled: u128 },
    MissingPreimage,
    MissingProof,
    ProofMismatch,
    BackendUnavailable,
    BackendTimeout,
    BackendRejected { detail: String },
    PartialFailure { detail: String },
    ReplayStoreFailure { detail: String },
}

impl LightningAdapterError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::ExpiredInvoice { .. }
            | Self::AmountMismatch { .. }
            | Self::MissingPreimage
            | Self::MissingProof
            | Self::ProofMismatch
            | Self::BackendRejected { .. } => StatusCode::PAYMENT_REQUIRED,
            Self::UnsupportedAsset { .. } => StatusCode::BAD_REQUEST,
            Self::ReplayDetected { .. } => StatusCode::CONFLICT,
            Self::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::BackendTimeout => StatusCode::GATEWAY_TIMEOUT,
            Self::PartialFailure { .. } => StatusCode::BAD_GATEWAY,
            Self::ReplayStoreFailure { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::ExpiredInvoice { .. } => "lightning_expired_invoice",
            Self::UnsupportedAsset { .. } => "lightning_unsupported_asset",
            Self::ReplayDetected { .. } => "lightning_replay_detected",
            Self::AmountMismatch { .. } => "lightning_amount_mismatch",
            Self::MissingPreimage => "lightning_missing_preimage",
            Self::MissingProof => "lightning_missing_proof",
            Self::ProofMismatch => "lightning_proof_mismatch",
            Self::BackendUnavailable => "lightning_backend_unavailable",
            Self::BackendTimeout => "lightning_backend_timeout",
            Self::BackendRejected { .. } => "lightning_backend_rejected",
            Self::PartialFailure { .. } => "lightning_partial_failure",
            Self::ReplayStoreFailure { .. } => "lightning_replay_store_failure",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::ExpiredInvoice { expiry, now } => {
                format!("Lightning invoice expired at {expiry} (now={now})")
            }
            Self::UnsupportedAsset { asset } => {
                format!("Unsupported Lightning asset: {asset}")
            }
            Self::ReplayDetected { challenge } => {
                format!("Replay detected for challenge: {challenge}")
            }
            Self::AmountMismatch { expected, settled } => {
                format!("Settled amount mismatch: expected {expected}, got {settled}")
            }
            Self::MissingPreimage => "Backend response missing payment preimage".to_string(),
            Self::MissingProof => "Backend response missing payment proof".to_string(),
            Self::ProofMismatch => {
                "Backend proof/preimage did not match declared request proof refs".to_string()
            }
            Self::BackendUnavailable => "Lightning backend unavailable after retries".to_string(),
            Self::BackendTimeout => "Lightning backend timed out after retries".to_string(),
            Self::BackendRejected { detail } => {
                format!("Lightning backend rejected settlement: {detail}")
            }
            Self::PartialFailure { detail } => {
                format!("Lightning backend partial failure: {detail}")
            }
            Self::ReplayStoreFailure { detail } => {
                format!("Lightning replay store failure: {detail}")
            }
        }
    }

    pub fn taxonomy(&self) -> FailureTaxonomy {
        match self {
            Self::ExpiredInvoice { .. }
            | Self::UnsupportedAsset { .. }
            | Self::AmountMismatch { .. }
            | Self::ProofMismatch
            | Self::BackendRejected { .. }
            | Self::ReplayDetected { .. } => FailureTaxonomy::Permanent,
            Self::BackendUnavailable | Self::BackendTimeout | Self::ReplayStoreFailure { .. } => {
                FailureTaxonomy::Transient
            }
            Self::PartialFailure { .. } | Self::MissingPreimage | Self::MissingProof => {
                FailureTaxonomy::Indeterminate
            }
        }
    }
}

pub trait ReplayGuard: Send + Sync {
    fn claim(&self, key: &str) -> Result<bool, String>;
    fn release(&self, key: &str) -> Result<(), String>;
}

#[derive(Default)]
pub struct InMemoryReplayGuard {
    claims: Mutex<HashSet<String>>,
}

impl ReplayGuard for InMemoryReplayGuard {
    fn claim(&self, key: &str) -> Result<bool, String> {
        let mut claims = self.claims
            .lock()
            .map_err(|_| "replay lock poisoned".to_string())?;
        Ok(claims.insert(key.to_string()))
    }

    fn release(&self, key: &str) -> Result<(), String> {
        let mut claims = self.claims
            .lock()
            .map_err(|_| "replay lock poisoned".to_string())?;
        claims.remove(key);
        Ok(())
    }
}

#[derive(Default)]
pub struct ProductionLightningBackend;

#[async_trait]
impl LightningBackend for ProductionLightningBackend {
    async fn settle_payment(
        &self,
        _request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        // Implementation for real Lightning node (e.g., LND, CLN) would go here.
        // For now, we return an error indicating it requires node configuration.
        warn!("Production Lightning backend invoked but not yet configured");
        Err(LightningBackendError::Unavailable)
    }
}

pub struct SimulatedLightningBackend;

#[async_trait]
impl LightningBackend for SimulatedLightningBackend {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        let preimage = request
            .proof_refs
            .iter()
            .find(|value| is_preimage_ref(value))
            .cloned()
            .unwrap_or_else(|| format!("preimage-{}", request.challenge));

        let proof = request
            .proof_refs
            .iter()
            .find(|value| is_proof_ref(value))
            .cloned()
            .or_else(|| request.proof_refs.first().cloned())
            .unwrap_or_else(|| format!("proof-{}", request.challenge));

        Ok(LightningSettlementResponse {
            settled_amount: request.amount,
            preimage,
            proof,
        })
    }
}

#[derive(Clone)]
pub struct LightningAdapter {
    backend: Arc<dyn LightningBackend>,
    replay_guard: Arc<dyn ReplayGuard>,
    backend_timeout: Duration,
    max_retries: usize,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl LightningAdapter {
    pub fn new(backend: Arc<dyn LightningBackend>) -> Self {
        Self {
            backend,
            replay_guard: Arc::new(InMemoryReplayGuard::default()),
            backend_timeout: DEFAULT_BACKEND_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            now: Arc::new(now_unix_secs),
        }
    }

    pub fn with_replay_guard(mut self, replay_guard: Arc<dyn ReplayGuard>) -> Self {
        self.replay_guard = replay_guard;
        self
    }

    pub fn with_clock<F>(mut self, now: F) -> Self
    where
        F: Fn() -> u64 + Send + Sync + 'static,
    {
        self.now = Arc::new(now);
        self
    }

    pub fn with_retry_policy(mut self, max_retries: usize, backend_timeout: Duration) -> Self {
        self.max_retries = max_retries;
        self.backend_timeout = backend_timeout;
        self
    }

    pub async fn execute_payment(
        &self,
        payload: &X402PaymentPayload,
    ) -> Result<LightningExecutionReceipt, LightningAdapterError> {
        let now = (self.now)();
        validate_request(payload, now)?;

        let mut intent = PaymentIntent::new(
            uuid::Uuid::new_v4().to_string(),
            payload.challenge.clone(),
            payload.amount as u64,
            payload.asset.clone(),
            payload.expiry,
        );

        if !self
            .replay_guard
            .claim(&payload.challenge)
            .map_err(|e| LightningAdapterError::ReplayStoreFailure { detail: e })?
        {
            return Err(LightningAdapterError::ReplayDetected {
                challenge: payload.challenge.clone(),
            });
        }

        let mut attempts = 0;
        let result = loop {
            intent.retry_count = attempts as u32;
            let settlement_req = LightningSettlementRequest {
                challenge: payload.challenge.clone(),
                amount: payload.amount,
                asset: payload.asset.clone(),
                expiry: payload.expiry,
                proof_refs: payload.proof_refs.clone(),
            };

            info!(
                challenge = %payload.challenge,
                attempt = attempts + 1,
                "Requesting Lightning settlement from backend"
            );

            let outcome = timeout(self.backend_timeout, self.backend.settle_payment(settlement_req)).await;

            match outcome {
                Ok(Ok(response)) => {
                    if response.settled_amount != payload.amount {
                        break Err(LightningAdapterError::AmountMismatch {
                            expected: payload.amount,
                            settled: response.settled_amount,
                        });
                    }
                    if response.preimage.is_empty() {
                        break Err(LightningAdapterError::MissingPreimage);
                    }
                    if response.proof.is_empty() {
                        break Err(LightningAdapterError::MissingProof);
                    }
                    if !payload.proof_refs.is_empty()
                        && !payload.proof_refs.contains(&response.proof)
                        && !payload.proof_refs.contains(&response.preimage)
                    {
                        break Err(LightningAdapterError::ProofMismatch);
                    }

                    intent.transition(PaymentLifecycle::Settled).unwrap_or_default();
                    break Ok(LightningExecutionReceipt {
                        challenge: payload.challenge.clone(),
                        settled_amount: response.settled_amount,
                        preimage: response.preimage,
                        proof: response.proof,
                    });
                }
                Ok(Err(LightningBackendError::Retryable { detail })) => {
                    attempts += 1;
                    if attempts > self.max_retries {
                        break Err(LightningAdapterError::BackendUnavailable);
                    }
                    warn!(
                        challenge = %payload.challenge,
                        detail = %detail,
                        "Backend retryable error, retrying..."
                    );
                }
                Ok(Err(LightningBackendError::Unavailable)) => {
                    attempts += 1;
                    if attempts > self.max_retries {
                        break Err(LightningAdapterError::BackendUnavailable);
                    }
                }
                Ok(Err(LightningBackendError::Rejected { detail })) => {
                    break Err(LightningAdapterError::BackendRejected { detail });
                }
                Ok(Err(LightningBackendError::PartialFailure { detail })) => {
                    break Err(LightningAdapterError::PartialFailure { detail });
                }
                Err(_) => {
                    attempts += 1;
                    if attempts > self.max_retries {
                        break Err(LightningAdapterError::BackendTimeout);
                    }
                }
            }
        };

        if result.is_err() {
            let _ = self.replay_guard.release(&payload.challenge);
        }

        result
    }
}

fn validate_request(payload: &X402PaymentPayload, now: u64) -> Result<(), LightningAdapterError> {
    if payload.expiry < now {
        return Err(LightningAdapterError::ExpiredInvoice {
            expiry: payload.expiry,
            now,
        });
    }

    if !SUPPORTED_ASSETS.contains(&payload.asset.as_str()) {
        return Err(LightningAdapterError::UnsupportedAsset {
            asset: payload.asset.clone(),
        });
    }

    Ok(())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn is_preimage_ref(val: &str) -> bool {
    val.starts_with("preimage-") || val.len() == 64
}

fn is_proof_ref(val: &str) -> bool {
    val.starts_with("proof-") || val.len() == 64
}
#[cfg(test)]
mod tests {
    use super::*;

    struct MockOutcome {
        result: Result<LightningSettlementResponse, LightningBackendError>,
    }

    struct SequenceBackend {
        outcomes: Mutex<Vec<MockOutcome>>,
        calls: Mutex<usize>,
    }

    impl SequenceBackend {
        fn new(outcomes: Vec<MockOutcome>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes),
                calls: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl LightningBackend for SequenceBackend {
        async fn settle_payment(&self, _req: LightningSettlementRequest) -> Result<LightningSettlementResponse, LightningBackendError> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            let mut outcomes = self.outcomes.lock().unwrap();
            outcomes.remove(0).result
        }
    }

    #[tokio::test]
    async fn test_lightning_adapter_success() {
        let response = LightningSettlementResponse {
            settled_amount: 1000,
            preimage: "preimage".to_string(),
            proof: "proof".to_string(),
        };
        let backend = SequenceBackend::new(vec![MockOutcome { result: Ok(response) }]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1000);

        let payload = X402PaymentPayload {
            amount: 1000,
            asset: "BTC".to_string(),
            challenge: "challenge".to_string(),
            expiry: 2000,
            proof_refs: vec![],
        };

        let receipt = adapter.execute_payment(&payload).await.unwrap();
        assert_eq!(receipt.settled_amount, 1000);
        assert_eq!(receipt.preimage, "preimage");
    }
}
