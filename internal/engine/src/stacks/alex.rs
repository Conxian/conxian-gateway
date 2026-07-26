use crate::stacks::rpc::StacksRpc;
use async_trait::async_trait;
pub use conxian_core::AlexSwapRequest;
use conxian_core::{
    AlexAssetRef, AlexExposureSnapshot, AlexIntentStatus, AlexManifestRejection,
    AlexPolicyDecision, AlexPolicyRejection, AlexPrincipal, AlexQuoteSnapshot,
    AlexSettlementRequest, AlexSourceClass, AlexSourceSnapshot, AlexUnsignedSettlementIntent,
    AlexVenueManifest, ConxianError, ConxianResult, VerifiedAlexVenueManifest,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tracing::warn;

/// Compatibility path retained for observation only. Official REST
/// documentation does not establish this endpoint as a quote contract.
pub const ALEX_UNVERIFIED_QUOTE_PATH: &str = "/v1/quote";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlexQuoteStatus {
    UnverifiedEndpoint,
    Fixture,
    Observed,
}

/// Quote evidence returned by an ALEX client. Optional fields are deliberate:
/// compatibility clients must not invent price impact, timestamps, exposure,
/// or proof to make an incomplete response appear policy-ready.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlexQuoteObservation {
    pub amount_out: u128,
    pub source: AlexSourceClass,
    pub status: AlexQuoteStatus,
    pub endpoint: String,
    pub quoted_at_epoch_secs: Option<u64>,
    pub expires_at_epoch_secs: Option<u64>,
    pub observed_at_epoch_secs: Option<u64>,
    pub price_impact_bps: Option<u32>,
    pub exposure: Option<AlexExposureSnapshot>,
    pub exposure_source: Option<AlexSourceClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlexPair {
    pub token_x: String,
    pub token_y: String,
    pub contract_id: String,
}

#[async_trait]
pub trait AlexClient: Send + Sync {
    async fn get_swap_quote(&self, request: AlexSwapRequest) -> ConxianResult<u128>;

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
            quoted_at_epoch_secs: None,
            expires_at_epoch_secs: None,
            observed_at_epoch_secs: None,
            price_impact_bps: None,
            exposure: None,
            exposure_source: None,
        })
    }

    async fn execute_swap(
        &self,
        request: AlexSwapRequest,
        signer_key: &str,
    ) -> ConxianResult<String>;

    /// Raw construction accepts only an intent already approved by policy.
    async fn build_swap_payload(
        &self,
        intent: &AlexUnsignedSettlementIntent,
    ) -> ConxianResult<serde_json::Value>;
}

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
            let res = minreq::get(&url).send().map_err(|error| {
                ConxianError::Stacks(format!(
                    "ALEX unverified compatibility quote request failed: {error}"
                ))
            })?;
            if res.status_code != 200 {
                return Err(ConxianError::Stacks(format!(
                    "ALEX unverified compatibility quote returned status {}",
                    res.status_code
                )));
            }
            let body = res.as_str().map_err(|_| {
                ConxianError::Stacks(
                    "ALEX unverified compatibility quote body was unreadable".to_string(),
                )
            })?;
            let quote: AlexQuoteResponse = serde_json::from_str(body).map_err(|_| {
                ConxianError::Stacks(
                    "ALEX unverified compatibility quote schema did not match".to_string(),
                )
            })?;
            Ok(quote.swap_amount_out)
        })
        .await
        .map_err(|error| ConxianError::Internal(error.to_string()))??;

        Ok(AlexQuoteObservation {
            amount_out,
            source: AlexSourceClass::Unverified,
            status: AlexQuoteStatus::UnverifiedEndpoint,
            endpoint: ALEX_UNVERIFIED_QUOTE_PATH.to_string(),
            quoted_at_epoch_secs: None,
            expires_at_epoch_secs: None,
            observed_at_epoch_secs: None,
            price_impact_bps: None,
            exposure: None,
            exposure_source: None,
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
        intent: &AlexUnsignedSettlementIntent,
    ) -> ConxianResult<serde_json::Value> {
        if intent.decision != AlexPolicyDecision::AllowUnsigned
            || intent.status != AlexIntentStatus::UnsignedPrepared
        {
            return Err(ConxianError::Security(
                "ALEX unsigned payload rejected: policy did not allow preparation".to_string(),
            ));
        }
        let helper = &intent.helper.principal;
        Ok(json!({
            "network": intent.network.as_str(),
            "contract_address": helper.address(),
            "contract_name": helper.contract_name(),
            "helper_principal": helper.value,
            "function_name": "swap-helper",
            "status": "unsigned_preparation",
            "signing": "disabled",
            "broadcast": "disabled",
            "function_args": [
                { "type": "principal", "value": intent.asset_in.principal.value },
                { "type": "principal", "value": intent.asset_out.principal.value },
                { "type": "uint", "value": intent.amount_in.to_string() },
                { "type": "uint", "value": intent.min_dy.to_string() }
            ]
        }))
    }

    async fn execute_swap(
        &self,
        _request: AlexSwapRequest,
        _signer_key: &str,
    ) -> ConxianResult<String> {
        warn!("ALEX swap execution is disabled: signer and broadcast gates are closed");
        Err(ConxianError::Internal(
            "ALEX swap execution is disabled: no signer, broadcast, receipt, or reconciliation path is configured"
                .to_string(),
        ))
    }
}

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
            quoted_at_epoch_secs: None,
            expires_at_epoch_secs: None,
            observed_at_epoch_secs: None,
            price_impact_bps: None,
            exposure: None,
            exposure_source: None,
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
        _intent: &AlexUnsignedSettlementIntent,
    ) -> ConxianResult<serde_json::Value> {
        Err(ConxianError::Internal(
            "ALEX simulated client cannot produce an unsigned settlement payload".to_string(),
        ))
    }
}

#[derive(Debug, Error)]
pub enum AlexManifestLoadError {
    #[error("manifest file could not be read")]
    Read,
    #[error("manifest JSON is invalid")]
    Parse,
    #[error("manifest verification failed: {0}")]
    Rejected(AlexManifestRejection),
}

impl AlexManifestLoadError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Read => "ALEX_MANIFEST_READ_FAILED",
            Self::Parse => "ALEX_MANIFEST_INVALID_JSON",
            Self::Rejected(_) => "ALEX_MANIFEST_VERIFICATION_FAILED",
        }
    }
}

pub fn load_alex_venue_manifest(
    path: &Path,
    now_epoch_secs: u64,
) -> Result<VerifiedAlexVenueManifest, AlexManifestLoadError> {
    let bytes = std::fs::read(path).map_err(|_| AlexManifestLoadError::Read)?;
    let manifest: AlexVenueManifest =
        serde_json::from_slice(&bytes).map_err(|_| AlexManifestLoadError::Parse)?;
    manifest
        .verify_at(now_epoch_secs)
        .map_err(AlexManifestLoadError::Rejected)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AlexPreparedPayload {
    pub intent: AlexUnsignedSettlementIntent,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AlexPrepareError {
    #[error("ALEX venue manifest is unavailable")]
    ManifestUnavailable,
    #[error("ALEX venue manifest is invalid or stale")]
    ManifestInvalid,
    #[error("ALEX quote evidence is unavailable or stale")]
    EvidenceUnavailable,
    #[error("ALEX quote or exposure evidence is unverified")]
    VerificationRequired,
    #[error("ALEX policy rejected the request: {0}")]
    PolicyRejected(AlexPolicyRejection),
    #[error("ALEX amount must be nonzero")]
    InvalidAmount,
    #[error("ALEX min_dy must be present and nonzero")]
    InvalidMinimumOutput,
    #[error("ALEX unsigned payload construction failed")]
    PayloadConstruction,
}

pub struct AlexPreparationService {
    client: Arc<dyn AlexClient>,
    manifest: Option<VerifiedAlexVenueManifest>,
}

impl AlexPreparationService {
    pub fn new(client: Arc<dyn AlexClient>, manifest: Option<VerifiedAlexVenueManifest>) -> Self {
        Self { client, manifest }
    }

    pub fn disabled(client: Arc<dyn AlexClient>) -> Self {
        Self::new(client, None)
    }

    pub async fn prepare(
        &self,
        request: AlexSwapRequest,
        now_epoch_secs: u64,
    ) -> Result<AlexPreparedPayload, AlexPrepareError> {
        if request.amount == 0 {
            return Err(AlexPrepareError::InvalidAmount);
        }
        if request.min_dy.is_none() || request.min_dy == Some(0) {
            return Err(AlexPrepareError::InvalidMinimumOutput);
        }
        let verified = self
            .manifest
            .as_ref()
            .ok_or(AlexPrepareError::ManifestUnavailable)?;
        verified
            .verify_at(now_epoch_secs)
            .map_err(|_| AlexPrepareError::ManifestInvalid)?;
        let manifest = verified.manifest();

        let observation = self
            .client
            .get_swap_quote_observation(request.clone())
            .await
            .map_err(|_| AlexPrepareError::EvidenceUnavailable)?;
        if observation.source != AlexSourceClass::Observed
            || observation.exposure_source != Some(AlexSourceClass::Observed)
        {
            return Err(AlexPrepareError::VerificationRequired);
        }
        let quoted_at_epoch_secs = observation
            .quoted_at_epoch_secs
            .ok_or(AlexPrepareError::VerificationRequired)?;
        let expires_at_epoch_secs = observation
            .expires_at_epoch_secs
            .ok_or(AlexPrepareError::VerificationRequired)?;
        let observed_at_epoch_secs = observation
            .observed_at_epoch_secs
            .ok_or(AlexPrepareError::VerificationRequired)?;
        let price_impact_bps = observation
            .price_impact_bps
            .ok_or(AlexPrepareError::VerificationRequired)?;
        let exposure = observation
            .exposure
            .ok_or(AlexPrepareError::VerificationRequired)?;

        let asset_in = AlexAssetRef::new(
            AlexPrincipal::new(manifest.venue.network, request.token_x.as_str())
                .map_err(AlexPrepareError::PolicyRejected)?,
        );
        let asset_out = AlexAssetRef::new(
            AlexPrincipal::new(manifest.venue.network, request.token_y.as_str())
                .map_err(AlexPrepareError::PolicyRejected)?,
        );
        let policy_request = AlexSettlementRequest {
            network: manifest.venue.network,
            asset_in,
            asset_out,
            pool: manifest.venue.pool.clone(),
            helper: manifest.venue.helper.clone(),
            amount_in: request.amount,
            min_dy: request.min_dy,
            quote: AlexQuoteSnapshot {
                amount_in: request.amount,
                amount_out: observation.amount_out,
                quoted_at_epoch_secs,
                expires_at_epoch_secs,
                price_impact_bps,
                source: AlexSourceSnapshot {
                    classification: observation.source,
                    observed_at_epoch_secs,
                },
            },
            venue: manifest.venue.clone(),
            policy_revision: manifest.policy.policy_revision.clone(),
            requested_at_epoch_secs: now_epoch_secs,
            exposure,
        };

        let intent = manifest
            .policy
            .evaluate(&policy_request, now_epoch_secs)
            .map_err(|error| match error {
                AlexPolicyRejection::QuoteFromFuture
                | AlexPolicyRejection::QuoteExpired
                | AlexPolicyRejection::QuoteStale => AlexPrepareError::EvidenceUnavailable,
                other => AlexPrepareError::PolicyRejected(other),
            })?;
        if intent.decision != AlexPolicyDecision::AllowUnsigned {
            return Err(AlexPrepareError::VerificationRequired);
        }
        let payload = self
            .client
            .build_swap_payload(&intent)
            .await
            .map_err(|_| AlexPrepareError::PayloadConstruction)?;
        Ok(AlexPreparedPayload { intent, payload })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::{
        AlexAdminState, AlexHelperRef, AlexNetwork, AlexPoolRef, AlexSettlementPolicy,
        AlexVenueAllowlistEntry, AlexVenueSnapshot, ALEX_EXPOSURE_SAFETY_CEILING_BPS,
        ALEX_VENUE_MANIFEST_VERSION,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    const ASSET_IN: &str = "SP000000000000000000002Q6VF78.usdcx";
    const ASSET_OUT: &str = "SP000000000000000000002Q6VF78.sbtc";
    const POOL: &str = "SP000000000000000000002Q6VF78.alex-pool";
    const HELPER: &str = "SP000000000000000000002Q6VF78.alex-helper";

    fn principal(value: &str) -> AlexPrincipal {
        AlexPrincipal::new(AlexNetwork::Mainnet, value).unwrap()
    }

    fn manifest() -> VerifiedAlexVenueManifest {
        let venue = AlexVenueAllowlistEntry {
            asset_in: AlexAssetRef::new(principal(ASSET_IN)),
            asset_out: AlexAssetRef::new(principal(ASSET_OUT)),
            pool: AlexPoolRef::new(principal(POOL)),
            helper: AlexHelperRef::new(principal(HELPER)),
        };
        AlexVenueManifest {
            manifest_version: ALEX_VENUE_MANIFEST_VERSION.to_string(),
            manifest_id: "test-venue".to_string(),
            manifest_revision: "policy-r1".to_string(),
            valid_from_epoch_secs: 900,
            expires_at_epoch_secs: 2_000,
            venue: AlexVenueSnapshot {
                network: AlexNetwork::Mainnet,
                asset_in: venue.asset_in.clone(),
                asset_out: venue.asset_out.clone(),
                pool: venue.pool.clone(),
                helper: venue.helper.clone(),
                config_revision: "config-r1".to_string(),
                helper_code_hash: "hash-r1".to_string(),
                admin_state: AlexAdminState::KnownActive,
                source: AlexSourceSnapshot {
                    classification: AlexSourceClass::Observed,
                    observed_at_epoch_secs: 1_000,
                },
            },
            policy: AlexSettlementPolicy {
                policy_id: "policy".to_string(),
                policy_revision: "policy-r1".to_string(),
                supported_network: AlexNetwork::Mainnet,
                allowed_venues: vec![venue],
                expected_config_revision: "config-r1".to_string(),
                expected_helper_code_hash: "hash-r1".to_string(),
                max_quote_age_secs: 60,
                max_price_impact_bps: 500,
                max_exposure_bps: 2_000,
                exposure_safety_ceiling_bps: ALEX_EXPOSURE_SAFETY_CEILING_BPS,
            },
        }
        .verify_at(1_010)
        .unwrap()
    }

    fn request() -> AlexSwapRequest {
        AlexSwapRequest {
            token_x: ASSET_IN.to_string(),
            token_y: ASSET_OUT.to_string(),
            factor: 100_000_000,
            amount: 100,
            min_dy: Some(90),
        }
    }

    struct FakeClient {
        observation: AlexQuoteObservation,
        quote_calls: AtomicUsize,
        build_calls: AtomicUsize,
    }

    impl FakeClient {
        fn observed() -> Self {
            Self {
                observation: AlexQuoteObservation {
                    amount_out: 100,
                    source: AlexSourceClass::Observed,
                    status: AlexQuoteStatus::Observed,
                    endpoint: "test-observer".to_string(),
                    quoted_at_epoch_secs: Some(1_000),
                    expires_at_epoch_secs: Some(1_030),
                    observed_at_epoch_secs: Some(1_000),
                    price_impact_bps: Some(100),
                    exposure: Some(AlexExposureSnapshot {
                        before: 0,
                        after: 10,
                        cap: 100,
                    }),
                    exposure_source: Some(AlexSourceClass::Observed),
                },
                quote_calls: AtomicUsize::new(0),
                build_calls: AtomicUsize::new(0),
            }
        }

        fn unverified() -> Self {
            let mut client = Self::observed();
            client.observation.source = AlexSourceClass::Unverified;
            client.observation.status = AlexQuoteStatus::UnverifiedEndpoint;
            client.observation.price_impact_bps = None;
            client.observation.exposure = None;
            client.observation.exposure_source = None;
            client
        }
    }

    #[async_trait]
    impl AlexClient for FakeClient {
        async fn get_swap_quote(&self, _request: AlexSwapRequest) -> ConxianResult<u128> {
            Ok(self.observation.amount_out)
        }

        async fn get_swap_quote_observation(
            &self,
            _request: AlexSwapRequest,
        ) -> ConxianResult<AlexQuoteObservation> {
            self.quote_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.observation.clone())
        }

        async fn execute_swap(
            &self,
            _request: AlexSwapRequest,
            _signer_key: &str,
        ) -> ConxianResult<String> {
            unreachable!()
        }

        async fn build_swap_payload(
            &self,
            intent: &AlexUnsignedSettlementIntent,
        ) -> ConxianResult<serde_json::Value> {
            self.build_calls.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"intent_hash": intent.intent_hash}))
        }
    }

    #[tokio::test]
    async fn no_manifest_disables_prepare_before_quote_or_payload() {
        let client = Arc::new(FakeClient::observed());
        let service = AlexPreparationService::disabled(client.clone());
        assert_eq!(
            service.prepare(request(), 1_010).await.unwrap_err(),
            AlexPrepareError::ManifestUnavailable
        );
        assert_eq!(client.quote_calls.load(Ordering::SeqCst), 0);
        assert_eq!(client.build_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn unverified_quote_denies_without_raw_payload_construction() {
        let client = Arc::new(FakeClient::unverified());
        let service = AlexPreparationService::new(client.clone(), Some(manifest()));
        assert_eq!(
            service.prepare(request(), 1_010).await.unwrap_err(),
            AlexPrepareError::VerificationRequired
        );
        assert_eq!(client.build_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn policy_rejection_happens_before_raw_payload_construction() {
        let client = Arc::new(FakeClient::observed());
        let service = AlexPreparationService::new(client.clone(), Some(manifest()));
        let mut wrong = request();
        wrong.token_x = "SP000000000000000000002Q6VF78.other".to_string();
        assert!(matches!(
            service.prepare(wrong, 1_010).await,
            Err(AlexPrepareError::PolicyRejected(_))
        ));
        assert_eq!(client.build_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn fully_observed_fake_reaches_policy_gated_unsigned_payload() {
        let client = Arc::new(FakeClient::observed());
        let service = AlexPreparationService::new(client.clone(), Some(manifest()));
        let prepared = service.prepare(request(), 1_010).await.unwrap();
        assert_eq!(prepared.intent.decision, AlexPolicyDecision::AllowUnsigned);
        assert_eq!(prepared.intent.status, AlexIntentStatus::UnsignedPrepared);
        assert_eq!(client.build_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn simulated_client_never_returns_receipt_or_unsigned_payload() {
        let client = SimulatedAlexClient;
        assert!(client.execute_swap(request(), "ignored").await.is_err());
        let observation = client.get_swap_quote_observation(request()).await.unwrap();
        assert_eq!(observation.source, AlexSourceClass::Fixture);
        assert_eq!(observation.status, AlexQuoteStatus::Fixture);
    }
}
