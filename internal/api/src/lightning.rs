use crate::x402::X402PaymentPayload;
use async_trait::async_trait;
use axum::http::StatusCode;
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
        self.claims
            .lock()
            .map_err(|_| "replay lock poisoned".to_string())
            .map(|mut claims| claims.insert(key.to_string()))
    }

    fn release(&self, key: &str) -> Result<(), String> {
        self.claims
            .lock()
            .map_err(|_| "replay lock poisoned".to_string())
            .map(|mut claims| {
                claims.remove(key);
            })
    }
}

#[derive(Default)]
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

        let replay_key = replay_key(payload);
        let claimed = self
            .replay_guard
            .claim(&replay_key)
            .map_err(|detail| LightningAdapterError::ReplayStoreFailure { detail })?;

        if !claimed {
            return Err(LightningAdapterError::ReplayDetected {
                challenge: payload.challenge.clone(),
            });
        }

        let request = LightningSettlementRequest {
            challenge: payload.challenge.clone(),
            amount: payload.amount,
            asset: payload.asset.clone(),
            expiry: payload.expiry,
            proof_refs: payload.proof_refs.clone(),
        };

        match self.settle_with_retries(request.clone()).await {
            Ok(response) => match validate_response(payload, &response) {
                Ok(()) => {
                    info!(
                        challenge = %payload.challenge,
                        amount = %response.settled_amount,
                        "Lightning payment settled"
                    );
                    Ok(LightningExecutionReceipt {
                        challenge: payload.challenge.clone(),
                        settled_amount: response.settled_amount,
                        preimage: response.preimage,
                        proof: response.proof,
                    })
                }
                Err(error) => {
                    release_replay_claim(&*self.replay_guard, &replay_key);
                    Err(error)
                }
            },
            Err(error) => {
                release_replay_claim(&*self.replay_guard, &replay_key);
                Err(error)
            }
        }
    }

    async fn settle_with_retries(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningAdapterError> {
        for attempt in 0..=self.max_retries {
            let backend = self.backend.clone();
            let request_clone = request.clone();

            let response = timeout(self.backend_timeout, async move {
                backend.settle_payment(request_clone).await
            })
            .await;

            match response {
                Ok(Ok(settled)) => return Ok(settled),
                Ok(Err(LightningBackendError::Retryable { detail })) => {
                    warn!(
                        attempt = attempt,
                        max_retries = self.max_retries,
                        detail = %detail,
                        "Lightning backend requested retry"
                    );

                    if attempt == self.max_retries {
                        return Err(LightningAdapterError::BackendRejected { detail });
                    }
                }
                Ok(Err(LightningBackendError::Unavailable)) => {
                    warn!(
                        attempt = attempt,
                        max_retries = self.max_retries,
                        "Lightning backend unavailable"
                    );

                    if attempt == self.max_retries {
                        return Err(LightningAdapterError::BackendUnavailable);
                    }
                }
                Ok(Err(LightningBackendError::Rejected { detail })) => {
                    return Err(LightningAdapterError::BackendRejected { detail });
                }
                Ok(Err(LightningBackendError::PartialFailure { detail })) => {
                    return Err(LightningAdapterError::PartialFailure { detail });
                }
                Err(_) => {
                    warn!(
                        attempt = attempt,
                        max_retries = self.max_retries,
                        timeout_ms = self.backend_timeout.as_millis(),
                        "Lightning backend timeout"
                    );
                    if attempt == self.max_retries {
                        return Err(LightningAdapterError::BackendTimeout);
                    }
                    continue;
                }
            }
        }

        Err(LightningAdapterError::BackendUnavailable)
    }
}

fn release_replay_claim(replay_guard: &dyn ReplayGuard, replay_key: &str) {
    if let Err(detail) = replay_guard.release(replay_key) {
        warn!(
            replay_key = %replay_key,
            detail = %detail,
            "Failed to release lightning replay claim"
        );
    }
}

fn validate_request(payload: &X402PaymentPayload, now: u64) -> Result<(), LightningAdapterError> {
    if !SUPPORTED_ASSETS
        .iter()
        .any(|asset| asset.eq_ignore_ascii_case(&payload.asset))
    {
        return Err(LightningAdapterError::UnsupportedAsset {
            asset: payload.asset.clone(),
        });
    }

    if payload.expiry <= now {
        return Err(LightningAdapterError::ExpiredInvoice {
            expiry: payload.expiry,
            now,
        });
    }

    Ok(())
}

fn validate_response(
    payload: &X402PaymentPayload,
    response: &LightningSettlementResponse,
) -> Result<(), LightningAdapterError> {
    if response.settled_amount != payload.amount {
        return Err(LightningAdapterError::AmountMismatch {
            expected: payload.amount,
            settled: response.settled_amount,
        });
    }

    if response.preimage.trim().is_empty() {
        return Err(LightningAdapterError::MissingPreimage);
    }

    if response.proof.trim().is_empty() {
        return Err(LightningAdapterError::MissingProof);
    }

    let proof_match = payload
        .proof_refs
        .iter()
        .any(|proof_ref| proof_ref == &response.proof);

    if !proof_match {
        return Err(LightningAdapterError::ProofMismatch);
    }

    if let Some(expected_preimage) = payload
        .proof_refs
        .iter()
        .find(|value| is_preimage_ref(value))
    {
        if expected_preimage != &response.preimage {
            return Err(LightningAdapterError::ProofMismatch);
        }
    }

    Ok(())
}

fn replay_key(payload: &X402PaymentPayload) -> String {
    let mut refs = payload.proof_refs.clone();
    refs.sort();
    format!(
        "{}:{}:{}:{}",
        payload.challenge,
        payload.amount,
        payload.asset,
        refs.join("|")
    )
}

fn is_preimage_ref(value: &str) -> bool {
    value.starts_with("preimage-")
}

fn is_proof_ref(value: &str) -> bool {
    value.starts_with("proof-") || value.starts_with("tx-") || value.starts_with("sig-")
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::{HashSet, VecDeque},
        sync::{atomic::AtomicUsize, atomic::Ordering, Mutex},
    };
    use tokio::time::sleep;

    #[derive(Default)]
    struct TestReplayGuard {
        claims: Mutex<HashSet<String>>,
        fail_claim: bool,
        fail_release: bool,
        release_calls: AtomicUsize,
    }

    impl TestReplayGuard {
        fn with_failures(fail_claim: bool, fail_release: bool) -> Self {
            Self {
                claims: Mutex::new(HashSet::new()),
                fail_claim,
                fail_release,
                release_calls: AtomicUsize::new(0),
            }
        }

        fn release_calls(&self) -> usize {
            self.release_calls.load(Ordering::SeqCst)
        }
    }

    impl ReplayGuard for TestReplayGuard {
        fn claim(&self, key: &str) -> Result<bool, String> {
            if self.fail_claim {
                return Err("claim store offline".to_string());
            }

            self.claims
                .lock()
                .map_err(|_| "lock poisoned".to_string())
                .map(|mut claims| claims.insert(key.to_string()))
        }

        fn release(&self, key: &str) -> Result<(), String> {
            self.release_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_release {
                return Err("release store offline".to_string());
            }

            self.claims
                .lock()
                .map_err(|_| "lock poisoned".to_string())
                .map(|mut claims| {
                    claims.remove(key);
                })
        }
    }

    #[derive(Clone)]
    enum MockOutcome {
        Success(LightningSettlementResponse),
        Error(LightningBackendError),
        DelayedSuccess {
            delay: Duration,
            response: LightningSettlementResponse,
        },
    }

    #[derive(Clone)]
    struct SequenceBackend {
        outcomes: Arc<Mutex<VecDeque<MockOutcome>>>,
        call_count: Arc<AtomicUsize>,
    }

    impl SequenceBackend {
        fn new(outcomes: Vec<MockOutcome>) -> Self {
            Self {
                outcomes: Arc::new(Mutex::new(VecDeque::from(outcomes))),
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LightningBackend for SequenceBackend {
        async fn settle_payment(
            &self,
            _request: LightningSettlementRequest,
        ) -> Result<LightningSettlementResponse, LightningBackendError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);

            let next = self
                .outcomes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    MockOutcome::Error(LightningBackendError::Rejected {
                        detail: "no mock response available".to_string(),
                    })
                });

            match next {
                MockOutcome::Success(response) => Ok(response),
                MockOutcome::Error(error) => Err(error),
                MockOutcome::DelayedSuccess { delay, response } => {
                    sleep(delay).await;
                    Ok(response)
                }
            }
        }
    }

    fn payload(now: u64) -> X402PaymentPayload {
        X402PaymentPayload {
            amount: 1_000,
            asset: "sBTC".to_string(),
            challenge: "invoice-123".to_string(),
            expiry: now + 120,
            proof_refs: vec!["proof-abc".to_string(), "preimage-abc".to_string()],
        }
    }

    fn successful_response() -> LightningSettlementResponse {
        LightningSettlementResponse {
            settled_amount: 1_000,
            preimage: "preimage-abc".to_string(),
            proof: "proof-abc".to_string(),
        }
    }

    #[tokio::test]
    async fn executes_successfully() {
        let backend = SequenceBackend::new(vec![MockOutcome::Success(successful_response())]);
        let adapter = LightningAdapter::new(Arc::new(backend.clone())).with_clock(|| 1_700_000_000);

        let receipt = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap();

        assert_eq!(receipt.settled_amount, 1_000);
        assert_eq!(receipt.preimage, "preimage-abc");
        assert_eq!(backend.call_count(), 1);
    }

    #[tokio::test]
    async fn rejects_expired_invoice() {
        let backend = SequenceBackend::new(vec![MockOutcome::Success(successful_response())]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1_700_000_000);

        let mut stale = payload(1_700_000_000);
        stale.expiry = 1_699_999_999;

        let error = adapter.execute_payment(&stale).await.unwrap_err();
        assert!(matches!(
            error,
            LightningAdapterError::ExpiredInvoice { .. }
        ));
    }

    #[tokio::test]
    async fn rejects_amount_mismatch() {
        let mut mismatch = successful_response();
        mismatch.settled_amount = 999;
        let backend = SequenceBackend::new(vec![MockOutcome::Success(mismatch)]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1_700_000_000);

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            LightningAdapterError::AmountMismatch {
                expected: 1_000,
                settled: 999
            }
        ));
    }

    #[tokio::test]
    async fn rejects_replay() {
        let backend = SequenceBackend::new(vec![
            MockOutcome::Success(successful_response()),
            MockOutcome::Success(successful_response()),
        ]);

        let adapter = LightningAdapter::new(Arc::new(backend.clone())).with_clock(|| 1_700_000_000);
        let request = payload(1_700_000_000);

        adapter.execute_payment(&request).await.unwrap();
        let error = adapter.execute_payment(&request).await.unwrap_err();

        assert!(matches!(
            error,
            LightningAdapterError::ReplayDetected { .. }
        ));
        assert_eq!(backend.call_count(), 1);
    }

    #[tokio::test]
    async fn retries_retryable_backend_error_then_succeeds() {
        let backend = SequenceBackend::new(vec![
            MockOutcome::Error(LightningBackendError::Retryable {
                detail: "temporary routing error".to_string(),
            }),
            MockOutcome::Success(successful_response()),
        ]);

        let adapter = LightningAdapter::new(Arc::new(backend.clone()))
            .with_clock(|| 1_700_000_000)
            .with_retry_policy(2, Duration::from_millis(50));

        let receipt = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap();

        assert_eq!(receipt.proof, "proof-abc");
        assert_eq!(backend.call_count(), 2);
    }

    #[tokio::test]
    async fn times_out_when_backend_is_slow() {
        let backend = SequenceBackend::new(vec![MockOutcome::DelayedSuccess {
            delay: Duration::from_millis(200),
            response: successful_response(),
        }]);

        let adapter = LightningAdapter::new(Arc::new(backend))
            .with_clock(|| 1_700_000_000)
            .with_retry_policy(0, Duration::from_millis(10));

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert_eq!(error, LightningAdapterError::BackendTimeout);
    }

    #[tokio::test]
    async fn returns_backend_unavailable_after_retries() {
        let backend = SequenceBackend::new(vec![
            MockOutcome::Error(LightningBackendError::Unavailable),
            MockOutcome::Error(LightningBackendError::Unavailable),
        ]);

        let adapter = LightningAdapter::new(Arc::new(backend))
            .with_clock(|| 1_700_000_000)
            .with_retry_policy(1, Duration::from_millis(20));

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert_eq!(error, LightningAdapterError::BackendUnavailable);
    }

    #[tokio::test]
    async fn propagates_partial_failure() {
        let backend = SequenceBackend::new(vec![MockOutcome::Error(
            LightningBackendError::PartialFailure {
                detail: "commit persisted but receipt missing".to_string(),
            },
        )]);

        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1_700_000_000);

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            LightningAdapterError::PartialFailure { detail }
            if detail.contains("receipt missing")
        ));
    }

    #[tokio::test]
    async fn rejects_missing_preimage() {
        let mut response = successful_response();
        response.preimage.clear();

        let backend = SequenceBackend::new(vec![MockOutcome::Success(response)]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1_700_000_000);

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert_eq!(error, LightningAdapterError::MissingPreimage);
    }

    #[tokio::test]
    async fn rejects_proof_mismatch() {
        let mut response = successful_response();
        response.proof = "proof-unexpected".to_string();

        let backend = SequenceBackend::new(vec![MockOutcome::Success(response)]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1_700_000_000);

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert_eq!(error, LightningAdapterError::ProofMismatch);
    }

    #[tokio::test]
    async fn rejects_unsupported_asset() {
        let backend = SequenceBackend::new(vec![MockOutcome::Success(successful_response())]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1_700_000_000);

        let mut request = payload(1_700_000_000);
        request.asset = "EUR".to_string();

        let error = adapter.execute_payment(&request).await.unwrap_err();
        assert!(matches!(
            error,
            LightningAdapterError::UnsupportedAsset { asset } if asset == "EUR"
        ));
    }

    #[tokio::test]
    async fn rejects_missing_proof() {
        let mut response = successful_response();
        response.proof.clear();

        let backend = SequenceBackend::new(vec![MockOutcome::Success(response)]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1_700_000_000);

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert_eq!(error, LightningAdapterError::MissingProof);
    }

    #[tokio::test]
    async fn simulated_backend_falls_back_when_no_proof_refs() {
        let backend = SimulatedLightningBackend;
        let response = backend
            .settle_payment(LightningSettlementRequest {
                challenge: "fallback-1".to_string(),
                amount: 77,
                asset: "sBTC".to_string(),
                expiry: 4_744_000_000,
                proof_refs: vec![],
            })
            .await
            .unwrap();

        assert_eq!(response.preimage, "preimage-fallback-1");
        assert_eq!(response.proof, "proof-fallback-1");
    }

    #[tokio::test]
    async fn surfaces_replay_store_claim_failure() {
        let replay = Arc::new(TestReplayGuard::with_failures(true, false));
        let backend = SequenceBackend::new(vec![MockOutcome::Success(successful_response())]);
        let adapter = LightningAdapter::new(Arc::new(backend))
            .with_clock(|| 1_700_000_000)
            .with_replay_guard(replay);

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            LightningAdapterError::ReplayStoreFailure { detail }
            if detail.contains("claim store offline")
        ));
    }

    #[tokio::test]
    async fn release_failure_does_not_mask_backend_error() {
        let replay = Arc::new(TestReplayGuard::with_failures(false, true));
        let backend =
            SequenceBackend::new(vec![MockOutcome::Error(LightningBackendError::Rejected {
                detail: "invalid proof".to_string(),
            })]);

        let adapter = LightningAdapter::new(Arc::new(backend))
            .with_clock(|| 1_700_000_000)
            .with_replay_guard(replay.clone());

        let error = adapter
            .execute_payment(&payload(1_700_000_000))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            LightningAdapterError::BackendRejected { detail }
            if detail == "invalid proof"
        ));
        assert_eq!(replay.release_calls(), 1);
    }

    #[test]
    fn lightning_error_metadata_is_stable() {
        let scenarios = vec![
            (
                LightningAdapterError::ExpiredInvoice {
                    expiry: 10,
                    now: 20,
                },
                StatusCode::PAYMENT_REQUIRED,
                "lightning_expired_invoice",
                "expired",
            ),
            (
                LightningAdapterError::ReplayDetected {
                    challenge: "abc".to_string(),
                },
                StatusCode::CONFLICT,
                "lightning_replay_detected",
                "Replay detected",
            ),
            (
                LightningAdapterError::BackendTimeout,
                StatusCode::GATEWAY_TIMEOUT,
                "lightning_backend_timeout",
                "timed out",
            ),
            (
                LightningAdapterError::ReplayStoreFailure {
                    detail: "disk error".to_string(),
                },
                StatusCode::INTERNAL_SERVER_ERROR,
                "lightning_replay_store_failure",
                "replay store",
            ),
        ];

        for (error, status, code, message_fragment) in scenarios {
            assert_eq!(error.status_code(), status);
            assert_eq!(error.code(), code);
            assert!(error.message().contains(message_fragment));
        }
    }
}
