use crate::x402::X402PaymentPayload;
use async_trait::async_trait;
use axum::http::StatusCode;
use conxian_core::{FailureTaxonomy, PaymentIntent, PaymentLifecycle};
use std::{
    collections::{HashMap, HashSet},
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

/// A server-issued Lightning invoice the client must pay (x402 server-issues →
/// client-pays → server-verifies). Returned by `make_invoice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedInvoice {
    /// BOLT11 payment request the client pays.
    pub bolt11: String,
    /// Payment hash used to look the invoice up for settlement verification.
    pub payment_hash: String,
}

/// The settlement state of a server-issued invoice, as reported by
/// `lookup_invoice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceSettlement {
    pub settled: bool,
    /// Amount received, in satoshi, when the backend reports it. `None` means
    /// the backend does not expose the amount (the caller then trusts the
    /// invoice's bound amount).
    pub amount_sats: Option<u64>,
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
    /// Legacy: settle by paying an invoice. **Never call this with a
    /// client-supplied invoice** — it spends the operator wallet. The x402 rail
    /// uses [`Self::create_invoice`] + [`Self::lookup_invoice`] instead.
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError>;

    /// Issue an invoice for the client to pay (server-issues flow).
    ///
    /// Defaults to `Unavailable` (fail-closed) so a backend that cannot issue
    /// invoices keeps x402 settlement closed rather than mis-paying.
    async fn create_invoice(
        &self,
        _amount_sats: u64,
        _expiry_seconds: u64,
        _description: &str,
    ) -> Result<IssuedInvoice, LightningBackendError> {
        Err(LightningBackendError::Unavailable)
    }

    /// Verify settlement of a server-issued invoice by payment hash.
    ///
    /// Defaults to `Unavailable` (fail-closed).
    async fn lookup_invoice(
        &self,
        _payment_hash: &str,
    ) -> Result<InvoiceSettlement, LightningBackendError> {
        Err(LightningBackendError::Unavailable)
    }
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
    ExpiredInvoice {
        expiry: u64,
        now: u64,
    },
    UnsupportedAsset {
        asset: String,
    },
    ReplayDetected {
        challenge: String,
    },
    AmountMismatch {
        expected: u128,
        settled: u128,
    },
    MissingPreimage,
    MissingProof,
    ProofMismatch,
    BackendUnavailable,
    BackendTimeout,
    BackendRejected {
        detail: String,
    },
    PartialFailure {
        detail: String,
    },
    ReplayStoreFailure {
        detail: String,
    },
    /// The client returned a challenge the server never issued.
    UnknownChallenge {
        challenge: String,
    },
    /// The challenge was issued for a different resource than the one requested.
    ChallengeResourceMismatch {
        challenge: String,
    },
    /// The server-issued invoice has not been settled yet.
    NotSettled,
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
            Self::UnknownChallenge { .. }
            | Self::ChallengeResourceMismatch { .. }
            | Self::NotSettled => StatusCode::PAYMENT_REQUIRED,
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
            Self::UnknownChallenge { .. } => "x402_unknown_challenge",
            Self::ChallengeResourceMismatch { .. } => "x402_challenge_resource_mismatch",
            Self::NotSettled => "x402_invoice_not_settled",
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
            Self::UnknownChallenge { challenge } => {
                format!("Challenge was not issued by this server: {challenge}")
            }
            Self::ChallengeResourceMismatch { challenge } => {
                format!("Challenge {challenge} was not issued for this resource")
            }
            Self::NotSettled => "Server-issued invoice has not been settled".to_string(),
        }
    }

    pub fn taxonomy(&self) -> FailureTaxonomy {
        match self {
            Self::ExpiredInvoice { .. }
            | Self::UnsupportedAsset { .. }
            | Self::AmountMismatch { .. }
            | Self::ProofMismatch
            | Self::BackendRejected { .. }
            | Self::ReplayDetected { .. }
            | Self::UnknownChallenge { .. }
            | Self::ChallengeResourceMismatch { .. } => FailureTaxonomy::Permanent,
            Self::BackendUnavailable | Self::BackendTimeout | Self::ReplayStoreFailure { .. } => {
                FailureTaxonomy::Transient
            }
            Self::PartialFailure { .. }
            | Self::MissingPreimage
            | Self::MissingProof
            | Self::NotSettled => FailureTaxonomy::Indeterminate,
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
        let mut claims = self
            .claims
            .lock()
            .map_err(|_| "replay lock poisoned".to_string())?;
        Ok(claims.insert(key.to_string()))
    }

    fn release(&self, key: &str) -> Result<(), String> {
        let mut claims = self
            .claims
            .lock()
            .map_err(|_| "replay lock poisoned".to_string())?;
        claims.remove(key);
        Ok(())
    }
}

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
    /// Simulated server-issued invoice: the payment hash is derived from the
    /// amount so the matching `lookup_invoice` reports it settled for the exact
    /// bound amount.
    async fn create_invoice(
        &self,
        amount_sats: u64,
        _expiry_seconds: u64,
        _description: &str,
    ) -> Result<IssuedInvoice, LightningBackendError> {
        Ok(IssuedInvoice {
            bolt11: format!("lnbc-sim-{amount_sats}"),
            payment_hash: format!("sim-hash-{amount_sats}"),
        })
    }

    async fn lookup_invoice(
        &self,
        payment_hash: &str,
    ) -> Result<InvoiceSettlement, LightningBackendError> {
        let amount_sats = payment_hash
            .strip_prefix("sim-hash-")
            .and_then(|s| s.parse::<u64>().ok());
        match amount_sats {
            Some(amount) => Ok(InvoiceSettlement {
                settled: true,
                amount_sats: Some(amount),
            }),
            None => Ok(InvoiceSettlement {
                settled: false,
                amount_sats: None,
            }),
        }
    }

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
    issued: Arc<Mutex<HashMap<String, IssuedChallenge>>>,
}

/// A challenge the server issued: the invoice the client must pay, bound to the
/// amount, resource, and expiry the server chose. Looked up by the opaque
/// challenge id the client returns (never trusting a client-supplied invoice).
#[derive(Debug, Clone)]
pub struct IssuedChallenge {
    pub challenge: String,
    pub payment_hash: String,
    pub bolt11: String,
    pub amount_sats: u64,
    pub asset: String,
    pub resource: String,
    pub expiry: u64,
}

/// The 402 body a client receives: the invoice to pay, bound server-side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X402ChallengeResponse {
    pub challenge: String,
    pub bolt11: String,
    pub amount_satoshi: u64,
    pub asset: String,
    pub expiry: u64,
    pub resource: String,
}

impl LightningAdapter {
    pub fn new(backend: Arc<dyn LightningBackend>) -> Self {
        Self {
            backend,
            replay_guard: Arc::new(InMemoryReplayGuard::default()),
            backend_timeout: DEFAULT_BACKEND_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            now: Arc::new(now_unix_secs),
            issued: Arc::new(Mutex::new(HashMap::new())),
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

    /// Issue a server-bound x402 challenge for `resource`: the server picks the
    /// amount and expiry, issues an invoice the client must pay, and records the
    /// binding so a later return can be verified against the server's record
    /// (never against a client-supplied invoice).
    pub async fn issue_challenge(
        &self,
        resource: &str,
        amount_sats: u64,
        expiry_seconds: u64,
    ) -> Result<X402ChallengeResponse, LightningAdapterError> {
        let issued = timeout(
            self.backend_timeout,
            self.backend
                .create_invoice(amount_sats, expiry_seconds, "Conxian x402"),
        )
        .await
        .map_err(|_| LightningAdapterError::BackendTimeout)?
        .map_err(map_backend_error)?;
        if issued.bolt11.is_empty() || issued.payment_hash.is_empty() {
            return Err(LightningAdapterError::MissingProof);
        }
        let now = (self.now)();
        let challenge = uuid::Uuid::new_v4().to_string();
        let record = IssuedChallenge {
            challenge: challenge.clone(),
            payment_hash: issued.payment_hash.clone(),
            bolt11: issued.bolt11.clone(),
            amount_sats,
            asset: crate::x402::X402_ASSET.to_string(),
            resource: resource.to_string(),
            expiry: now + expiry_seconds,
        };
        self.issued
            .lock()
            .map_err(|e| LightningAdapterError::ReplayStoreFailure {
                detail: e.to_string(),
            })?
            .insert(challenge.clone(), record);
        Ok(X402ChallengeResponse {
            challenge,
            bolt11: issued.bolt11,
            amount_satoshi: amount_sats,
            asset: crate::x402::X402_ASSET.to_string(),
            expiry: now + expiry_seconds,
            resource: resource.to_string(),
        })
    }

    /// Verify a returned challenge against the server's own record: the challenge
    /// must have been issued, be unexpired, and be for this exact resource; the
    /// invoice must be settled for at least the bound amount. The challenge is
    /// claimed exactly once, so a settled challenge cannot be replayed.
    pub async fn verify_challenge(
        &self,
        challenge: &str,
        resource: &str,
    ) -> Result<IssuedChallenge, LightningAdapterError> {
        let record = {
            let issued =
                self.issued
                    .lock()
                    .map_err(|e| LightningAdapterError::ReplayStoreFailure {
                        detail: e.to_string(),
                    })?;
            issued.get(challenge).cloned().ok_or_else(|| {
                LightningAdapterError::UnknownChallenge {
                    challenge: challenge.to_string(),
                }
            })?
        };

        if record.resource != resource {
            return Err(LightningAdapterError::ChallengeResourceMismatch {
                challenge: challenge.to_string(),
            });
        }

        let now = (self.now)();
        if record.expiry < now {
            return Err(LightningAdapterError::ExpiredInvoice {
                expiry: record.expiry,
                now,
            });
        }

        let settlement = timeout(
            self.backend_timeout,
            self.backend.lookup_invoice(&record.payment_hash),
        )
        .await
        .map_err(|_| LightningAdapterError::BackendTimeout)?
        .map_err(map_backend_error)?;
        if !settlement.settled {
            return Err(LightningAdapterError::NotSettled);
        }
        if let Some(received) = settlement.amount_sats {
            if (received as u128) < (record.amount_sats as u128) {
                return Err(LightningAdapterError::AmountMismatch {
                    expected: record.amount_sats as u128,
                    settled: received as u128,
                });
            }
        }

        // Claim exactly once; a replay of a settled challenge is rejected.
        if !self
            .replay_guard
            .claim(&record.payment_hash)
            .map_err(|e| LightningAdapterError::ReplayStoreFailure { detail: e })?
        {
            return Err(LightningAdapterError::ReplayDetected {
                challenge: challenge.to_string(),
            });
        }

        Ok(record)
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

            let outcome = timeout(
                self.backend_timeout,
                self.backend.settle_payment(settlement_req),
            )
            .await;

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

                    intent
                        .transition(PaymentLifecycle::Settled)
                        .unwrap_or_default();
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

/// Map a backend error from the server-issued invoice flow (`create_invoice` /
/// `lookup_invoice`) to an adapter error. A backend that cannot issue or verify
/// invoices maps to `BackendUnavailable`, so settlement stays fail-closed.
fn map_backend_error(error: LightningBackendError) -> LightningAdapterError {
    match error {
        LightningBackendError::Rejected { detail } => {
            LightningAdapterError::BackendRejected { detail }
        }
        LightningBackendError::PartialFailure { detail } => {
            LightningAdapterError::PartialFailure { detail }
        }
        LightningBackendError::Retryable { .. } | LightningBackendError::Unavailable => {
            LightningAdapterError::BackendUnavailable
        }
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock moved backwards")
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

    struct SimulatedOutcome {
        result: Result<LightningSettlementResponse, LightningBackendError>,
    }

    struct SequenceBackend {
        outcomes: Mutex<Vec<SimulatedOutcome>>,
        calls: Mutex<usize>,
    }

    impl SequenceBackend {
        fn new(outcomes: Vec<SimulatedOutcome>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes),
                calls: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl LightningBackend for SequenceBackend {
        async fn settle_payment(
            &self,
            _req: LightningSettlementRequest,
        ) -> Result<LightningSettlementResponse, LightningBackendError> {
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
        let backend = SequenceBackend::new(vec![SimulatedOutcome {
            result: Ok(response),
        }]);
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

    #[test]
    fn test_in_memory_replay_guard() {
        let guard = InMemoryReplayGuard::default();
        assert!(guard.claim("key1").unwrap());
        assert!(!guard.claim("key1").unwrap());
        guard.release("key1").unwrap();
        assert!(guard.claim("key1").unwrap());
    }

    #[test]
    fn test_lightning_adapter_error_methods() {
        let err = LightningAdapterError::ExpiredInvoice {
            expiry: 100,
            now: 200,
        };
        assert_eq!(err.status_code(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(err.code(), "lightning_expired_invoice");
        assert!(err.message().contains("expired"));
        assert_eq!(err.taxonomy(), FailureTaxonomy::Permanent);

        let err = LightningAdapterError::UnsupportedAsset {
            asset: "USD".into(),
        };
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.taxonomy(), FailureTaxonomy::Permanent);

        let err = LightningAdapterError::ReplayDetected {
            challenge: "c1".into(),
        };
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
        assert_eq!(err.taxonomy(), FailureTaxonomy::Permanent);

        let err = LightningAdapterError::BackendUnavailable;
        assert_eq!(err.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(err.taxonomy(), FailureTaxonomy::Transient);

        let err = LightningAdapterError::BackendTimeout;
        assert_eq!(err.status_code(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(err.taxonomy(), FailureTaxonomy::Transient);

        let err = LightningAdapterError::PartialFailure {
            detail: "part".into(),
        };
        assert_eq!(err.status_code(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.taxonomy(), FailureTaxonomy::Indeterminate);

        let err = LightningAdapterError::ReplayStoreFailure {
            detail: "fail".into(),
        };
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.taxonomy(), FailureTaxonomy::Transient);
    }

    #[tokio::test]
    async fn test_execute_payment_failure_paths() {
        let payload = X402PaymentPayload {
            amount: 1000,
            asset: "BTC".into(),
            challenge: "fail-c1".into(),
            expiry: 2000,
            proof_refs: vec![],
        };

        // Expired
        let backend = SequenceBackend::new(vec![]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 3000);
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::ExpiredInvoice { .. }));

        // Unsupported Asset
        let bad_payload = X402PaymentPayload {
            asset: "GOL".into(),
            ..payload.clone()
        };
        let adapter =
            LightningAdapter::new(Arc::new(SequenceBackend::new(vec![]))).with_clock(|| 1000);
        let err = adapter.execute_payment(&bad_payload).await.unwrap_err();
        assert!(matches!(
            err,
            LightningAdapterError::UnsupportedAsset { .. }
        ));

        // Replay Detected
        let adapter =
            LightningAdapter::new(Arc::new(SequenceBackend::new(vec![]))).with_clock(|| 1000);
        adapter.replay_guard.claim(&payload.challenge).unwrap();
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::ReplayDetected { .. }));

        // Amount Mismatch
        let response = LightningSettlementResponse {
            settled_amount: 500,
            preimage: "p".into(),
            proof: "p".into(),
        };
        let adapter =
            LightningAdapter::new(Arc::new(SequenceBackend::new(vec![SimulatedOutcome {
                result: Ok(response),
            }])))
            .with_clock(|| 1000);
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::AmountMismatch { .. }));

        // Backend Rejected
        let adapter =
            LightningAdapter::new(Arc::new(SequenceBackend::new(vec![SimulatedOutcome {
                result: Err(LightningBackendError::Rejected {
                    detail: "no".into(),
                }),
            }])))
            .with_clock(|| 1000);
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::BackendRejected { .. }));

        // Use a backend that actually hangs to trigger timeout
        struct HangingBackend;
        #[async_trait]
        impl LightningBackend for HangingBackend {
            async fn settle_payment(
                &self,
                _req: LightningSettlementRequest,
            ) -> Result<LightningSettlementResponse, LightningBackendError> {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Err(LightningBackendError::Unavailable)
            }
        }
        let adapter = LightningAdapter::new(Arc::new(HangingBackend))
            .with_clock(|| 1000)
            .with_retry_policy(0, Duration::from_millis(10));
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::BackendTimeout));

        // Backend Unavailable (Retry Exhaustion)
        let adapter = LightningAdapter::new(Arc::new(SequenceBackend::new(vec![
            SimulatedOutcome {
                result: Err(LightningBackendError::Unavailable),
            },
            SimulatedOutcome {
                result: Err(LightningBackendError::Unavailable),
            },
        ])))
        .with_clock(|| 1000)
        .with_retry_policy(1, Duration::from_millis(100));
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::BackendUnavailable));
    }

    #[test]
    fn test_utility_functions() {
        assert!(is_preimage_ref("preimage-123"));
        assert!(is_preimage_ref(&"a".repeat(64)));
        assert!(!is_preimage_ref("too-short"));

        assert!(is_proof_ref("proof-123"));
        assert!(is_proof_ref(&"b".repeat(64)));
        assert!(!is_proof_ref("short"));
    }

    #[tokio::test]
    async fn test_backend_implementations() {
        let request = LightningSettlementRequest {
            challenge: "c1".into(),
            amount: 100,
            asset: "BTC".into(),
            expiry: 1000,
            proof_refs: vec!["preimage-p1".into(), "proof-s1".into()],
        };

        // Simulated
        let backend = SimulatedLightningBackend;
        let response = backend.settle_payment(request.clone()).await.unwrap();
        assert_eq!(response.settled_amount, 100);
        assert_eq!(response.preimage, "preimage-p1");
        assert_eq!(response.proof, "proof-s1");

        // Production (Initial Implementation)
        let backend = ProductionLightningBackend;
        let err = backend.settle_payment(request).await.unwrap_err();
        assert!(matches!(err, LightningBackendError::Unavailable));
    }

    #[tokio::test]
    async fn test_execute_payment_mismatches_and_missing() {
        let payload = X402PaymentPayload {
            amount: 1000,
            asset: "BTC".into(),
            challenge: "fail-c2".into(),
            expiry: 2000,
            proof_refs: vec!["wanted-proof".into()],
        };

        // Missing Preimage
        let backend = SequenceBackend::new(vec![SimulatedOutcome {
            result: Ok(LightningSettlementResponse {
                settled_amount: 1000,
                preimage: "".into(),
                proof: "proof".into(),
            }),
        }]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1000);
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::MissingPreimage));

        // Missing Proof
        let backend = SequenceBackend::new(vec![SimulatedOutcome {
            result: Ok(LightningSettlementResponse {
                settled_amount: 1000,
                preimage: "preimage".into(),
                proof: "".into(),
            }),
        }]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1000);
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::MissingProof));

        // Proof Mismatch
        let backend = SequenceBackend::new(vec![SimulatedOutcome {
            result: Ok(LightningSettlementResponse {
                settled_amount: 1000,
                preimage: "wrong-preimage".into(),
                proof: "wrong-proof".into(),
            }),
        }]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1000);
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::ProofMismatch));

        // Partial Failure
        let backend = SequenceBackend::new(vec![SimulatedOutcome {
            result: Err(LightningBackendError::PartialFailure {
                detail: "oops".into(),
            }),
        }]);
        let adapter = LightningAdapter::new(Arc::new(backend)).with_clock(|| 1000);
        let err = adapter.execute_payment(&payload).await.unwrap_err();
        assert!(matches!(err, LightningAdapterError::PartialFailure { .. }));
    }
}

#[cfg(test)]
mod additional_lightning_tests {
    use super::*;

    #[test]
    fn test_lightning_adapter_error_codes_and_taxonomy() {
        let cases = vec![
            (
                LightningAdapterError::BackendRejected {
                    detail: "no".into(),
                },
                "lightning_backend_rejected",
                FailureTaxonomy::Permanent,
            ),
            (
                LightningAdapterError::AmountMismatch {
                    expected: 1,
                    settled: 2,
                },
                "lightning_amount_mismatch",
                FailureTaxonomy::Permanent,
            ),
            (
                LightningAdapterError::MissingPreimage,
                "lightning_missing_preimage",
                FailureTaxonomy::Indeterminate,
            ),
            (
                LightningAdapterError::MissingProof,
                "lightning_missing_proof",
                FailureTaxonomy::Indeterminate,
            ),
            (
                LightningAdapterError::ProofMismatch,
                "lightning_proof_mismatch",
                FailureTaxonomy::Permanent,
            ),
            (
                LightningAdapterError::BackendUnavailable,
                "lightning_backend_unavailable",
                FailureTaxonomy::Transient,
            ),
            (
                LightningAdapterError::BackendTimeout,
                "lightning_backend_timeout",
                FailureTaxonomy::Transient,
            ),
            (
                LightningAdapterError::PartialFailure {
                    detail: "part".into(),
                },
                "lightning_partial_failure",
                FailureTaxonomy::Indeterminate,
            ),
            (
                LightningAdapterError::ReplayStoreFailure {
                    detail: "fail".into(),
                },
                "lightning_replay_store_failure",
                FailureTaxonomy::Transient,
            ),
        ];

        for (err, expected_code, expected_taxonomy) in cases {
            assert_eq!(err.code(), expected_code);
            assert_eq!(err.taxonomy(), expected_taxonomy);
            assert!(!err.message().is_empty());
        }
    }

    #[tokio::test]
    async fn test_simulated_backend_preimage_logic() {
        let backend = SimulatedLightningBackend;
        let request = LightningSettlementRequest {
            challenge: "c1".into(),
            amount: 100,
            asset: "BTC".into(),
            expiry: 1000,
            proof_refs: vec!["some-random-ref".into()],
        };
        let response = backend.settle_payment(request).await.unwrap();
        assert!(response.preimage.contains("c1"));
        assert_eq!(response.proof, "some-random-ref");
    }

    /// A backend that implements the server-issued invoice flow with a fixed
    /// settlement result, so `issue_challenge`/`verify_challenge` can be tested
    /// without a Lightning node.
    struct ServerIssuedBackend {
        settled: bool,
        amount_sats: Option<u64>,
        issued: bool,
    }

    #[async_trait]
    impl LightningBackend for ServerIssuedBackend {
        async fn settle_payment(
            &self,
            _req: LightningSettlementRequest,
        ) -> Result<LightningSettlementResponse, LightningBackendError> {
            Err(LightningBackendError::Unavailable)
        }

        async fn create_invoice(
            &self,
            _amount_sats: u64,
            _expiry_seconds: u64,
            _description: &str,
        ) -> Result<IssuedInvoice, LightningBackendError> {
            if !self.issued {
                return Err(LightningBackendError::Unavailable);
            }
            Ok(IssuedInvoice {
                bolt11: "lnbc1servertest".to_string(),
                payment_hash: "hash-server-test".to_string(),
            })
        }

        async fn lookup_invoice(
            &self,
            _payment_hash: &str,
        ) -> Result<InvoiceSettlement, LightningBackendError> {
            Ok(InvoiceSettlement {
                settled: self.settled,
                amount_sats: self.amount_sats,
            })
        }
    }

    fn issued_adapter(settled: bool, amount_sats: Option<u64>) -> LightningAdapter {
        LightningAdapter::new(Arc::new(ServerIssuedBackend {
            settled,
            amount_sats,
            issued: true,
        }))
        .with_clock(|| 1_000)
    }

    #[tokio::test]
    async fn issue_then_verify_settled_challenge_succeeds() {
        let adapter = issued_adapter(true, Some(1000));
        let issued = adapter.issue_challenge("/settle", 1000, 900).await.unwrap();
        assert_eq!(issued.amount_satoshi, 1000);
        assert_eq!(issued.bolt11, "lnbc1servertest");
        let record = adapter
            .verify_challenge(&issued.challenge, "/settle")
            .await
            .unwrap();
        assert_eq!(record.payment_hash, "hash-server-test");
        assert_eq!(record.amount_sats, 1000);
    }

    #[tokio::test]
    async fn unissued_challenge_is_rejected() {
        let adapter = issued_adapter(true, Some(1000));
        let err = adapter
            .verify_challenge("not-issued-by-us", "/settle")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LightningAdapterError::UnknownChallenge { .. }
        ));
        assert_eq!(err.code(), "x402_unknown_challenge");
    }

    #[tokio::test]
    async fn challenge_for_a_different_resource_is_rejected() {
        let adapter = issued_adapter(true, Some(1000));
        let issued = adapter.issue_challenge("/settle", 1000, 900).await.unwrap();
        let err = adapter
            .verify_challenge(&issued.challenge, "/erp/sync")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LightningAdapterError::ChallengeResourceMismatch { .. }
        ));
    }

    #[tokio::test]
    async fn unsettled_invoice_is_rejected() {
        let adapter = issued_adapter(false, Some(0));
        let issued = adapter.issue_challenge("/settle", 1000, 900).await.unwrap();
        let err = adapter
            .verify_challenge(&issued.challenge, "/settle")
            .await
            .unwrap_err();
        assert!(matches!(err, LightningAdapterError::NotSettled));
    }

    #[tokio::test]
    async fn underpaid_invoice_is_rejected() {
        // The invoice settled for less than the bound amount.
        let adapter = issued_adapter(true, Some(500));
        let issued = adapter.issue_challenge("/settle", 1000, 900).await.unwrap();
        let err = adapter
            .verify_challenge(&issued.challenge, "/settle")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LightningAdapterError::AmountMismatch {
                expected: 1000,
                settled: 500
            }
        ));
    }

    #[tokio::test]
    async fn settled_challenge_cannot_be_replayed() {
        let adapter = issued_adapter(true, Some(1000));
        let issued = adapter.issue_challenge("/settle", 1000, 900).await.unwrap();
        adapter
            .verify_challenge(&issued.challenge, "/settle")
            .await
            .unwrap();
        let err = adapter
            .verify_challenge(&issued.challenge, "/settle")
            .await
            .unwrap_err();
        assert!(matches!(err, LightningAdapterError::ReplayDetected { .. }));
    }

    #[tokio::test]
    async fn expired_challenge_is_rejected() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let clock = Arc::new(AtomicU64::new(1_000));
        let handle = clock.clone();
        let adapter = LightningAdapter::new(Arc::new(ServerIssuedBackend {
            settled: true,
            amount_sats: Some(1000),
            issued: true,
        }))
        .with_clock(move || handle.load(Ordering::SeqCst));
        // Issued at t=1000 with a 60s expiry -> valid until 1060.
        let issued = adapter.issue_challenge("/settle", 1000, 60).await.unwrap();
        // Advance the clock past the expiry.
        clock.store(2_000, Ordering::SeqCst);
        let err = adapter
            .verify_challenge(&issued.challenge, "/settle")
            .await
            .unwrap_err();
        assert!(matches!(err, LightningAdapterError::ExpiredInvoice { .. }));
    }

    #[tokio::test]
    async fn backend_without_invoice_support_fails_closed() {
        // The default trait methods return Unavailable, so a backend that cannot
        // issue invoices keeps x402 settlement closed.
        let adapter = LightningAdapter::new(Arc::new(ProductionLightningBackend));
        let err = adapter
            .issue_challenge("/settle", 1000, 900)
            .await
            .unwrap_err();
        assert!(matches!(err, LightningAdapterError::BackendUnavailable));
    }
}
