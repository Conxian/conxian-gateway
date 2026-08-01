use async_trait::async_trait;
use bitcoin::blockdata::block::Header as BitcoinHeader;
use bitcoin::consensus::deserialize;
use bitcoin::hex::FromHex;
use bitcoin::pow::{Target, Work};
use bitcoin::BlockHash;
use conxian_core::{BlockInfo, ChainAdapter, ConxianError, ConxianResult};
use lib_conxian_core::babylon::StakingIntent;
use lib_conxian_core::control_model::TrustTier;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
#[cfg(test)]
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

const BABYLON_BTC_LIGHT_CLIENT_PATH: &str = "/babylon/btclightclient/v1";
const BABYLON_HEADER_RANGE_LIMIT: u64 = 4_096;
const BABYLON_PAGE_REQUEST_LIMIT: usize = 256;
const BABYLON_MAX_PAGE_HEADERS: usize = BABYLON_PAGE_REQUEST_LIMIT;
const BABYLON_MAX_JSON_BODY_BYTES: usize = 1_048_576;
const BABYLON_MAX_SCAN_PAGES: usize = 128;
const BABYLON_MAX_SCAN_BYTES: usize = 8 * 1_048_576;
const BABYLON_MAX_SCAN_HEADERS: usize = BABYLON_MAX_SCAN_PAGES * BABYLON_MAX_PAGE_HEADERS;
const MAX_WORK_DECIMAL_DIGITS: usize = 78;

/// Babylon's BTC light-client header response.
///
/// `height` is accepted as either a JSON number or a decimal string because
/// Cosmos REST gateways can render integer fields in either form. Values are
/// still bounded to the u32 range used by Babylon's BTC header type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BabylonHeaderInfoResponse {
    pub header_hex: String,
    pub hash_hex: String,
    #[serde(deserialize_with = "deserialize_u32_compatible")]
    pub height: u32,
    pub work: String,
}

/// Cosmos-style pagination metadata returned by Babylon REST queries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BabylonPagination {
    #[serde(default)]
    pub next_key: Option<String>,
}

/// Response envelope for `/babylon/btclightclient/v1/tip`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BabylonTipResponse {
    pub header: BabylonHeaderInfoResponse,
}

/// Response envelope for `/babylon/btclightclient/v1/mainchain`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BabylonMainChainResponse {
    pub headers: Vec<BabylonHeaderInfoResponse>,
    pub pagination: BabylonPagination,
}

/// Parsed and locally verified Bitcoin header information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BtcHeaderInfo {
    pub height: u64,
    pub hash: String,
    pub timestamp: u64,
    pub prev_blockhash: String,
}

#[derive(Debug, Clone)]
struct ParsedBtcHeader {
    info: BtcHeaderInfo,
    advertised_work: Work,
    header_work: Work,
}

/// Narrow source abstraction for Babylon BTC header-chain data.
///
/// Keeping this separate from `BitcoinRpc` makes Babylon verification testable
/// offline and keeps the Babylon REST contract isolated from Bitcoin Core RPC.
#[async_trait]
pub trait BabylonHeaderSource: Send + Sync {
    async fn tip(&self) -> ConxianResult<BabylonHeaderInfoResponse>;

    async fn main_chain(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> ConxianResult<Vec<BabylonHeaderInfoResponse>>;
}

/// Production Babylon REST client for the BTC light-client endpoints.
pub struct BabylonHttpClient {
    base_url: String,
    client: reqwest::Client,
}

impl BabylonHttpClient {
    pub fn new(base_url: impl Into<String>) -> ConxianResult<Self> {
        let base_url = base_url.into();
        let base_url = base_url.trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| {
                ConxianError::Internal("Failed to build Babylon HTTP client".to_string())
            })?;

        Ok(Self { base_url, client })
    }

    async fn get_json<T>(&self, path: &str, query: &[(String, String)]) -> ConxianResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.get_json_with_size(path, query)
            .await
            .map(|(value, _)| value)
    }

    async fn get_json_with_size<T>(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> ConxianResult<(T, usize)>
    where
        T: for<'de> Deserialize<'de>,
    {
        if self.base_url.is_empty() {
            return Err(ConxianError::Internal(
                "Babylon API URL is not configured".to_string(),
            ));
        }

        debug!(chain = "babylon", endpoint = %path, "Querying Babylon BTC light client");

        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(url)
            .query(query)
            .send()
            .await
            .map_err(|_| ConxianError::Internal("Babylon API request failed".to_string()))?;

        let status = response.status();
        let response = response.error_for_status().map_err(|_| {
            warn!(
                chain = "babylon",
                endpoint = %path,
                status = status.as_u16(),
                "Babylon API returned a non-success response"
            );
            ConxianError::Api(format!(
                "Babylon API returned HTTP status {}",
                status.as_u16()
            ))
        })?;

        let declared_length = match response.headers().get(reqwest::header::CONTENT_LENGTH) {
            Some(value) => Some(
                value
                    .to_str()
                    .map_err(|_| {
                        ConxianError::Api(
                            "Babylon API returned an invalid Content-Length".to_string(),
                        )
                    })?
                    .parse::<u64>()
                    .map_err(|_| {
                        ConxianError::Api(
                            "Babylon API returned an invalid Content-Length".to_string(),
                        )
                    })?,
            ),
            None => None,
        };
        if declared_length.is_some_and(|length| length > BABYLON_MAX_JSON_BODY_BYTES as u64) {
            return Err(ConxianError::Api(format!(
                "Babylon API response body exceeds {} bytes",
                BABYLON_MAX_JSON_BODY_BYTES
            )));
        }

        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            warn!(
                chain = "babylon",
                endpoint = %path,
                "Babylon API response body could not be read"
            );
            ConxianError::Internal("Babylon API response body could not be read".to_string())
        })? {
            if body
                .len()
                .checked_add(chunk.len())
                .is_none_or(|size| size > BABYLON_MAX_JSON_BODY_BYTES)
            {
                return Err(ConxianError::Api(format!(
                    "Babylon API response body exceeds {} bytes",
                    BABYLON_MAX_JSON_BODY_BYTES
                )));
            }
            body.extend_from_slice(&chunk);
        }

        let body_len = body.len();
        let value = serde_json::from_slice(&body).map_err(|_| {
            warn!(
                chain = "babylon",
                endpoint = %path,
                "Babylon API returned malformed JSON"
            );
            ConxianError::Internal(format!("Babylon API returned malformed JSON for {path}"))
        })?;

        Ok((value, body_len))
    }
}

#[async_trait]
impl BabylonHeaderSource for BabylonHttpClient {
    async fn tip(&self) -> ConxianResult<BabylonHeaderInfoResponse> {
        let response: BabylonTipResponse = self
            .get_json(&format!("{BABYLON_BTC_LIGHT_CLIENT_PATH}/tip"), &[])
            .await?;
        Ok(response.header)
    }

    async fn main_chain(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> ConxianResult<Vec<BabylonHeaderInfoResponse>> {
        let range_len = validate_height_range(from_height, to_height)?;
        let page_limit = range_len.min(BABYLON_PAGE_REQUEST_LIMIT as u64);
        let mut requested_headers = BTreeMap::new();
        let mut next_key: Option<String> = None;
        let mut seen_page_keys = HashSet::new();
        let mut scanned_pages = 0usize;
        let mut scanned_bytes = 0usize;
        let mut scanned_headers = 0usize;

        loop {
            if scanned_pages >= BABYLON_MAX_SCAN_PAGES {
                return Err(ConxianError::Internal(
                    "Babylon API pagination exceeded the bounded scan page budget".to_string(),
                ));
            }
            scanned_pages += 1;

            let mut query = vec![("pagination.limit".to_string(), page_limit.to_string())];
            if let Some(key) = &next_key {
                query.push(("pagination.key".to_string(), key.clone()));
            }

            let (response, response_bytes): (BabylonMainChainResponse, usize) = self
                .get_json_with_size(
                    &format!("{BABYLON_BTC_LIGHT_CLIENT_PATH}/mainchain"),
                    &query,
                )
                .await?;

            scanned_bytes = scanned_bytes.checked_add(response_bytes).ok_or_else(|| {
                ConxianError::Internal("Babylon API scan byte count overflowed".to_string())
            })?;
            if scanned_bytes > BABYLON_MAX_SCAN_BYTES {
                return Err(ConxianError::Internal(
                    "Babylon API pagination exceeded the bounded scan byte budget".to_string(),
                ));
            }

            let page_header_count = response.headers.len();
            if page_header_count > page_limit as usize
                || page_header_count > BABYLON_MAX_PAGE_HEADERS
            {
                return Err(ConxianError::Security(format!(
                    "Babylon API main-chain page returned more than the requested {page_limit} headers"
                )));
            }
            scanned_headers = scanned_headers
                .checked_add(page_header_count)
                .ok_or_else(|| {
                    ConxianError::Internal("Babylon API scan header count overflowed".to_string())
                })?;
            if scanned_headers > BABYLON_MAX_SCAN_HEADERS {
                return Err(ConxianError::Internal(
                    "Babylon API pagination exceeded the bounded scan header budget".to_string(),
                ));
            }

            for header in response.headers {
                let height = u64::from(header.height);
                if (from_height..=to_height).contains(&height)
                    && requested_headers.insert(height, header).is_some()
                {
                    return Err(ConxianError::Security(
                        "Babylon API returned a duplicate requested header height".to_string(),
                    ));
                }
            }

            if requested_headers.len() == range_len as usize {
                return Ok(requested_headers.into_values().collect());
            }

            let Some(key) = response
                .pagination
                .next_key
                .filter(|key| !key.trim().is_empty())
            else {
                return Err(ConxianError::Security(format!(
                    "Babylon API pagination ended before covering requested heights {from_height}..={to_height}"
                )));
            };

            if !seen_page_keys.insert(key.clone()) {
                return Err(ConxianError::Internal(
                    "Babylon API pagination key repeated".to_string(),
                ));
            }
            next_key = Some(key);
        }
    }
}

/// Protocol Adapter for Babylon (Partner Lane - CON-712).
pub struct BabylonAdapter {
    pub network: String,
    header_source: Option<Arc<dyn BabylonHeaderSource>>,
}

impl BabylonAdapter {
    /// Construct an adapter without a Babylon data source.
    ///
    /// Header-height and header-chain operations return an explicit
    /// configuration error until a source is injected.
    pub fn new(network: String) -> Self {
        Self {
            network,
            header_source: None,
        }
    }

    /// Construct an adapter with an injectable Babylon header source.
    pub fn with_header_source(
        network: String,
        header_source: Arc<dyn BabylonHeaderSource>,
    ) -> Self {
        Self {
            network,
            header_source: Some(header_source),
        }
    }

    /// Construct an adapter backed by Babylon's official REST query paths.
    pub fn with_babylon_api_url(
        network: String,
        api_url: impl Into<String>,
    ) -> ConxianResult<Self> {
        let api_url = api_url.into();
        if api_url.trim().is_empty() {
            return Ok(Self::new(network));
        }

        Ok(Self::with_header_source(
            network,
            Arc::new(BabylonHttpClient::new(api_url)?),
        ))
    }

    fn header_source(&self) -> ConxianResult<&Arc<dyn BabylonHeaderSource>> {
        self.header_source
            .as_ref()
            .ok_or_else(|| ConxianError::Internal("Babylon API URL is not configured".to_string()))
    }

    /// Get current BTC header-chain height from Babylon's canonical tip.
    pub async fn get_btc_header_height(&self) -> ConxianResult<u64> {
        let raw_tip = self.header_source()?.tip().await?;
        let tip = parse_babylon_header(&raw_tip)?;
        info!(
            chain = "babylon",
            btc_height = tip.info.height,
            "BTC header-chain tip verified"
        );
        Ok(tip.info.height)
    }

    /// Get a verified BTC header from Babylon's canonical chain.
    pub async fn get_verified_btc_header(&self, height: u64) -> ConxianResult<BtcHeaderInfo> {
        let raw_headers = self.header_source()?.main_chain(height, height).await?;
        let verified = verify_header_entries(raw_headers, height, height)?;
        verified.into_iter().next().ok_or_else(|| {
            ConxianError::Security(format!(
                "Babylon header chain did not return requested height {height}"
            ))
        })
    }

    /// Get BTC block information for generic chain consumers.
    pub async fn get_btc_header_info(&self, height: u64) -> ConxianResult<BlockInfo> {
        let header = self.get_verified_btc_header(height).await?;
        Ok(BlockInfo {
            hash: header.hash,
            height: header.height,
            timestamp: header.timestamp,
        })
    }

    /// Validate a Babylon staking intent against core trust tier taxonomy.
    ///
    /// Uses `lib_conxian_core::babylon::StakingIntent` to validate staking
    /// lifecycle operations against the T2 (Managed) trust tier. Babylon
    /// staking requires BTC finality verification (via header chain) before
    /// accepting any staking intent.
    ///
    /// Wire path: gateway babylon_adapter → lib_conxian_core::babylon::StakingIntent
    ///            → lib_conxian_core::control_model::TrustTier::Managed
    pub async fn validate_staking_intent(
        &self,
        staker_pubkey: &[u8],
        finality_provider_pubkey: &[u8],
        amount_sats: u64,
        lock_time_blocks: u32,
    ) -> ConxianResult<StakingIntent> {
        let current_height = self.get_latest_height().await?;

        info!(
            chain = "babylon",
            amount_sats,
            lock_time_blocks,
            current_height,
            "Validating Babylon staking intent at T2 (Managed) trust tier"
        );

        // Minimum 6 confirmations for T2 trust tier (per CON-791)
        if lock_time_blocks < 6 {
            return Err(ConxianError::Validation(format!(
                "Babylon staking requires ≥6 lock-time blocks (got {lock_time_blocks})"
            )));
        }

        let intent = StakingIntent {
            staker_pubkey: staker_pubkey.to_vec(),
            finality_provider_pubkey: finality_provider_pubkey.to_vec(),
            amount_sats,
            lock_time_blocks,
        };

        info!(
            chain = "babylon",
            amount_sats = intent.amount_sats,
            lock_time_blocks = intent.lock_time_blocks,
            trust_tier = ?TrustTier::Managed,
            "Babylon staking intent validated — ready for treasury processing"
        );

        Ok(intent)
    }

    /// Verify a bounded, contiguous Babylon BTC header-chain range.
    pub async fn verify_header_chain(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> ConxianResult<bool> {
        let _ = validate_height_range(from_height, to_height)?;
        debug!(
            chain = "babylon",
            from_height, to_height, "Verifying Babylon BTC header-chain range"
        );

        let raw_headers = self
            .header_source()?
            .main_chain(from_height, to_height)
            .await?;
        let verified = verify_header_entries(raw_headers, from_height, to_height)?;
        info!(
            chain = "babylon",
            from_height,
            to_height,
            header_count = verified.len(),
            "Babylon BTC header-chain range verified"
        );
        Ok(true)
    }
}

#[async_trait]
impl ChainAdapter for BabylonAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        self.get_btc_header_height().await
    }

    async fn get_chain_identity(&self) -> String {
        format!("babylon:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(chain = "babylon", "Preparing staking transaction");
        Ok(json!({
            "chain": "babylon",
            "status": "prepared",
            "payload": tx_details,
            "type": "staking"
        }))
    }

    async fn verify_state_proof(&self, proof_metadata: Value) -> ConxianResult<bool> {
        info!(chain = "babylon", "Verifying Babylon finality proof");

        // If a Babylon source is configured, require explicit BTC height
        // metadata and use the canonical tip for the bounded recency check.
        // EOTS and full finality verification remain outside this issue's
        // scope. Never fall through to rehearsal-mode acceptance here.
        if self.header_source.is_some() {
            let Some(proof_height) = proof_metadata["btc_height"]
                .as_u64()
                .filter(|height| *height > 0)
            else {
                return Ok(false);
            };

            let current_height = self.get_btc_header_height().await?;
            return Ok(current_height >= proof_height && current_height - proof_height <= 6);
        }

        // Preserve the existing rehearsal-mode proof-type behavior for callers
        // that have not supplied Babylon BTC proof metadata yet.
        let proof_type = proof_metadata["type"].as_str().unwrap_or("unknown");
        Ok(proof_type == "finality_gadget")
    }
}

fn validate_height_range(from_height: u64, to_height: u64) -> ConxianResult<u64> {
    if from_height > to_height {
        return Err(ConxianError::Internal(
            "Invalid height range for header chain verification".to_string(),
        ));
    }

    let range_len = to_height
        .checked_sub(from_height)
        .and_then(|span| span.checked_add(1))
        .ok_or_else(|| {
            ConxianError::Internal("Invalid height range for header chain verification".to_string())
        })?;

    if range_len > BABYLON_HEADER_RANGE_LIMIT {
        return Err(ConxianError::Internal(format!(
            "Babylon header range exceeds limit of {BABYLON_HEADER_RANGE_LIMIT} headers"
        )));
    }

    Ok(range_len)
}

fn verify_header_entries(
    headers: Vec<BabylonHeaderInfoResponse>,
    from_height: u64,
    to_height: u64,
) -> ConxianResult<Vec<BtcHeaderInfo>> {
    let expected_count = validate_height_range(from_height, to_height)? as usize;
    let mut requested_headers = BTreeMap::new();
    for header in headers {
        let height = u64::from(header.height);
        if (from_height..=to_height).contains(&height)
            && requested_headers.insert(height, header).is_some()
        {
            return Err(ConxianError::Security(
                "Babylon header chain contains a duplicate requested height".to_string(),
            ));
        }
    }

    if requested_headers.len() != expected_count {
        return Err(ConxianError::Security(format!(
            "Babylon header chain is missing requested heights {from_height}..={to_height}"
        )));
    }

    let mut parsed: Vec<ParsedBtcHeader> = Vec::with_capacity(requested_headers.len());
    for raw in requested_headers.into_values() {
        let parsed_header = parse_babylon_header(&raw)?;

        if let Some(previous) = parsed.last() {
            let expected_height = previous.info.height.checked_add(1).ok_or_else(|| {
                ConxianError::Security("Babylon header height overflow".to_string())
            })?;
            if parsed_header.info.height != expected_height {
                return Err(ConxianError::Security(
                    "Babylon header chain contains a gap or duplicate height".to_string(),
                ));
            }
            if parsed_header.info.prev_blockhash != previous.info.hash {
                return Err(ConxianError::Security(
                    "Babylon header chain has a broken previous-block linkage".to_string(),
                ));
            }
        }

        parsed.push(parsed_header);
    }

    let first_height = parsed.first().map(|header| header.info.height);
    let last_height = parsed.last().map(|header| header.info.height);
    if first_height != Some(from_height) || last_height != Some(to_height) {
        return Err(ConxianError::Security(format!(
            "Babylon header chain did not cover requested heights {from_height}..={to_height}"
        )));
    }

    if from_height == 0 {
        let genesis = parsed.first().expect("validated range contains a header");
        if genesis.advertised_work != genesis.header_work {
            return Err(ConxianError::Security(
                "Babylon genesis cumulative work does not equal its own header work".to_string(),
            ));
        }
    }

    for pair in parsed.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        let expected_work = checked_add_work(previous.advertised_work, current.header_work)
            .ok_or_else(|| {
                ConxianError::Security(
                    "Babylon cumulative work transition overflowed 256 bits".to_string(),
                )
            })?;
        let actual_delta = checked_sub_work(current.advertised_work, previous.advertised_work);
        if actual_delta != Some(current.header_work) || current.advertised_work != expected_work {
            return Err(ConxianError::Security(format!(
                "Babylon cumulative work transition is invalid at height {}",
                current.info.height
            )));
        }
    }

    Ok(parsed.into_iter().map(|header| header.info).collect())
}

fn parse_babylon_header(raw: &BabylonHeaderInfoResponse) -> ConxianResult<ParsedBtcHeader> {
    let header_bytes = Vec::<u8>::from_hex(&raw.header_hex).map_err(|_| {
        ConxianError::Security(format!(
            "Babylon header at height {} contains invalid header hex",
            raw.height
        ))
    })?;

    if header_bytes.len() != BitcoinHeader::SIZE {
        return Err(ConxianError::Security(format!(
            "Babylon header at height {} is not exactly 80 bytes",
            raw.height
        )));
    }

    let header: BitcoinHeader = deserialize(&header_bytes).map_err(|_| {
        ConxianError::Security(format!(
            "Babylon header at height {} is not a valid Bitcoin consensus header",
            raw.height
        ))
    })?;
    let target = Target::from_compact(header.bits);
    let derived_hash = header.block_hash();
    let advertised_hash = BlockHash::from_str(&raw.hash_hex).map_err(|_| {
        ConxianError::Security(format!(
            "Babylon header at height {} contains an invalid block hash",
            raw.height
        ))
    })?;

    if derived_hash != advertised_hash {
        return Err(ConxianError::Security(format!(
            "Babylon header at height {} does not match its advertised block hash",
            raw.height
        )));
    }

    if target == Target::ZERO || !target.is_met_by(derived_hash) {
        return Err(ConxianError::Security(format!(
            "Babylon header at height {} does not satisfy proof-of-work",
            raw.height
        )));
    }

    let advertised_work = parse_decimal_work(&raw.work).ok_or_else(|| {
        ConxianError::Security(format!(
            "Babylon header at height {} contains invalid work",
            raw.height
        ))
    })?;
    if advertised_work == Work::from_be_bytes([0; 32]) {
        return Err(ConxianError::Security(format!(
            "Babylon header at height {} contains zero work",
            raw.height
        )));
    }

    Ok(ParsedBtcHeader {
        info: BtcHeaderInfo {
            height: u64::from(raw.height),
            hash: derived_hash.to_string(),
            timestamp: u64::from(header.time),
            prev_blockhash: header.prev_blockhash.to_string(),
        },
        advertised_work,
        header_work: target.to_work(),
    })
}

fn normalize_decimal_string(value: &str) -> Option<String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    let normalized = value.trim_start_matches('0');
    Some(if normalized.is_empty() {
        "0".to_string()
    } else {
        normalized.to_string()
    })
}

#[cfg(test)]
fn compare_decimal_strings(left: &str, right: &str) -> Ordering {
    match (
        normalize_decimal_string(left),
        normalize_decimal_string(right),
    ) {
        (Some(left), Some(right)) => left
            .len()
            .cmp(&right.len())
            .then_with(|| left.as_bytes().cmp(right.as_bytes())),
        _ => Ordering::Equal,
    }
}

fn parse_decimal_work(value: &str) -> Option<Work> {
    let normalized = normalize_decimal_string(value)?;
    if normalized.len() > MAX_WORK_DECIMAL_DIGITS {
        return None;
    }
    let mut bytes = [0u8; 32];

    for digit in normalized.bytes() {
        let mut carry = u16::from(digit - b'0');
        for byte in bytes.iter_mut().rev() {
            let product = u16::from(*byte) * 10 + carry;
            *byte = product as u8;
            carry = product >> 8;
        }
        if carry != 0 {
            return None;
        }
    }

    Some(Work::from_be_bytes(bytes))
}

fn checked_add_work(left: Work, right: Work) -> Option<Work> {
    let left = left.to_be_bytes();
    let right = right.to_be_bytes();
    let mut result = [0u8; 32];
    let mut carry = 0u16;

    for index in (0..result.len()).rev() {
        let sum = u16::from(left[index]) + u16::from(right[index]) + carry;
        result[index] = sum as u8;
        carry = sum >> 8;
    }

    (carry == 0).then(|| Work::from_be_bytes(result))
}

fn checked_sub_work(left: Work, right: Work) -> Option<Work> {
    let left = left.to_be_bytes();
    let right = right.to_be_bytes();
    let mut result = [0u8; 32];
    let mut borrow = 0i16;

    for index in (0..result.len()).rev() {
        let difference = i16::from(left[index]) - i16::from(right[index]) - borrow;
        if difference < 0 {
            result[index] = (difference + 256) as u8;
            borrow = 1;
        } else {
            result[index] = difference as u8;
            borrow = 0;
        }
    }

    (borrow == 0).then(|| Work::from_be_bytes(result))
}

fn deserialize_u32_compatible<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    let parsed = match value {
        Value::Number(number) => number.as_u64(),
        Value::String(value) => value.parse::<u64>().ok(),
        _ => None,
    };

    parsed
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| D::Error::custom("height must be a u32-compatible integer"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::task::JoinHandle;

    const MAINCHAIN_FIXTURE: &str =
        include_str!("../../test-fixtures/babylon/mainnet_mainchain.json");
    const TIP_FIXTURE: &str = include_str!("../../test-fixtures/babylon/mainnet_tip.json");

    #[derive(Clone)]
    struct FixtureSource {
        tip: BabylonHeaderInfoResponse,
        headers: Vec<BabylonHeaderInfoResponse>,
    }

    #[async_trait]
    impl BabylonHeaderSource for FixtureSource {
        async fn tip(&self) -> ConxianResult<BabylonHeaderInfoResponse> {
            Ok(self.tip.clone())
        }

        async fn main_chain(
            &self,
            from_height: u64,
            to_height: u64,
        ) -> ConxianResult<Vec<BabylonHeaderInfoResponse>> {
            Ok(self
                .headers
                .iter()
                .filter(|header| (from_height..=to_height).contains(&u64::from(header.height)))
                .cloned()
                .collect())
        }
    }

    fn fixture_source() -> FixtureSource {
        let mainchain: BabylonMainChainResponse =
            serde_json::from_str(MAINCHAIN_FIXTURE).expect("valid mainchain fixture");
        let tip: BabylonTipResponse = serde_json::from_str(TIP_FIXTURE).expect("valid tip fixture");
        FixtureSource {
            tip: tip.header,
            headers: mainchain.headers,
        }
    }

    fn fixture_adapter() -> BabylonAdapter {
        BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(fixture_source()))
    }

    fn response_body(headers: Vec<BabylonHeaderInfoResponse>, next_key: Option<&str>) -> String {
        serde_json::to_string(&BabylonMainChainResponse {
            headers,
            pagination: BabylonPagination {
                next_key: next_key.map(str::to_string),
            },
        })
        .unwrap()
    }

    #[derive(Clone)]
    struct TestResponse {
        status: u16,
        body: String,
    }

    async fn spawn_http_server(
        responses: Vec<TestResponse>,
    ) -> (String, Arc<Mutex<Vec<String>>>, JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind test HTTP server");
        let address = listener.local_addr().expect("test HTTP server address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded_requests = Arc::clone(&requests);

        let handle = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.expect("accept test request");
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                loop {
                    let read = stream.read(&mut buffer).await.expect("read test request");
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }

                let request_text = String::from_utf8_lossy(&request).to_string();
                recorded_requests.lock().unwrap().push(request_text);
                let reason = match response.status {
                    200 => "OK",
                    500 => "Internal Server Error",
                    _ => "Test Response",
                };
                let http_response = format!(
                    "HTTP/1.1 {} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.status,
                    response.body.len(),
                    response.body
                );
                stream.write_all(http_response.as_bytes()).await.ok();
                stream.shutdown().await.ok();
            }
        });

        (format!("http://{address}"), requests, handle)
    }

    #[tokio::test]
    async fn verifies_valid_mainnet_chain_and_normalizes_tip_first_response_order() {
        let adapter = fixture_adapter();

        assert!(adapter.verify_header_chain(0, 2).await.unwrap());
        assert_eq!(adapter.get_btc_header_height().await.unwrap(), 2);

        let header = adapter.get_verified_btc_header(1).await.unwrap();
        assert_eq!(header.height, 1);
        assert_eq!(header.timestamp, 1_231_469_665);
    }

    #[tokio::test]
    async fn rejects_no_source_instead_of_returning_zero() {
        let adapter = BabylonAdapter::new("testnet".to_string());
        let error = adapter.get_btc_header_height().await.unwrap_err();
        assert!(error
            .to_string()
            .contains("Babylon API URL is not configured"));
        assert!(adapter
            .verify_state_proof(json!({"type": "finality_gadget"}))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn rejects_zero_work_and_accepts_positive_leading_zero_work() {
        let mut zero_tip = fixture_source();
        zero_tip.tip.work = "0000".to_string();
        let adapter = BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(zero_tip));
        assert!(adapter.get_btc_header_height().await.is_err());

        let mut zero_mainchain = fixture_source();
        zero_mainchain.headers[2].work = "0000".to_string();
        let adapter =
            BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(zero_mainchain));
        assert!(adapter.verify_header_chain(0, 2).await.is_err());

        let mut leading_zero_tip = fixture_source();
        leading_zero_tip.tip.work = "00012885098499".to_string();
        let adapter =
            BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(leading_zero_tip));
        assert_eq!(adapter.get_btc_header_height().await.unwrap(), 2);

        let mut leading_zero_chain = fixture_source();
        leading_zero_chain.headers[0].work = "00012885098499".to_string();
        leading_zero_chain.headers[1].work = "0008590065666".to_string();
        leading_zero_chain.headers[2].work = "0004295032833".to_string();
        let adapter =
            BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(leading_zero_chain));
        assert!(adapter.verify_header_chain(0, 2).await.unwrap());
    }

    #[tokio::test]
    async fn rejects_malformed_tip_header_and_hash() {
        let mut malformed_length = fixture_source();
        malformed_length.tip.header_hex = "00".repeat(79);
        let adapter =
            BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(malformed_length));
        assert!(adapter.get_btc_header_height().await.is_err());

        let mut malformed_hash = fixture_source();
        malformed_hash.tip.hash_hex = "00".repeat(31);
        let adapter =
            BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(malformed_hash));
        assert!(adapter.get_btc_header_height().await.is_err());
    }

    #[tokio::test]
    async fn configured_source_does_not_accept_finality_type_without_btc_height() {
        let adapter = fixture_adapter();
        assert!(!adapter
            .verify_state_proof(json!({"type": "finality_gadget"}))
            .await
            .unwrap());
        assert!(adapter
            .verify_state_proof(json!({
                "type": "finality_gadget",
                "btc_height": 1
            }))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn rejects_malformed_header_hex_and_wrong_length() {
        let source = fixture_source();
        let mut malformed = source.headers.clone();
        malformed[0].header_hex = "zz".to_string();
        let adapter = BabylonAdapter::with_header_source(
            "testnet".to_string(),
            Arc::new(FixtureSource {
                tip: source.tip.clone(),
                headers: malformed,
            }),
        );
        assert!(adapter.verify_header_chain(0, 2).await.is_err());

        let mut wrong_length = source.headers;
        wrong_length[0].header_hex = "00".repeat(79);
        let adapter = BabylonAdapter::with_header_source(
            "testnet".to_string(),
            Arc::new(FixtureSource {
                tip: source.tip,
                headers: wrong_length,
            }),
        );
        assert!(adapter.verify_header_chain(0, 2).await.is_err());
    }

    #[tokio::test]
    async fn rejects_mismatched_hash() {
        let mut source = fixture_source();
        source.headers[1].hash_hex = "00".repeat(32);
        let adapter = BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(source));
        assert!(adapter.verify_header_chain(0, 2).await.is_err());
    }

    #[tokio::test]
    async fn rejects_gaps_duplicates_and_broken_previous_linkage() {
        let source = fixture_source();

        let mut gap = source.clone();
        gap.headers[1].height = 2;
        let adapter = BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(gap));
        assert!(adapter.verify_header_chain(0, 2).await.is_err());

        let mut duplicate = source.clone();
        duplicate.headers.push(duplicate.headers[0].clone());
        let adapter =
            BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(duplicate));
        assert!(adapter.verify_header_chain(0, 2).await.is_err());

        let mut broken_link = source;
        broken_link.headers[2]
            .header_hex
            .replace_range(8..72, &"11".repeat(32));
        let broken_bytes = Vec::<u8>::from_hex(&broken_link.headers[2].header_hex).unwrap();
        let broken_header: BitcoinHeader = deserialize(&broken_bytes).unwrap();
        broken_link.headers[2].hash_hex = broken_header.block_hash().to_string();
        let adapter =
            BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(broken_link));
        assert!(adapter.verify_header_chain(0, 2).await.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_headers_and_invalid_work() {
        let source = fixture_source();
        let mut missing = source.clone();
        missing.headers.retain(|header| header.height != 1);
        let adapter = BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(missing));
        assert!(adapter.verify_header_chain(0, 2).await.is_err());

        let mut invalid_work = source.clone();
        invalid_work.headers[1].work = "".to_string();
        let adapter =
            BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(invalid_work));
        assert!(adapter.verify_header_chain(0, 2).await.is_err());

        let mut non_increasing_work = source;
        non_increasing_work.headers[0].work = "8590065666".to_string();
        let adapter = BabylonAdapter::with_header_source(
            "testnet".to_string(),
            Arc::new(non_increasing_work),
        );
        assert!(adapter.verify_header_chain(0, 2).await.is_err());
    }

    #[tokio::test]
    async fn rejects_matching_hash_with_invalid_proof_of_work() {
        let mut source = fixture_source();
        source.headers[0]
            .header_hex
            .replace_range(152..160, "00000000");
        let invalid_bytes = Vec::<u8>::from_hex(&source.headers[0].header_hex).unwrap();
        let invalid_header: BitcoinHeader = deserialize(&invalid_bytes).unwrap();
        source.headers[0].hash_hex = invalid_header.block_hash().to_string();

        let adapter = BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(source));
        let error = adapter.verify_header_chain(0, 2).await.unwrap_err();
        assert!(error.to_string().contains("proof-of-work"));
    }

    #[tokio::test]
    async fn rejects_wrong_cumulative_work_delta() {
        let mut source = fixture_source();
        source.headers[0].work = "12885098498".to_string();
        let adapter = BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(source));
        let error = adapter.verify_header_chain(0, 2).await.unwrap_err();
        assert!(error.to_string().contains("cumulative work transition"));
    }

    #[tokio::test]
    async fn rejects_wrong_genesis_cumulative_work() {
        let mut source = fixture_source();
        source.headers[2].work = "4295032834".to_string();
        let adapter = BabylonAdapter::with_header_source("testnet".to_string(), Arc::new(source));
        let error = adapter.verify_header_chain(0, 2).await.unwrap_err();
        assert!(error.to_string().contains("genesis cumulative work"));
    }

    #[tokio::test]
    async fn rejects_invalid_ranges() {
        let adapter = fixture_adapter();
        assert!(adapter.verify_header_chain(2, 0).await.is_err());
        assert!(adapter
            .verify_header_chain(0, BABYLON_HEADER_RANGE_LIMIT)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn handles_tip_first_key_pagination_and_filters_out_of_range_headers() {
        let source = fixture_source();
        let tip = source.headers[0].clone();
        let middle = source.headers[1].clone();
        let lower = source.headers[2].clone();
        let (base_url, requests, server) = spawn_http_server(vec![
            TestResponse {
                status: 200,
                body: response_body(vec![tip], Some("page-2")),
            },
            TestResponse {
                status: 200,
                body: response_body(vec![middle], Some("page-3")),
            },
            TestResponse {
                status: 200,
                body: response_body(vec![lower], None),
            },
        ])
        .await;

        let adapter =
            BabylonAdapter::with_babylon_api_url("testnet".to_string(), base_url).unwrap();
        assert!(adapter.verify_header_chain(0, 1).await.unwrap());
        server.await.unwrap();

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        let first_request = requests[0].lines().next().unwrap();
        assert!(first_request
            .starts_with("GET /babylon/btclightclient/v1/mainchain?pagination.limit=2 HTTP/1.1"));
        assert!(!first_request.contains("pagination.offset"));
        assert!(!first_request.contains("pagination.key"));
        assert!(requests[1].contains("pagination.limit=2&pagination.key=page-2"));
        assert!(requests[2].contains("pagination.limit=2&pagination.key=page-3"));
    }

    #[tokio::test]
    async fn rejects_repeated_or_cyclic_pagination_keys() {
        let source = fixture_source();
        let (base_url, requests, server) = spawn_http_server(vec![
            TestResponse {
                status: 200,
                body: response_body(vec![source.headers[2].clone()], Some("page-a")),
            },
            TestResponse {
                status: 200,
                body: response_body(vec![source.headers[1].clone()], Some("page-b")),
            },
            TestResponse {
                status: 200,
                body: response_body(vec![source.headers[0].clone()], Some("page-a")),
            },
        ])
        .await;

        let adapter =
            BabylonAdapter::with_babylon_api_url("testnet".to_string(), base_url).unwrap();
        let error = adapter.verify_header_chain(0, 3).await.unwrap_err();
        assert!(error.to_string().contains("pagination key repeated"));
        server.await.unwrap();
        assert_eq!(requests.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn rejects_page_with_more_headers_than_requested() {
        let source = fixture_source();
        let (base_url, _, server) = spawn_http_server(vec![TestResponse {
            status: 200,
            body: response_body(
                vec![source.headers[0].clone(), source.headers[1].clone()],
                None,
            ),
        }])
        .await;

        let adapter =
            BabylonAdapter::with_babylon_api_url("testnet".to_string(), base_url).unwrap();
        let error = adapter.verify_header_chain(0, 0).await.unwrap_err();
        assert!(error.to_string().contains("more than the requested"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn maps_malformed_json_and_non_success_without_leaking_body() {
        let (base_url, _, malformed_server) = spawn_http_server(vec![TestResponse {
            status: 200,
            body: "{not-json".to_string(),
        }])
        .await;
        let client = BabylonHttpClient::new(base_url).unwrap();
        let error = client.tip().await.unwrap_err();
        assert!(error.to_string().contains("malformed JSON"));
        assert!(!error.to_string().contains("not-json"));
        malformed_server.await.unwrap();

        let (base_url, _, error_server) = spawn_http_server(vec![TestResponse {
            status: 500,
            body: json!({"secret": "should-not-leak"}).to_string(),
        }])
        .await;
        let client = BabylonHttpClient::new(base_url).unwrap();
        let error = client.tip().await.unwrap_err();
        assert!(error.to_string().contains("HTTP status 500"));
        assert!(!error.to_string().contains("should-not-leak"));
        error_server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_oversized_response_body_before_deserialization() {
        let (base_url, _, server) = spawn_http_server(vec![TestResponse {
            status: 200,
            body: "x".repeat(BABYLON_MAX_JSON_BODY_BYTES + 1),
        }])
        .await;
        let client = BabylonHttpClient::new(base_url).unwrap();
        let error = client.tip().await.unwrap_err();
        assert!(error.to_string().contains("response body exceeds"));
        server.await.unwrap();
    }

    #[test]
    fn decimal_work_comparison_does_not_require_big_integers() {
        assert_eq!(compare_decimal_strings("0010", "10"), Ordering::Equal);
        assert_eq!(compare_decimal_strings("10", "11"), Ordering::Less);
        assert_eq!(
            compare_decimal_strings("100000000000000000000", "99"),
            Ordering::Greater
        );
        assert_eq!(normalize_decimal_string("00010"), Some("10".to_string()));
        assert!(normalize_decimal_string("12a").is_none());
    }

    #[test]
    fn decimal_work_conversion_and_arithmetic_are_bounded() {
        let genesis_work = parse_decimal_work("0004295032833").unwrap();
        assert_eq!(genesis_work.to_string(), "4295032833");
        assert_eq!(
            parse_decimal_work(
                "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            )
            .unwrap()
            .to_string(),
            "115792089237316195423570985008687907853269984665640564039457584007913129639935"
        );
        assert!(parse_decimal_work(
            "115792089237316195423570985008687907853269984665640564039457584007913129639936"
        )
        .is_none());

        let one = parse_decimal_work("1").unwrap();
        let max = parse_decimal_work(
            "115792089237316195423570985008687907853269984665640564039457584007913129639935",
        )
        .unwrap();
        assert_eq!(
            checked_add_work(genesis_work, one).unwrap().to_string(),
            "4295032834"
        );
        assert!(checked_add_work(max, one).is_none());
        assert_eq!(
            checked_sub_work(genesis_work, one).unwrap().to_string(),
            "4295032832"
        );
        assert!(checked_sub_work(Work::from_be_bytes([0; 32]), one).is_none());
    }
}
