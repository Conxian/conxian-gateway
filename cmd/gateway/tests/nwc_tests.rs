// ============================================================
// G-22: Nostr NWC (NIP-47) Relay Integration Test
// ============================================================
//
// Validates the Nostr Wallet Connect relay adapter:
// 1. Success path for NWC spontaneous payment settlement
// 2. NWC relay errors (unavailable, rejected, partial failure)
// 3. Adapter-level error wrapping

use conxian_api::lightning::{
    LightningAdapter, LightningAdapterError, LightningBackend, LightningBackendError,
    LightningSettlementRequest, LightningSettlementResponse,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Simulated NWC relay that tracks requests.
struct MockNwcRelay {
    settle_calls: Mutex<Vec<LightningSettlementRequest>>,
}

impl MockNwcRelay {
    fn new() -> Self {
        Self {
            settle_calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl LightningBackend for MockNwcRelay {
    async fn settle_payment(
        &self,
        req: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        self.settle_calls.lock().unwrap().push(req.clone());
        Ok(LightningSettlementResponse {
            settled_amount: req.amount,
            preimage: "preimage-nwc-abc123".to_string(),
            proof: "proof-nwc-xyz789".to_string(),
        })
    }
}

#[tokio::test]
async fn test_nwc_spontaneous_payment_success() {
    let mock = Arc::new(MockNwcRelay::new());
    let adapter = LightningAdapter::new(mock.clone()).with_clock(|| 1000);

    let payload = conxian_api::x402::X402PaymentPayload {
        amount: 50_000,
        asset: "BTC".to_string(),
        challenge: "nwc-challenge-001".to_string(),
        expiry: 2000,
        proof_refs: vec![
            "preimage-nwc-abc123".to_string(),
            "proof-nwc-xyz789".to_string(),
        ],
    };

    let result = adapter.execute_payment(&payload).await;
    assert!(result.is_ok());

    let receipt = result.unwrap();
    assert_eq!(receipt.settled_amount, 50_000);
    assert_eq!(receipt.preimage, "preimage-nwc-abc123");
    assert_eq!(receipt.proof, "proof-nwc-xyz789");

    let calls = mock.settle_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].amount, 50_000);
    assert_eq!(calls[0].challenge, "nwc-challenge-001");
}

/// Failing NWC relay backend for testing error paths.
struct FailingNwcRelay {
    error: LightningBackendError,
}

#[async_trait]
impl LightningBackend for FailingNwcRelay {
    async fn settle_payment(
        &self,
        _req: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        Err(self.error.clone())
    }
}

#[tokio::test]
async fn test_nwc_relay_unavailable() {
    let backend = Arc::new(FailingNwcRelay {
        error: LightningBackendError::Unavailable,
    });
    let adapter = LightningAdapter::new(backend)
        .with_clock(|| 1000)
        .with_retry_policy(0, Duration::from_millis(10));

    let payload = conxian_api::x402::X402PaymentPayload {
        amount: 10_000,
        asset: "BTC".to_string(),
        challenge: "nwc-challenge-002".to_string(),
        expiry: 2000,
        proof_refs: vec![],
    };

    let err = adapter.execute_payment(&payload).await.unwrap_err();
    assert!(matches!(err, LightningAdapterError::BackendUnavailable));
}

#[tokio::test]
async fn test_nwc_relay_rejected() {
    let backend = Arc::new(FailingNwcRelay {
        error: LightningBackendError::Rejected {
            detail: "NWC: insufficient balance".to_string(),
        },
    });
    let adapter = LightningAdapter::new(backend).with_clock(|| 1000);

    let payload = conxian_api::x402::X402PaymentPayload {
        amount: 10_000,
        asset: "BTC".to_string(),
        challenge: "nwc-challenge-003".to_string(),
        expiry: 2000,
        proof_refs: vec![],
    };

    let err = adapter.execute_payment(&payload).await.unwrap_err();
    assert!(matches!(err, LightningAdapterError::BackendRejected { .. }));
    assert!(err.message().contains("insufficient balance"));
}

#[tokio::test]
async fn test_nwc_relay_partial_failure() {
    let backend = Arc::new(FailingNwcRelay {
        error: LightningBackendError::PartialFailure {
            detail: "NWC: partial route failure".to_string(),
        },
    });
    let adapter = LightningAdapter::new(backend).with_clock(|| 1000);

    let payload = conxian_api::x402::X402PaymentPayload {
        amount: 25_000,
        asset: "sBTC".to_string(),
        challenge: "nwc-challenge-004".to_string(),
        expiry: 2000,
        proof_refs: vec![],
    };

    let err = adapter.execute_payment(&payload).await.unwrap_err();
    assert!(matches!(err, LightningAdapterError::PartialFailure { .. }));
    assert!(err.message().contains("partial route failure"));
}

#[tokio::test]
async fn test_nwc_retryable_error() {
    // Backend returns Retryable; adapter should retry and eventually succeed
    // if the second attempt works.
    struct RetryOnceBackend {
        attempt: Mutex<u32>,
    }
    #[async_trait]
    impl LightningBackend for RetryOnceBackend {
        async fn settle_payment(
            &self,
            req: LightningSettlementRequest,
        ) -> Result<LightningSettlementResponse, LightningBackendError> {
            let mut attempt = self.attempt.lock().unwrap();
            *attempt += 1;
            if *attempt == 1 {
                Err(LightningBackendError::Retryable {
                    detail: "transient NWC error".to_string(),
                })
            } else {
                Ok(LightningSettlementResponse {
                    settled_amount: req.amount,
                    preimage: "preimage-nwc-retry-abc".to_string(),
                    proof: "proof-nwc-retry-xyz".to_string(),
                })
            }
        }
    }

    let backend = Arc::new(RetryOnceBackend {
        attempt: Mutex::new(0),
    });
    let adapter = LightningAdapter::new(backend)
        .with_clock(|| 1000)
        .with_retry_policy(2, Duration::from_millis(10));

    let payload = conxian_api::x402::X402PaymentPayload {
        amount: 30_000,
        asset: "BTC".to_string(),
        challenge: "nwc-challenge-retry".to_string(),
        expiry: 3000,
        proof_refs: vec![
            "preimage-nwc-retry-abc".to_string(),
            "proof-nwc-retry-xyz".to_string(),
        ],
    };

    let result = adapter.execute_payment(&payload).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().settled_amount, 30_000);
}
