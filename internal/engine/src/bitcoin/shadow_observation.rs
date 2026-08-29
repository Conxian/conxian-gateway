use async_trait::async_trait;
use conxian_core::{ConxianError, ConxianResult};
use reqwest::{redirect::Policy, Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tracing::warn;

pub const SHADOW_FEE_TARGETS: [u16; 3] = [2, 6, 12];
const SHADOW_RPC_ID: &str = "conxian-shadow-observer";
const SAT_PER_BTC_PER_KVB: f64 = 100_000.0;
const BLOCK_STATS_FIELDS: [&str; 9] = [
    "height",
    "blockhash",
    "txs",
    "totalfee",
    "total_weight",
    "minfeerate",
    "avgfeerate",
    "maxfeerate",
    "feerate_percentiles",
];
const DEPLOYMENT_ALIASES: [(&str, DeploymentAlias); 2] = [
    ("reduced_data", DeploymentAlias::ReducedData),
    ("bip110", DeploymentAlias::Bip110),
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationAvailability {
    Observed,
    Unavailable,
    DependencyUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationErrorCategory {
    Transport,
    Rpc,
    InvalidResponse,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceObservation<T> {
    pub availability: ObservationAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_category: Option<ObservationErrorCategory>,
    /// Unix timestamp (seconds) when this observation was captured
    pub observed_at_unix: u64,
}

impl<T> SourceObservation<T> {
    fn observed(data: T) -> Self {
        Self {
            availability: ObservationAvailability::Observed,
            data: Some(data),
            error_category: None,
            observed_at_unix: now_unix(),
        }
    }

    fn unavailable(category: ObservationErrorCategory) -> Self {
        Self {
            availability: ObservationAvailability::Unavailable,
            data: None,
            error_category: Some(category),
            observed_at_unix: now_unix(),
        }
    }

    fn dependency_unavailable() -> Self {
        Self {
            availability: ObservationAvailability::DependencyUnavailable,
            data: None,
            error_category: None,
            observed_at_unix: now_unix(),
        }
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoreNetworkInfo {
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoreBlockchainInfo {
    pub chain: String,
    pub tip_height: u64,
    pub best_block_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeeEstimateAvailability {
    Observed,
    NoEstimate,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct FeeEstimateObservation {
    pub target_blocks: u16,
    pub unit: FeeRateUnit,
    pub availability: FeeEstimateAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_rate_sat_vb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_category: Option<ObservationErrorCategory>,
    /// Unix timestamp (seconds) when this fee estimate was captured
    pub observed_at_unix: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeeRateUnit {
    SatPerVbyte,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreMempoolInfo {
    pub transaction_count: u64,
    pub virtual_size_bytes: u64,
    pub memory_usage_bytes: u64,
    pub max_mempool_bytes: u64,
    pub mempool_min_fee_sat_vb: f64,
    pub min_relay_tx_fee_sat_vb: f64,
    pub unbroadcast_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreBestBlockStats {
    pub height: u64,
    pub block_hash: String,
    pub transaction_count: u64,
    pub total_fees_sat: u64,
    pub total_weight: u64,
    pub min_fee_rate_sat_vb: f64,
    pub average_fee_rate_sat_vb: f64,
    pub max_fee_rate_sat_vb: f64,
    pub fee_rate_percentiles_sat_vb: [f64; 5],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentObservationStatus {
    Observed,
    NotExposed,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentAlias {
    ReducedData,
    Bip110,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentReportedState {
    Defined,
    Started,
    LockedIn,
    Active,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentSourceScope {
    BitcoinCoreConfiguredEndpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentObservation {
    pub source_scope: DeploymentSourceScope,
    pub status: DeploymentObservationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_alias: Option<DeploymentAlias>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reported_state: Option<DeploymentReportedState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_category: Option<ObservationErrorCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Tracks the confidence and calibration metadata for shadow-observed data
/// used in fee-bump decisions, route selection, and BIP-110 deployment tracking.
///
/// See [#245](https://github.com/Conxian/conxian-gateway/issues/245).
pub struct RouteConfidence {
    /// Unix timestamp (seconds) of when the observation was captured
    pub observed_at_unix: u64,
    /// How fresh the fee-estimate source is, in milliseconds since observation
    pub source_freshness_ms: u64,
    /// Fraction of advertised mempool / network peers covered (0.0–1.0).
    /// Always 1.0 for a single, directly-configured Bitcoin Core node.
    pub coverage_fraction: f64,
    /// Optional calibration metadata (source version, network, peer count)
    pub calibration: Option<CalibrationMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationMeta {
    /// Bitcoin Core version string (e.g. "280000")
    pub version: String,
    /// Network name (e.g. "main", "test", "signet")
    pub network: String,
    /// Number of peers connected at observation time
    pub connections: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitcoinCoreShadowObservation {
    pub network_info: SourceObservation<CoreNetworkInfo>,
    pub blockchain_info: SourceObservation<CoreBlockchainInfo>,
    pub fee_estimates: [FeeEstimateObservation; 3],
    pub mempool_info: SourceObservation<CoreMempoolInfo>,
    pub best_block_stats: SourceObservation<CoreBestBlockStats>,
    pub deployment: DeploymentObservation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_confidence: Option<RouteConfidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadowObserverFailure {
    pub category: ObservationErrorCategory,
}

#[async_trait]
pub trait BitcoinCoreShadowObserver: Send + Sync {
    async fn observe(&self) -> Result<BitcoinCoreShadowObservation, ShadowObserverFailure>;
}

#[derive(Clone, Copy)]
struct ShadowTransportLimits {
    connect_timeout: Duration,
    read_timeout: Duration,
    request_timeout: Duration,
    max_response_body_bytes: usize,
}

const PRODUCTION_TRANSPORT_LIMITS: ShadowTransportLimits = ShadowTransportLimits {
    connect_timeout: Duration::from_millis(500),
    read_timeout: Duration::from_secs(1),
    request_timeout: Duration::from_secs(2),
    max_response_body_bytes: 256 * 1024,
};

struct BasicAuth {
    username: String,
    password: String,
}

/// Read-only, observer-specific Bitcoin Core JSON-RPC transport.
///
/// This remains separate from the action-capable client so cancellation never
/// leaves blocking Bitcoin Core work running.
pub struct BitcoinCoreShadowObserverClient {
    client: Client,
    endpoint: Url,
    auth: Option<BasicAuth>,
    limits: ShadowTransportLimits,
    admission: Arc<Semaphore>,
}

impl BitcoinCoreShadowObserverClient {
    pub fn new(url: &str, user: &str, pass: &str) -> ConxianResult<Self> {
        Self::build(url, user, pass, PRODUCTION_TRANSPORT_LIMITS)
    }

    #[cfg(test)]
    fn new_with_transport_limits(
        url: &str,
        user: &str,
        pass: &str,
        limits: ShadowTransportLimits,
    ) -> ConxianResult<Self> {
        Self::build(url, user, pass, limits)
    }

    fn build(
        url: &str,
        user: &str,
        pass: &str,
        limits: ShadowTransportLimits,
    ) -> ConxianResult<Self> {
        let url = url.trim();
        let user = user.trim();
        if url.is_empty() {
            return Err(ConxianError::Bitcoin(
                "Invalid Bitcoin RPC URL: URL is empty".to_string(),
            ));
        }
        let endpoint = Url::parse(url)
            .map_err(|_| ConxianError::Bitcoin("Invalid Bitcoin RPC URL".to_string()))?;
        let auth = match (user.is_empty(), pass.is_empty()) {
            (true, true) => None,
            (false, false) => Some(BasicAuth {
                username: user.to_string(),
                password: pass.to_string(),
            }),
            (true, false) => {
                return Err(ConxianError::Bitcoin(
                    "Invalid Bitcoin RPC auth: password set but username is empty".to_string(),
                ));
            }
            (false, true) => {
                return Err(ConxianError::Bitcoin(
                    "Invalid Bitcoin RPC auth: username set but password is empty".to_string(),
                ));
            }
        };
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(limits.connect_timeout)
            .read_timeout(limits.read_timeout)
            .timeout(limits.request_timeout)
            .build()
            .map_err(|_| {
                ConxianError::Bitcoin("Failed to initialize Bitcoin RPC observer".to_string())
            })?;
        Ok(Self {
            client,
            endpoint,
            auth,
            limits,
            admission: Arc::new(Semaphore::new(1)),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RpcCallFailure {
    MethodNotFound,
    Category(ObservationErrorCategory),
}

impl BitcoinCoreShadowObserverClient {
    async fn shadow_rpc_call(
        &self,
        source: &'static str,
        method: &'static str,
        params: Vec<Value>,
    ) -> Result<Value, RpcCallFailure> {
        let request = json!({
            "jsonrpc": "1.0",
            "id": SHADOW_RPC_ID,
            "method": method,
            "params": params,
        });
        let mut request_builder = self.client.post(self.endpoint.clone()).json(&request);
        if let Some(auth) = &self.auth {
            request_builder = request_builder.basic_auth(&auth.username, Some(&auth.password));
        }
        let response = request_builder.send().await.map_err(|_| {
            log_source_failure(source, ObservationErrorCategory::Transport);
            RpcCallFailure::Category(ObservationErrorCategory::Transport)
        })?;
        let status = response.status();
        let body = self.read_bounded_body(source, response).await?;
        parse_rpc_response(status, &body).inspect_err(|failure| {
            if let RpcCallFailure::Category(category) = *failure {
                log_source_failure(source, category);
            }
        })
    }

    async fn read_bounded_body(
        &self,
        source: &'static str,
        mut response: reqwest::Response,
    ) -> Result<Vec<u8>, RpcCallFailure> {
        if response
            .content_length()
            .is_some_and(|length| length > self.limits.max_response_body_bytes as u64)
        {
            log_source_failure(source, ObservationErrorCategory::InvalidResponse);
            return Err(RpcCallFailure::Category(
                ObservationErrorCategory::InvalidResponse,
            ));
        }

        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            log_source_failure(source, ObservationErrorCategory::Transport);
            RpcCallFailure::Category(ObservationErrorCategory::Transport)
        })? {
            let next_len = body
                .len()
                .checked_add(chunk.len())
                .ok_or(RpcCallFailure::Category(
                    ObservationErrorCategory::InvalidResponse,
                ))?;
            if next_len > self.limits.max_response_body_bytes {
                log_source_failure(source, ObservationErrorCategory::InvalidResponse);
                return Err(RpcCallFailure::Category(
                    ObservationErrorCategory::InvalidResponse,
                ));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    async fn observe_network_info(&self) -> SourceObservation<CoreNetworkInfo> {
        match self
            .shadow_rpc_call("getnetworkinfo", "getnetworkinfo", vec![])
            .await
        {
            Ok(value) => parse_network_info(&value).unwrap_or_else(|category| {
                log_source_failure("getnetworkinfo", category);
                SourceObservation::unavailable(category)
            }),
            Err(failure) => SourceObservation::unavailable(failure.category()),
        }
    }

    async fn observe_blockchain_info(&self) -> SourceObservation<CoreBlockchainInfo> {
        match self
            .shadow_rpc_call("getblockchaininfo", "getblockchaininfo", vec![])
            .await
        {
            Ok(value) => parse_blockchain_info(&value).unwrap_or_else(|category| {
                log_source_failure("getblockchaininfo", category);
                SourceObservation::unavailable(category)
            }),
            Err(failure) => SourceObservation::unavailable(failure.category()),
        }
    }

    async fn observe_fee_estimate(&self, target_blocks: u16) -> FeeEstimateObservation {
        match self
            .shadow_rpc_call(
                "estimatesmartfee",
                "estimatesmartfee",
                vec![json!(target_blocks)],
            )
            .await
        {
            Ok(value) => parse_fee_estimate(target_blocks, &value).unwrap_or_else(|category| {
                log_source_failure("estimatesmartfee", category);
                unavailable_fee_estimate(target_blocks, category)
            }),
            Err(failure) => unavailable_fee_estimate(target_blocks, failure.category()),
        }
    }

    async fn observe_mempool_info(&self) -> SourceObservation<CoreMempoolInfo> {
        match self
            .shadow_rpc_call("getmempoolinfo", "getmempoolinfo", vec![])
            .await
        {
            Ok(value) => parse_mempool_info(&value).unwrap_or_else(|category| {
                log_source_failure("getmempoolinfo", category);
                SourceObservation::unavailable(category)
            }),
            Err(failure) => SourceObservation::unavailable(failure.category()),
        }
    }

    async fn observe_block_stats(
        &self,
        blockchain: &SourceObservation<CoreBlockchainInfo>,
    ) -> SourceObservation<CoreBestBlockStats> {
        let Some(blockchain) = blockchain.data.as_ref() else {
            return SourceObservation::dependency_unavailable();
        };
        let block_hash = blockchain.best_block_hash.clone();
        match self
            .shadow_rpc_call(
                "getblockstats",
                "getblockstats",
                vec![json!(block_hash), json!(BLOCK_STATS_FIELDS)],
            )
            .await
        {
            Ok(value) => {
                parse_block_stats(&value, &blockchain.best_block_hash).unwrap_or_else(|category| {
                    log_source_failure("getblockstats", category);
                    SourceObservation::unavailable(category)
                })
            }
            Err(failure) => SourceObservation::unavailable(failure.category()),
        }
    }

    async fn observe_deployment(&self) -> DeploymentObservation {
        deployment_from_rpc_result(
            self.shadow_rpc_call("getdeploymentinfo", "getdeploymentinfo", vec![])
                .await,
        )
    }
}

fn deployment_from_rpc_result(result: Result<Value, RpcCallFailure>) -> DeploymentObservation {
    match result {
        Ok(value) => parse_deployment_info(&value),
        Err(RpcCallFailure::MethodNotFound) => DeploymentObservation {
            source_scope: DeploymentSourceScope::BitcoinCoreConfiguredEndpoint,
            status: DeploymentObservationStatus::Unsupported,
            observed_alias: None,
            reported_state: None,
            error_category: None,
        },
        Err(RpcCallFailure::Category(category)) => DeploymentObservation {
            source_scope: DeploymentSourceScope::BitcoinCoreConfiguredEndpoint,
            status: DeploymentObservationStatus::Unknown,
            observed_alias: None,
            reported_state: None,
            error_category: Some(category),
        },
    }
}

#[async_trait]
impl BitcoinCoreShadowObserver for BitcoinCoreShadowObserverClient {
    async fn observe(&self) -> Result<BitcoinCoreShadowObservation, ShadowObserverFailure> {
        let _permit =
            self.admission
                .clone()
                .try_acquire_owned()
                .map_err(|_| ShadowObserverFailure {
                    category: ObservationErrorCategory::Transport,
                })?;
        let (
            network_info,
            blockchain_info,
            estimate_2,
            estimate_6,
            estimate_12,
            mempool_info,
            deployment,
        ) = tokio::join!(
            self.observe_network_info(),
            self.observe_blockchain_info(),
            self.observe_fee_estimate(SHADOW_FEE_TARGETS[0]),
            self.observe_fee_estimate(SHADOW_FEE_TARGETS[1]),
            self.observe_fee_estimate(SHADOW_FEE_TARGETS[2]),
            self.observe_mempool_info(),
            self.observe_deployment(),
        );
        let best_block_stats = self.observe_block_stats(&blockchain_info).await;

        Ok(BitcoinCoreShadowObservation {
            network_info: network_info.clone(),
            blockchain_info: blockchain_info.clone(),
            fee_estimates: [estimate_2, estimate_6, estimate_12],
            mempool_info,
            best_block_stats,
            deployment,
            route_confidence: Some(RouteConfidence {
                observed_at_unix: network_info.observed_at_unix,
                source_freshness_ms: estimate_2
                    .observed_at_unix
                    .saturating_sub(network_info.observed_at_unix)
                    .saturating_mul(1000),
                coverage_fraction: 1.0,
                calibration: Some(CalibrationMeta {
                    version: network_info
                        .data
                        .as_ref()
                        .map(|n| n.version.to_string())
                        .unwrap_or_default(),
                    network: blockchain_info
                        .data
                        .as_ref()
                        .map(|b| b.chain.clone())
                        .unwrap_or_default(),
                    connections: 0,
                }),
            }),
        })
    }
}

impl RpcCallFailure {
    fn category(self) -> ObservationErrorCategory {
        match self {
            Self::MethodNotFound | Self::Category(ObservationErrorCategory::Rpc) => {
                ObservationErrorCategory::Rpc
            }
            Self::Category(category) => category,
        }
    }
}

fn parse_rpc_response(status: StatusCode, body: &[u8]) -> Result<Value, RpcCallFailure> {
    let envelope: Value = serde_json::from_slice(body)
        .map_err(|_| RpcCallFailure::Category(ObservationErrorCategory::InvalidResponse))?;
    let envelope = envelope.as_object().ok_or(RpcCallFailure::Category(
        ObservationErrorCategory::InvalidResponse,
    ))?;
    if envelope.get("id").and_then(Value::as_str) != Some(SHADOW_RPC_ID) {
        return Err(RpcCallFailure::Category(
            ObservationErrorCategory::InvalidResponse,
        ));
    }
    if let Some(error) = envelope.get("error").filter(|error| !error.is_null()) {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .ok_or(RpcCallFailure::Category(
                ObservationErrorCategory::InvalidResponse,
            ))?;
        return if code == -32601 {
            Err(RpcCallFailure::MethodNotFound)
        } else {
            Err(RpcCallFailure::Category(ObservationErrorCategory::Rpc))
        };
    }
    if !status.is_success() {
        return Err(RpcCallFailure::Category(ObservationErrorCategory::Rpc));
    }
    envelope
        .get("result")
        .cloned()
        .ok_or(RpcCallFailure::Category(
            ObservationErrorCategory::InvalidResponse,
        ))
}

fn log_source_failure(source: &'static str, category: ObservationErrorCategory) {
    warn!(source, category = ?category, "Bitcoin Core shadow observation source unavailable");
}

fn parse_network_info(
    value: &Value,
) -> Result<SourceObservation<CoreNetworkInfo>, ObservationErrorCategory> {
    let object = value
        .as_object()
        .ok_or(ObservationErrorCategory::InvalidResponse)?;
    let version = parse_u64(object.get("version"))?;
    Ok(SourceObservation::observed(CoreNetworkInfo { version }))
}

fn parse_blockchain_info(
    value: &Value,
) -> Result<SourceObservation<CoreBlockchainInfo>, ObservationErrorCategory> {
    let object = value
        .as_object()
        .ok_or(ObservationErrorCategory::InvalidResponse)?;
    let chain = parse_bounded_string(object.get("chain"), 32)?;
    let tip_height = parse_u64(object.get("blocks"))?;
    let best_block_hash = parse_block_hash(object.get("bestblockhash"))?;
    Ok(SourceObservation::observed(CoreBlockchainInfo {
        chain,
        tip_height,
        best_block_hash,
    }))
}

fn parse_fee_estimate(
    target_blocks: u16,
    value: &Value,
) -> Result<FeeEstimateObservation, ObservationErrorCategory> {
    let object = value
        .as_object()
        .ok_or(ObservationErrorCategory::InvalidResponse)?;
    let Some(raw_fee_rate) = object.get("feerate") else {
        return Ok(no_fee_estimate(target_blocks));
    };
    if raw_fee_rate.is_null() {
        return Ok(no_fee_estimate(target_blocks));
    }
    let btc_per_kvb = parse_non_negative_f64(Some(raw_fee_rate))?;
    let fee_rate_sat_vb = btc_per_kvb * SAT_PER_BTC_PER_KVB;
    if !fee_rate_sat_vb.is_finite() || fee_rate_sat_vb < 0.0 {
        return Err(ObservationErrorCategory::InvalidResponse);
    }
    Ok(FeeEstimateObservation {
        target_blocks,
        unit: FeeRateUnit::SatPerVbyte,
        availability: FeeEstimateAvailability::Observed,
        fee_rate_sat_vb: Some(fee_rate_sat_vb),
        error_category: None,
        observed_at_unix: now_unix(),
    })
}

fn no_fee_estimate(target_blocks: u16) -> FeeEstimateObservation {
    FeeEstimateObservation {
        target_blocks,
        unit: FeeRateUnit::SatPerVbyte,
        availability: FeeEstimateAvailability::NoEstimate,
        fee_rate_sat_vb: None,
        error_category: None,
        observed_at_unix: now_unix(),
    }
}

fn unavailable_fee_estimate(
    target_blocks: u16,
    category: ObservationErrorCategory,
) -> FeeEstimateObservation {
    FeeEstimateObservation {
        target_blocks,
        unit: FeeRateUnit::SatPerVbyte,
        availability: FeeEstimateAvailability::Unavailable,
        fee_rate_sat_vb: None,
        error_category: Some(category),
        observed_at_unix: now_unix(),
    }
}

fn parse_mempool_info(
    value: &Value,
) -> Result<SourceObservation<CoreMempoolInfo>, ObservationErrorCategory> {
    let object = value
        .as_object()
        .ok_or(ObservationErrorCategory::InvalidResponse)?;
    Ok(SourceObservation::observed(CoreMempoolInfo {
        transaction_count: parse_u64(object.get("size"))?,
        virtual_size_bytes: parse_u64(object.get("bytes"))?,
        memory_usage_bytes: parse_u64(object.get("usage"))?,
        max_mempool_bytes: parse_u64(object.get("maxmempool"))?,
        mempool_min_fee_sat_vb: btc_per_kvb_to_sat_vb(object.get("mempoolminfee"))?,
        min_relay_tx_fee_sat_vb: btc_per_kvb_to_sat_vb(object.get("minrelaytxfee"))?,
        unbroadcast_count: parse_u64(object.get("unbroadcastcount"))?,
    }))
}

fn parse_block_stats(
    value: &Value,
    expected_hash: &str,
) -> Result<SourceObservation<CoreBestBlockStats>, ObservationErrorCategory> {
    let object = value
        .as_object()
        .ok_or(ObservationErrorCategory::InvalidResponse)?;
    let block_hash = parse_block_hash(object.get("blockhash"))?;
    if block_hash != expected_hash {
        return Err(ObservationErrorCategory::InvalidResponse);
    }
    let percentiles = object
        .get("feerate_percentiles")
        .and_then(Value::as_array)
        .ok_or(ObservationErrorCategory::InvalidResponse)?;
    if percentiles.len() != 5 {
        return Err(ObservationErrorCategory::InvalidResponse);
    }
    let fee_rate_percentiles_sat_vb = [
        parse_non_negative_f64(percentiles.first())?,
        parse_non_negative_f64(percentiles.get(1))?,
        parse_non_negative_f64(percentiles.get(2))?,
        parse_non_negative_f64(percentiles.get(3))?,
        parse_non_negative_f64(percentiles.get(4))?,
    ];
    Ok(SourceObservation::observed(CoreBestBlockStats {
        height: parse_u64(object.get("height"))?,
        block_hash,
        transaction_count: parse_u64(object.get("txs"))?,
        total_fees_sat: parse_u64(object.get("totalfee"))?,
        total_weight: parse_u64(object.get("total_weight"))?,
        min_fee_rate_sat_vb: parse_non_negative_f64(object.get("minfeerate"))?,
        average_fee_rate_sat_vb: parse_non_negative_f64(object.get("avgfeerate"))?,
        max_fee_rate_sat_vb: parse_non_negative_f64(object.get("maxfeerate"))?,
        fee_rate_percentiles_sat_vb,
    }))
}

fn parse_deployment_info(value: &Value) -> DeploymentObservation {
    let Some(object) = value.as_object() else {
        return unknown_deployment(ObservationErrorCategory::InvalidResponse);
    };
    let Some(deployments) = object.get("deployments").and_then(Value::as_object) else {
        return unknown_deployment(ObservationErrorCategory::InvalidResponse);
    };

    for (key, alias) in DEPLOYMENT_ALIASES {
        let Some(raw_deployment) = deployments.get(key) else {
            continue;
        };
        let Some(deployment) = raw_deployment.as_object() else {
            return unknown_deployment(ObservationErrorCategory::InvalidResponse);
        };
        let reported_state = deployment
            .get("bip9")
            .and_then(Value::as_object)
            .and_then(|bip9| bip9.get("status"))
            .and_then(Value::as_str)
            .and_then(parse_reported_state);
        return DeploymentObservation {
            source_scope: DeploymentSourceScope::BitcoinCoreConfiguredEndpoint,
            status: DeploymentObservationStatus::Observed,
            observed_alias: Some(alias),
            reported_state,
            error_category: None,
        };
    }

    DeploymentObservation {
        source_scope: DeploymentSourceScope::BitcoinCoreConfiguredEndpoint,
        status: DeploymentObservationStatus::NotExposed,
        observed_alias: None,
        reported_state: None,
        error_category: None,
    }
}

fn unknown_deployment(category: ObservationErrorCategory) -> DeploymentObservation {
    DeploymentObservation {
        source_scope: DeploymentSourceScope::BitcoinCoreConfiguredEndpoint,
        status: DeploymentObservationStatus::Unknown,
        observed_alias: None,
        reported_state: None,
        error_category: Some(category),
    }
}

fn parse_reported_state(raw: &str) -> Option<DeploymentReportedState> {
    match raw {
        "defined" => Some(DeploymentReportedState::Defined),
        "started" => Some(DeploymentReportedState::Started),
        "locked_in" => Some(DeploymentReportedState::LockedIn),
        "active" => Some(DeploymentReportedState::Active),
        "failed" => Some(DeploymentReportedState::Failed),
        _ => None,
    }
}

fn parse_u64(value: Option<&Value>) -> Result<u64, ObservationErrorCategory> {
    value
        .and_then(Value::as_u64)
        .ok_or(ObservationErrorCategory::InvalidResponse)
}

fn parse_non_negative_f64(value: Option<&Value>) -> Result<f64, ObservationErrorCategory> {
    let value = value
        .and_then(Value::as_f64)
        .ok_or(ObservationErrorCategory::InvalidResponse)?;
    validate_non_negative_f64(value)
}

fn validate_non_negative_f64(value: f64) -> Result<f64, ObservationErrorCategory> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(ObservationErrorCategory::InvalidResponse)
    }
}

fn btc_per_kvb_to_sat_vb(value: Option<&Value>) -> Result<f64, ObservationErrorCategory> {
    let converted = parse_non_negative_f64(value)? * SAT_PER_BTC_PER_KVB;
    validate_non_negative_f64(converted)
}

fn parse_bounded_string(
    value: Option<&Value>,
    max_len: usize,
) -> Result<String, ObservationErrorCategory> {
    let value = value
        .and_then(Value::as_str)
        .ok_or(ObservationErrorCategory::InvalidResponse)?;
    if value.is_empty() || value.len() > max_len || !value.is_ascii() {
        return Err(ObservationErrorCategory::InvalidResponse);
    }
    Ok(value.to_string())
}

fn parse_block_hash(value: Option<&Value>) -> Result<String, ObservationErrorCategory> {
    let hash = parse_bounded_string(value, 64)?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ObservationErrorCategory::InvalidResponse);
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        },
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::{mpsc, Notify},
        task::JoinHandle,
        time::{sleep, timeout, Instant},
    };

    const HASH: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    struct CapturedRequest {
        headers: HashMap<String, String>,
        body: Value,
    }

    fn test_limits(
        request_timeout: Duration,
        max_response_body_bytes: usize,
    ) -> ShadowTransportLimits {
        ShadowTransportLimits {
            connect_timeout: Duration::from_millis(50),
            read_timeout: request_timeout,
            request_timeout,
            max_response_body_bytes,
        }
    }

    fn expected_test_basic_auth(username: &str, password: &str) -> String {
        format!(
            "Basic {}",
            STANDARD.encode(format!("{username}:{password}"))
        )
    }

    async fn read_request(stream: &mut TcpStream) -> CapturedRequest {
        let mut received = Vec::new();
        let header_end = loop {
            let mut chunk = [0_u8; 1024];
            let read = stream.read(&mut chunk).await.unwrap();
            assert!(read > 0, "request closed before headers completed");
            received.extend_from_slice(&chunk[..read]);
            assert!(
                received.len() <= 32 * 1024,
                "request headers are unexpectedly large"
            );
            if let Some(position) = received.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let header_text = std::str::from_utf8(&received[..header_end]).unwrap();
        let mut lines = header_text.split("\r\n");
        assert!(lines.next().unwrap().starts_with("POST "));
        let headers: HashMap<String, String> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
            .collect();
        let content_length = headers
            .get("content-length")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        while received.len() - header_end < content_length {
            let mut chunk = [0_u8; 1024];
            let read = stream.read(&mut chunk).await.unwrap();
            assert!(read > 0, "request closed before body completed");
            received.extend_from_slice(&chunk[..read]);
        }
        let body =
            serde_json::from_slice(&received[header_end..header_end + content_length]).unwrap();
        CapturedRequest { headers, body }
    }

    fn rpc_result(method: &str) -> Value {
        let result = match method {
            "getnetworkinfo" => json!({ "version": 290000 }),
            "getblockchaininfo" => json!({
                "chain": "regtest",
                "blocks": 101,
                "bestblockhash": HASH
            }),
            "estimatesmartfee" => json!({ "feerate": 0.00002 }),
            "getmempoolinfo" => json!({
                "size": 3,
                "bytes": 900,
                "usage": 1200,
                "maxmempool": 300000000,
                "mempoolminfee": 0.00001,
                "minrelaytxfee": 0.00001,
                "unbroadcastcount": 0
            }),
            "getdeploymentinfo" => json!({ "deployments": {} }),
            "getblockstats" => json!({
                "height": 101,
                "blockhash": HASH,
                "txs": 2,
                "totalfee": 1200,
                "total_weight": 800,
                "minfeerate": 1.0,
                "avgfeerate": 2.0,
                "maxfeerate": 3.0,
                "feerate_percentiles": [1.0, 1.5, 2.0, 2.5, 3.0]
            }),
            other => panic!("unexpected method: {other}"),
        };
        json!({
            "result": result,
            "error": null,
            "id": "conxian-shadow-observer"
        })
    }

    async fn write_json_response(stream: &mut TcpStream, status: &str, body: &Value) {
        let body = serde_json::to_vec(body).unwrap();
        let headers = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        if stream.write_all(headers.as_bytes()).await.is_err() {
            return;
        }
        if stream.write_all(&body).await.is_err() {
            return;
        }
        let _ = stream.shutdown().await;
    }

    async fn spawn_standard_server(
        expected_requests: usize,
    ) -> (String, mpsc::Receiver<CapturedRequest>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (requests_tx, requests_rx) = mpsc::channel(expected_requests);
        let handle = tokio::spawn(async move {
            let mut handlers = Vec::new();
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().await.unwrap();
                let requests_tx = requests_tx.clone();
                handlers.push(tokio::spawn(async move {
                    let request = read_request(&mut stream).await;
                    let method = request.body["method"].as_str().unwrap().to_string();
                    requests_tx.send(request).await.unwrap();
                    write_json_response(&mut stream, "200 OK", &rpc_result(&method)).await;
                }));
            }
            for handler in handlers {
                handler.await.unwrap();
            }
        });
        (url, requests_rx, handle)
    }

    #[test]
    fn fee_estimate_converts_btc_per_kvb_and_preserves_no_estimate() {
        let estimate = parse_fee_estimate(2, &json!({ "feerate": 0.00025 })).unwrap();
        assert_eq!(estimate.fee_rate_sat_vb, Some(25.0));
        assert_eq!(estimate.unit, FeeRateUnit::SatPerVbyte);

        for value in [json!({}), json!({ "feerate": null })] {
            let estimate = parse_fee_estimate(6, &value).unwrap();
            assert_eq!(estimate.availability, FeeEstimateAvailability::NoEstimate);
            assert_eq!(estimate.fee_rate_sat_vb, None);
        }
    }

    #[test]
    fn fee_estimate_rejects_negative_and_non_finite_values() {
        assert_eq!(
            parse_fee_estimate(2, &json!({ "feerate": -0.1 })),
            Err(ObservationErrorCategory::InvalidResponse)
        );
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                validate_non_negative_f64(value),
                Err(ObservationErrorCategory::InvalidResponse)
            );
        }
    }

    #[test]
    fn block_stats_requires_exact_hash_and_five_valid_percentiles() {
        let valid = json!({
            "height": 900000,
            "blockhash": HASH,
            "txs": 2500,
            "totalfee": 123456,
            "total_weight": 3999000,
            "minfeerate": 1.0,
            "avgfeerate": 4.5,
            "maxfeerate": 100.0,
            "feerate_percentiles": [1.0, 2.0, 3.0, 4.0, 5.0]
        });
        assert!(parse_block_stats(&valid, HASH).is_ok());

        let malformed = [
            json!({ "feerate_percentiles": [1, 2, 3, 4] }),
            json!({
                "height": 1, "blockhash": HASH, "txs": 1, "totalfee": 1,
                "total_weight": 1, "minfeerate": 1, "avgfeerate": 1,
                "maxfeerate": 1, "feerate_percentiles": [1, 2, -3, 4, 5]
            }),
        ];
        for value in malformed {
            assert_eq!(
                parse_block_stats(&value, HASH),
                Err(ObservationErrorCategory::InvalidResponse)
            );
        }
    }

    #[test]
    fn deployment_observation_uses_exact_aliases_and_closed_states() {
        let observed = parse_deployment_info(&json!({
            "deployments": { "reduced_data": { "bip9": { "status": "started" } } }
        }));
        assert_eq!(observed.status, DeploymentObservationStatus::Observed);
        assert_eq!(observed.observed_alias, Some(DeploymentAlias::ReducedData));
        assert_eq!(
            observed.reported_state,
            Some(DeploymentReportedState::Started)
        );

        let not_exposed = parse_deployment_info(&json!({
            "deployments": { "reduced-data": {}, "BIP110": {} }
        }));
        assert_eq!(not_exposed.status, DeploymentObservationStatus::NotExposed);

        let malformed = parse_deployment_info(&json!({ "deployments": [] }));
        assert_eq!(malformed.status, DeploymentObservationStatus::Unknown);
        assert_eq!(
            malformed.error_category,
            Some(ObservationErrorCategory::InvalidResponse)
        );

        let malformed_alias = parse_deployment_info(&json!({ "deployments": { "bip110": true } }));
        assert_eq!(malformed_alias.status, DeploymentObservationStatus::Unknown);
    }

    #[test]
    fn deployment_method_not_found_is_classified_as_unsupported_input() {
        let body = json!({
            "result": null,
            "error": { "code": -32601, "message": "not included in output" },
            "id": "conxian-shadow-observer"
        });
        let failure = parse_rpc_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &serde_json::to_vec(&body).unwrap(),
        )
        .unwrap_err();
        assert_eq!(failure, RpcCallFailure::MethodNotFound);
        let observation = deployment_from_rpc_result(Err(failure));
        assert_eq!(observation.status, DeploymentObservationStatus::Unsupported);
        assert_eq!(observation.error_category, None);
    }

    #[test]
    fn rpc_response_missing_id_is_invalid() {
        let failure = parse_rpc_response(
            StatusCode::OK,
            &serde_json::to_vec(&json!({ "result": {}, "error": null })).unwrap(),
        )
        .unwrap_err();
        assert_eq!(
            failure,
            RpcCallFailure::Category(ObservationErrorCategory::InvalidResponse)
        );
    }

    #[test]
    fn rpc_response_null_id_is_invalid() {
        let failure = parse_rpc_response(
            StatusCode::OK,
            &serde_json::to_vec(&json!({ "result": {}, "error": null, "id": null })).unwrap(),
        )
        .unwrap_err();
        assert_eq!(
            failure,
            RpcCallFailure::Category(ObservationErrorCategory::InvalidResponse)
        );
    }

    #[test]
    fn rpc_response_mismatched_id_is_invalid() {
        let failure = parse_rpc_response(
            StatusCode::OK,
            &serde_json::to_vec(&json!({
                "result": {},
                "error": null,
                "id": "different-test-response"
            }))
            .unwrap(),
        )
        .unwrap_err();
        assert_eq!(
            failure,
            RpcCallFailure::Category(ObservationErrorCategory::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn missing_chain_observation_makes_block_stats_dependency_unavailable() {
        let client = BitcoinCoreShadowObserverClient::new("http://127.0.0.1:1", "", "").unwrap();
        let blockchain = SourceObservation::<CoreBlockchainInfo>::unavailable(
            ObservationErrorCategory::Transport,
        );
        let stats = client.observe_block_stats(&blockchain).await;
        assert_eq!(
            stats.availability,
            ObservationAvailability::DependencyUnavailable
        );
        assert_eq!(stats.data, None);
        assert_eq!(stats.error_category, None);
    }

    #[tokio::test]
    async fn wire_contract_uses_fixed_methods_params_fields_and_basic_auth() {
        let (url, mut requests, server) = spawn_standard_server(8).await;
        let username = ["shadow", "test", "user"].join("-");
        let password = ["shadow", "test", "password"].join("-");
        let expected_auth = expected_test_basic_auth(&username, &password);
        let client = BitcoinCoreShadowObserverClient::new_with_transport_limits(
            &url,
            &format!(" {username} "),
            &password,
            test_limits(Duration::from_secs(1), 16 * 1024),
        )
        .unwrap();

        let observation = timeout(Duration::from_secs(3), client.observe())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            observation.network_info.availability,
            ObservationAvailability::Observed
        );
        server.await.unwrap();

        let mut captured = Vec::new();
        while let Some(request) = requests.recv().await {
            assert_eq!(
                request.headers.get("authorization").map(String::as_str),
                Some(expected_auth.as_str())
            );
            captured.push(request.body);
        }
        assert_eq!(captured.len(), 8);

        for method in [
            "getnetworkinfo",
            "getblockchaininfo",
            "getmempoolinfo",
            "getdeploymentinfo",
        ] {
            let request = captured
                .iter()
                .find(|request| request["method"] == method)
                .unwrap();
            assert_eq!(request["params"], json!([]));
        }
        let mut fee_params: Vec<Value> = captured
            .iter()
            .filter(|request| request["method"] == "estimatesmartfee")
            .map(|request| request["params"].clone())
            .collect();
        fee_params.sort_by_key(|params| params[0].as_u64().unwrap());
        assert_eq!(fee_params, vec![json!([2]), json!([6]), json!([12])]);

        let stats = captured
            .iter()
            .find(|request| request["method"] == "getblockstats")
            .unwrap();
        assert_eq!(stats["params"], json!([HASH, BLOCK_STATS_FIELDS]));
    }

    #[tokio::test]
    async fn slow_response_fails_promptly_without_follow_on_work() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let accepted = Arc::new(AtomicUsize::new(0));
        let server_accepted = accepted.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _request = read_request(&mut stream).await;
            server_accepted.fetch_add(1, Ordering::SeqCst);
            sleep(Duration::from_millis(250)).await;
            write_json_response(&mut stream, "200 OK", &rpc_result("getnetworkinfo")).await;
        });
        let client = BitcoinCoreShadowObserverClient::new_with_transport_limits(
            &url,
            "",
            "",
            test_limits(Duration::from_millis(25), 4096),
        )
        .unwrap();

        let started = Instant::now();
        let failure = timeout(
            Duration::from_millis(500),
            client.shadow_rpc_call("getnetworkinfo", "getnetworkinfo", vec![]),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(
            failure,
            RpcCallFailure::Category(ObservationErrorCategory::Transport)
        );
        assert!(started.elapsed() < Duration::from_millis(200));
        assert_eq!(accepted.load(Ordering::SeqCst), 1);
        sleep(Duration::from_millis(60)).await;
        assert_eq!(accepted.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn oversized_declared_content_length_is_rejected_before_buffering() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _request = read_request(&mut stream).await;
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 65\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            sleep(Duration::from_millis(250)).await;
        });
        let client = BitcoinCoreShadowObserverClient::new_with_transport_limits(
            &url,
            "",
            "",
            test_limits(Duration::from_millis(150), 64),
        )
        .unwrap();

        let failure = timeout(
            Duration::from_millis(100),
            client.shadow_rpc_call("getnetworkinfo", "getnetworkinfo", vec![]),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(
            failure,
            RpcCallFailure::Category(ObservationErrorCategory::InvalidResponse)
        );
        server.abort();
    }

    #[tokio::test]
    async fn chunked_body_crossing_actual_byte_cap_is_rejected_while_streaming() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _request = read_request(&mut stream).await;
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n28\r\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n28\r\nbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\r\n0\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let client = BitcoinCoreShadowObserverClient::new_with_transport_limits(
            &url,
            "",
            "",
            test_limits(Duration::from_millis(250), 64),
        )
        .unwrap();

        let failure = timeout(
            Duration::from_millis(500),
            client.shadow_rpc_call("getnetworkinfo", "getnetworkinfo", vec![]),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(
            failure,
            RpcCallFailure::Category(ObservationErrorCategory::InvalidResponse)
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cancellation_releases_admission_and_concurrent_observation_fails_fast() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hold = Arc::new(AtomicBool::new(true));
        let accepted = Arc::new(AtomicUsize::new(0));
        let accepted_notify = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let server = {
            let hold = hold.clone();
            let accepted = accepted.clone();
            let accepted_notify = accepted_notify.clone();
            let release = release.clone();
            tokio::spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let hold = hold.clone();
                    let accepted = accepted.clone();
                    let accepted_notify = accepted_notify.clone();
                    let release = release.clone();
                    tokio::spawn(async move {
                        let request = read_request(&mut stream).await;
                        accepted.fetch_add(1, Ordering::SeqCst);
                        accepted_notify.notify_waiters();
                        while hold.load(Ordering::SeqCst) {
                            release.notified().await;
                        }
                        let method = request.body["method"].as_str().unwrap();
                        write_json_response(&mut stream, "200 OK", &rpc_result(method)).await;
                    });
                }
            })
        };
        let client = Arc::new(
            BitcoinCoreShadowObserverClient::new_with_transport_limits(
                &url,
                "",
                "",
                test_limits(Duration::from_secs(2), 16 * 1024),
            )
            .unwrap(),
        );

        let first = {
            let client = client.clone();
            tokio::spawn(async move { client.observe().await })
        };
        timeout(Duration::from_secs(1), async {
            while accepted.load(Ordering::SeqCst) == 0 {
                accepted_notify.notified().await;
            }
        })
        .await
        .unwrap();

        let accepted_before = accepted.load(Ordering::SeqCst);
        let concurrent = timeout(Duration::from_millis(500), client.observe())
            .await
            .expect("concurrent admission must fail without waiting for the active observation")
            .unwrap_err();
        assert_eq!(concurrent.category, ObservationErrorCategory::Transport);
        assert_eq!(accepted.load(Ordering::SeqCst), accepted_before);

        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        hold.store(false, Ordering::SeqCst);
        release.notify_waiters();

        let subsequent = timeout(Duration::from_secs(3), client.observe())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            subsequent.best_block_stats.availability,
            ObservationAvailability::Observed
        );
        server.abort();
    }

    #[tokio::test]
    async fn http_error_json_rpc_method_not_found_remains_unsupported() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            assert_eq!(request.body["method"], "getdeploymentinfo");
            write_json_response(
                &mut stream,
                "500 Internal Server Error",
                &json!({
                    "result": null,
                    "error": { "code": -32601, "message": "Method not found" },
                    "id": "conxian-shadow-observer"
                }),
            )
            .await;
        });
        let client = BitcoinCoreShadowObserverClient::new_with_transport_limits(
            &url,
            "",
            "",
            test_limits(Duration::from_millis(250), 4096),
        )
        .unwrap();

        let deployment = client.observe_deployment().await;
        assert_eq!(deployment.status, DeploymentObservationStatus::Unsupported);
        assert_eq!(deployment.error_category, None);
        server.await.unwrap();
    }

    #[test]
    fn action_capable_trait_is_not_referenced_by_shadow_module() {
        let source = include_str!("shadow_observation.rs");
        for forbidden in [
            concat!("spawn_", "blocking"),
            concat!("Bitcoin", "RpcClient"),
            concat!("FeeBump", "PolicyConfig"),
            concat!("decide_", "fee_bump"),
            concat!("Mempool", "Orchestrator"),
            concat!("submit_rbf_", "replacement"),
            concat!("submit_cpfp_", "child"),
            concat!("sendraw", "transaction"),
        ] {
            assert!(
                !source.contains(forbidden),
                "unexpected coupling: {forbidden}"
            );
        }
    }
}
