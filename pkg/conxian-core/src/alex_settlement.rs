//! Typed, fail-closed ALEX settlement-intent policy boundary.
//!
//! This module deliberately stops before signing, broadcasting, custody, and
//! reconciliation. It validates exact network-qualified identifiers and
//! produces an unsigned intent plus a deterministic hash that can become the
//! key for later persistence/reconciliation. The hash alone is not replay
//! prevention because this slice has no persistence.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The only ALEX settlement networks accepted by this policy boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlexNetwork {
    Mainnet,
    Testnet,
}

impl AlexNetwork {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
        }
    }

    const fn principal_prefix(self) -> &'static str {
        match self {
            Self::Mainnet => "SP",
            Self::Testnet => "ST",
        }
    }
}

/// Classification of the source used to construct an ALEX quote or venue
/// snapshot. Only observed data can produce an unsigned-preparation status;
/// fixture and unverified data are explicitly shadow-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlexSourceClass {
    Observed,
    Fixture,
    Unverified,
}

impl AlexSourceClass {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::Fixture => "fixture",
            Self::Unverified => "unverified",
        }
    }
}

/// Minimal source metadata. References, credentials, and PII are intentionally
/// not carried by the intent contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexSourceSnapshot {
    pub classification: AlexSourceClass,
    pub observed_at_epoch_secs: u64,
}

/// A network-qualified exact Stacks principal.
///
/// The constructor validates the network prefix and principal shape, while
/// policy evaluation also compares the value against an exact allowlist. A
/// ticker such as `sBTC` or `STX` is never accepted as an asset reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexPrincipal {
    pub network: AlexNetwork,
    pub value: String,
}

impl AlexPrincipal {
    pub fn new(
        network: AlexNetwork,
        value: impl Into<String>,
    ) -> Result<Self, AlexPolicyRejection> {
        let principal = Self {
            network,
            value: value.into(),
        };
        principal.validate()?;
        Ok(principal)
    }

    pub fn validate(&self) -> Result<(), AlexPolicyRejection> {
        validate_principal(self.network, &self.value)
    }

    pub fn address(&self) -> &str {
        self.value
            .split_once('.')
            .map(|(address, _)| address)
            .unwrap_or(&self.value)
    }

    pub fn contract_name(&self) -> Option<&str> {
        self.value.split_once('.').map(|(_, contract)| contract)
    }
}

fn validate_principal(network: AlexNetwork, value: &str) -> Result<(), AlexPolicyRejection> {
    if value.is_empty() || value.trim() != value {
        return Err(AlexPolicyRejection::MalformedPrincipal);
    }

    let mut parts = value.split('.');
    let address = parts.next().unwrap_or_default();
    let contract = parts.next();
    if parts.next().is_some() {
        return Err(AlexPolicyRejection::MalformedPrincipal);
    }

    // Stacks c32 addresses vary in length when leading zeroes are present, but
    // are materially longer than a ticker or short placeholder. Exact
    // allowlisting remains the authoritative deployment check.
    if address.len() < 20
        || address.len() > 42
        || !address.starts_with(network.principal_prefix())
        || !address
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(AlexPolicyRejection::MalformedPrincipal);
    }

    if let Some(contract) = contract {
        if contract.is_empty()
            || contract.len() > 128
            || !contract
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || !contract
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic())
        {
            return Err(AlexPolicyRejection::MalformedPrincipal);
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexAssetRef {
    pub principal: AlexPrincipal,
}

impl AlexAssetRef {
    pub fn new(principal: AlexPrincipal) -> Self {
        Self { principal }
    }

    fn validate(&self) -> Result<(), AlexPolicyRejection> {
        self.principal.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexPoolRef {
    pub principal: AlexPrincipal,
}

impl AlexPoolRef {
    pub fn new(principal: AlexPrincipal) -> Self {
        Self { principal }
    }

    fn validate(&self) -> Result<(), AlexPolicyRejection> {
        self.principal.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexHelperRef {
    pub principal: AlexPrincipal,
}

impl AlexHelperRef {
    pub fn new(principal: AlexPrincipal) -> Self {
        Self { principal }
    }

    fn validate(&self) -> Result<(), AlexPolicyRejection> {
        self.principal.validate()?;
        if self.principal.contract_name().is_none() {
            return Err(AlexPolicyRejection::MalformedPrincipal);
        }
        Ok(())
    }
}

/// One exact asset pair/pool/helper tuple allowed by policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexVenueAllowlistEntry {
    pub asset_in: AlexAssetRef,
    pub asset_out: AlexAssetRef,
    pub pool: AlexPoolRef,
    pub helper: AlexHelperRef,
}

impl AlexVenueAllowlistEntry {
    fn validate(&self, network: AlexNetwork) -> Result<(), AlexPolicyRejection> {
        self.asset_in.validate()?;
        self.asset_out.validate()?;
        self.pool.validate()?;
        self.helper.validate()?;

        for principal_network in [
            self.asset_in.principal.network,
            self.asset_out.principal.network,
            self.pool.principal.network,
            self.helper.principal.network,
        ] {
            if principal_network != network {
                return Err(AlexPolicyRejection::NetworkMismatch);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlexAdminState {
    KnownActive,
    Paused,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexVenueSnapshot {
    pub network: AlexNetwork,
    pub asset_in: AlexAssetRef,
    pub asset_out: AlexAssetRef,
    pub pool: AlexPoolRef,
    pub helper: AlexHelperRef,
    pub config_revision: String,
    pub helper_code_hash: String,
    pub admin_state: AlexAdminState,
    pub source: AlexSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexQuoteSnapshot {
    pub amount_in: u128,
    pub amount_out: u128,
    pub quoted_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
    pub price_impact_bps: u32,
    pub source: AlexSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexExposureSnapshot {
    pub before: u128,
    pub after: u128,
    pub cap: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexSettlementRequest {
    pub network: AlexNetwork,
    pub asset_in: AlexAssetRef,
    pub asset_out: AlexAssetRef,
    pub pool: AlexPoolRef,
    pub helper: AlexHelperRef,
    pub amount_in: u128,
    /// Optional at the conversion boundary so missing and zero values can be
    /// rejected distinctly instead of receiving a default.
    pub min_dy: Option<u128>,
    pub quote: AlexQuoteSnapshot,
    pub venue: AlexVenueSnapshot,
    pub policy_revision: String,
    pub requested_at_epoch_secs: u64,
    pub exposure: AlexExposureSnapshot,
}

/// The issue's proposed 20% exposure limit is represented only as an upper
/// safety bound. A concrete policy must still provide `max_exposure_bps`.
pub const ALEX_EXPOSURE_SAFETY_CEILING_BPS: u32 = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexSettlementPolicy {
    pub policy_id: String,
    pub policy_revision: String,
    pub supported_network: AlexNetwork,
    pub allowed_venues: Vec<AlexVenueAllowlistEntry>,
    pub expected_config_revision: String,
    pub expected_helper_code_hash: String,
    pub max_quote_age_secs: u64,
    pub max_price_impact_bps: u32,
    pub max_exposure_bps: u32,
    #[serde(default = "default_exposure_safety_ceiling_bps")]
    pub exposure_safety_ceiling_bps: u32,
}

const fn default_exposure_safety_ceiling_bps() -> u32 {
    ALEX_EXPOSURE_SAFETY_CEILING_BPS
}

impl AlexSettlementPolicy {
    pub fn evaluate(
        &self,
        request: &AlexSettlementRequest,
        now_epoch_secs: u64,
    ) -> Result<AlexUnsignedSettlementIntent, AlexPolicyRejection> {
        self.validate()?;

        if request.network != self.supported_network {
            return Err(AlexPolicyRejection::UnsupportedNetwork);
        }
        if request.policy_revision != self.policy_revision {
            return Err(AlexPolicyRejection::StalePolicyRevision);
        }

        validate_request_principals(request)?;
        if request.venue.asset_in != request.asset_in
            || request.venue.asset_out != request.asset_out
            || request.venue.pool != request.pool
            || request.venue.helper != request.helper
        {
            return Err(AlexPolicyRejection::VenueSnapshotMismatch);
        }
        self.validate_allowlist(request)?;

        if request.amount_in == 0 {
            return Err(AlexPolicyRejection::InvalidAmount);
        }

        let min_dy = request
            .min_dy
            .ok_or(AlexPolicyRejection::MissingMinimumOutput)?;
        if min_dy == 0 {
            return Err(AlexPolicyRejection::ZeroMinimumOutput);
        }

        if request.quote.amount_in != request.amount_in {
            return Err(AlexPolicyRejection::QuoteAmountMismatch);
        }
        if request.quote.quoted_at_epoch_secs > now_epoch_secs
            || request.quote.source.observed_at_epoch_secs > now_epoch_secs
            || request.venue.source.observed_at_epoch_secs > now_epoch_secs
        {
            return Err(AlexPolicyRejection::QuoteFromFuture);
        }
        if request.quote.expires_at_epoch_secs <= now_epoch_secs
            || request.quote.expires_at_epoch_secs <= request.quote.quoted_at_epoch_secs
        {
            return Err(AlexPolicyRejection::QuoteExpired);
        }
        if now_epoch_secs.saturating_sub(request.quote.quoted_at_epoch_secs)
            > self.max_quote_age_secs
        {
            return Err(AlexPolicyRejection::QuoteStale);
        }
        if min_dy > request.quote.amount_out {
            return Err(AlexPolicyRejection::MinimumOutputExceedsQuote);
        }

        if request.venue.config_revision != self.expected_config_revision {
            return Err(AlexPolicyRejection::StaleConfigRevision);
        }
        if request.venue.helper_code_hash != self.expected_helper_code_hash {
            return Err(AlexPolicyRejection::WrongCodeHash);
        }

        match request.venue.admin_state {
            AlexAdminState::KnownActive => {}
            AlexAdminState::Paused => return Err(AlexPolicyRejection::Paused),
            AlexAdminState::Unknown => return Err(AlexPolicyRejection::UnknownAdminState),
        }

        if request.quote.price_impact_bps > self.max_price_impact_bps {
            return Err(AlexPolicyRejection::PriceImpactExceeded);
        }

        if request.exposure.cap == 0 {
            return Err(AlexPolicyRejection::InvalidExposureCap);
        }
        let exposure_after_bps = request
            .exposure
            .after
            .checked_mul(10_000)
            .ok_or(AlexPolicyRejection::InvalidExposureCap)?;
        let allowed_exposure = request
            .exposure
            .cap
            .checked_mul(self.max_exposure_bps as u128)
            .ok_or(AlexPolicyRejection::InvalidExposureCap)?;
        if exposure_after_bps > allowed_exposure {
            return Err(AlexPolicyRejection::ExposureCapExceeded);
        }

        let decision = if request.quote.source.classification == AlexSourceClass::Observed
            && request.venue.source.classification == AlexSourceClass::Observed
        {
            AlexPolicyDecision::AllowUnsigned
        } else {
            AlexPolicyDecision::AllowShadow
        };
        let status = match decision {
            AlexPolicyDecision::AllowUnsigned => AlexIntentStatus::UnsignedPrepared,
            AlexPolicyDecision::AllowShadow => AlexIntentStatus::ShadowOnly,
            AlexPolicyDecision::Reject(_) => {
                unreachable!("rejected decisions do not build intents")
            }
        };

        let mut intent = AlexUnsignedSettlementIntent {
            intent_version: "alex-unsigned-intent-v1".to_string(),
            policy_id: self.policy_id.clone(),
            policy_revision: self.policy_revision.clone(),
            network: request.network,
            asset_in: request.asset_in.clone(),
            asset_out: request.asset_out.clone(),
            pool: request.pool.clone(),
            helper: request.helper.clone(),
            amount_in: request.amount_in,
            min_dy,
            quote: request.quote.clone(),
            venue: request.venue.clone(),
            requested_at_epoch_secs: request.requested_at_epoch_secs,
            expires_at_epoch_secs: request.quote.expires_at_epoch_secs,
            exposure: request.exposure.clone(),
            decision,
            status,
            intent_hash: String::new(),
        };
        intent.intent_hash = intent.compute_intent_hash();
        Ok(intent)
    }

    fn validate(&self) -> Result<(), AlexPolicyRejection> {
        if self.policy_id.trim().is_empty()
            || self.policy_revision.trim().is_empty()
            || self.expected_config_revision.trim().is_empty()
            || self.expected_helper_code_hash.trim().is_empty()
            || self.allowed_venues.is_empty()
            || self.max_quote_age_secs == 0
            || self.max_price_impact_bps > 10_000
            || self.exposure_safety_ceiling_bps != ALEX_EXPOSURE_SAFETY_CEILING_BPS
            || self.max_exposure_bps > self.exposure_safety_ceiling_bps
        {
            return Err(AlexPolicyRejection::InvalidPolicyConfig);
        }

        for venue in &self.allowed_venues {
            venue.validate(self.supported_network)?;
        }
        Ok(())
    }

    fn validate_allowlist(
        &self,
        request: &AlexSettlementRequest,
    ) -> Result<(), AlexPolicyRejection> {
        let asset_pair_matches = self.allowed_venues.iter().any(|venue| {
            venue.asset_in == request.asset_in && venue.asset_out == request.asset_out
        });
        if !asset_pair_matches {
            return Err(AlexPolicyRejection::AssetNotAllowlisted);
        }

        let pool_matches = self.allowed_venues.iter().any(|venue| {
            venue.asset_in == request.asset_in
                && venue.asset_out == request.asset_out
                && venue.pool == request.pool
        });
        if !pool_matches {
            return Err(AlexPolicyRejection::PoolNotAllowlisted);
        }

        let exact_match = self.allowed_venues.iter().any(|venue| {
            venue.asset_in == request.asset_in
                && venue.asset_out == request.asset_out
                && venue.pool == request.pool
                && venue.helper == request.helper
        });
        if !exact_match {
            return Err(AlexPolicyRejection::HelperNotAllowlisted);
        }

        Ok(())
    }
}

fn validate_request_principals(request: &AlexSettlementRequest) -> Result<(), AlexPolicyRejection> {
    if request.asset_in.principal.network != request.network
        || request.asset_out.principal.network != request.network
        || request.pool.principal.network != request.network
        || request.helper.principal.network != request.network
        || request.venue.network != request.network
    {
        return Err(AlexPolicyRejection::NetworkMismatch);
    }

    request.asset_in.validate()?;
    request.asset_out.validate()?;
    request.pool.validate()?;
    request.helper.validate()?;
    request.venue.asset_in.validate()?;
    request.venue.asset_out.validate()?;
    request.venue.pool.validate()?;
    request.venue.helper.validate()?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlexPolicyDecision {
    AllowUnsigned,
    AllowShadow,
    Reject(AlexPolicyRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlexIntentStatus {
    Rejected,
    ShadowOnly,
    UnsignedPrepared,
    Signed,
    Broadcast,
    Settled,
    Completed,
}

impl AlexIntentStatus {
    pub const fn is_settled_or_completed(self) -> bool {
        matches!(self, Self::Settled | Self::Completed)
    }
}

/// Unsigned ALEX intent produced after policy checks. This type does not
/// contain a signature, broadcast response, receipt, or reconciliation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlexUnsignedSettlementIntent {
    pub intent_version: String,
    pub policy_id: String,
    pub policy_revision: String,
    pub network: AlexNetwork,
    pub asset_in: AlexAssetRef,
    pub asset_out: AlexAssetRef,
    pub pool: AlexPoolRef,
    pub helper: AlexHelperRef,
    pub amount_in: u128,
    pub min_dy: u128,
    pub quote: AlexQuoteSnapshot,
    pub venue: AlexVenueSnapshot,
    pub requested_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
    pub exposure: AlexExposureSnapshot,
    pub decision: AlexPolicyDecision,
    pub status: AlexIntentStatus,
    /// SHA-256 over the versioned canonical fields below. This is a dedupe key
    /// for future persistence/reconciliation, not replay prevention by itself.
    pub intent_hash: String,
}

impl AlexUnsignedSettlementIntent {
    pub fn compute_intent_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"conxian:alex:unsigned-intent:v1\0");
        hasher.update(self.canonical_fields());
        hex::encode(hasher.finalize())
    }

    pub fn dedupe_key(&self) -> String {
        format!("alex-intent-v1:{}", self.intent_hash)
    }

    fn canonical_fields(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        put_str(&mut bytes, &self.intent_version);
        put_str(&mut bytes, &self.policy_id);
        put_str(&mut bytes, &self.policy_revision);
        put_str(&mut bytes, self.network.as_str());
        put_principal(&mut bytes, &self.asset_in.principal);
        put_principal(&mut bytes, &self.asset_out.principal);
        put_principal(&mut bytes, &self.pool.principal);
        put_principal(&mut bytes, &self.helper.principal);
        put_u128(&mut bytes, self.amount_in);
        put_u128(&mut bytes, self.min_dy);
        put_u128(&mut bytes, self.quote.amount_in);
        put_u128(&mut bytes, self.quote.amount_out);
        put_u64(&mut bytes, self.quote.quoted_at_epoch_secs);
        put_u64(&mut bytes, self.quote.expires_at_epoch_secs);
        put_u32(&mut bytes, self.quote.price_impact_bps);
        put_source(&mut bytes, &self.quote.source);
        put_str(&mut bytes, self.venue.network.as_str());
        put_principal(&mut bytes, &self.venue.asset_in.principal);
        put_principal(&mut bytes, &self.venue.asset_out.principal);
        put_principal(&mut bytes, &self.venue.pool.principal);
        put_principal(&mut bytes, &self.venue.helper.principal);
        put_str(&mut bytes, &self.venue.config_revision);
        put_str(&mut bytes, &self.venue.helper_code_hash);
        put_u8(
            &mut bytes,
            match self.venue.admin_state {
                AlexAdminState::KnownActive => 0,
                AlexAdminState::Paused => 1,
                AlexAdminState::Unknown => 2,
            },
        );
        put_source(&mut bytes, &self.venue.source);
        put_u64(&mut bytes, self.requested_at_epoch_secs);
        put_u64(&mut bytes, self.expires_at_epoch_secs);
        put_u128(&mut bytes, self.exposure.before);
        put_u128(&mut bytes, self.exposure.after);
        put_u128(&mut bytes, self.exposure.cap);
        put_u8(
            &mut bytes,
            match self.decision {
                AlexPolicyDecision::AllowUnsigned => 0,
                AlexPolicyDecision::AllowShadow => 1,
                AlexPolicyDecision::Reject(_) => 2,
            },
        );
        bytes
    }
}

fn put_principal(bytes: &mut Vec<u8>, principal: &AlexPrincipal) {
    put_str(bytes, principal.network.as_str());
    put_str(bytes, &principal.value);
}

fn put_source(bytes: &mut Vec<u8>, source: &AlexSourceSnapshot) {
    put_str(bytes, source.classification.as_str());
    put_u64(bytes, source.observed_at_epoch_secs);
}

fn put_str(bytes: &mut Vec<u8>, value: &str) {
    put_u64(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn put_u8(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

fn put_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn put_u128(bytes: &mut Vec<u8>, value: u128) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlexPolicyRejection {
    #[error("ALEX policy rejected: unsupported network")]
    UnsupportedNetwork,
    #[error("ALEX policy rejected: network-qualified identifier mismatch")]
    NetworkMismatch,
    #[error("ALEX policy rejected: malformed exact principal")]
    MalformedPrincipal,
    #[error("ALEX policy rejected: asset pair is not allowlisted")]
    AssetNotAllowlisted,
    #[error("ALEX policy rejected: pool is not allowlisted")]
    PoolNotAllowlisted,
    #[error("ALEX policy rejected: helper is not allowlisted")]
    HelperNotAllowlisted,
    #[error("ALEX policy rejected: venue snapshot does not match request identifiers")]
    VenueSnapshotMismatch,
    #[error("ALEX policy rejected: amount-in must be nonzero")]
    InvalidAmount,
    #[error("ALEX policy rejected: min-dy is required")]
    MissingMinimumOutput,
    #[error("ALEX policy rejected: min-dy must be nonzero")]
    ZeroMinimumOutput,
    #[error("ALEX policy rejected: quote amount does not match amount-in")]
    QuoteAmountMismatch,
    #[error("ALEX policy rejected: quote timestamp is from the future")]
    QuoteFromFuture,
    #[error("ALEX policy rejected: quote is expired")]
    QuoteExpired,
    #[error("ALEX policy rejected: quote is stale")]
    QuoteStale,
    #[error("ALEX policy rejected: min-dy exceeds quoted output")]
    MinimumOutputExceedsQuote,
    #[error("ALEX policy rejected: policy revision is stale")]
    StalePolicyRevision,
    #[error("ALEX policy rejected: venue configuration revision is stale")]
    StaleConfigRevision,
    #[error("ALEX policy rejected: helper code hash does not match policy")]
    WrongCodeHash,
    #[error("ALEX policy rejected: venue is paused")]
    Paused,
    #[error("ALEX policy rejected: venue admin state is unknown")]
    UnknownAdminState,
    #[error("ALEX policy rejected: price impact exceeds policy")]
    PriceImpactExceeded,
    #[error("ALEX policy rejected: exposure cap is invalid")]
    InvalidExposureCap,
    #[error("ALEX policy rejected: exposure exceeds policy cap")]
    ExposureCapExceeded,
    #[error("ALEX policy rejected: policy configuration is invalid")]
    InvalidPolicyConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAINNET_ASSET_IN: &str = "SP000000000000000000002Q6VF78.usdcx";
    const MAINNET_ASSET_OUT: &str = "SP000000000000000000002Q6VF78.sbtc";
    const MAINNET_POOL: &str = "SP000000000000000000002Q6VF78.alex-pool";
    const MAINNET_HELPER: &str = "SP000000000000000000002Q6VF78.alex-helper";
    const TESTNET_ASSET_IN: &str = "ST000000000000000000002Q6VF78.usdcx";
    const TESTNET_ASSET_OUT: &str = "ST000000000000000000002Q6VF78.sbtc";
    const TESTNET_POOL: &str = "ST000000000000000000002Q6VF78.alex-pool";
    const TESTNET_HELPER: &str = "ST000000000000000000002Q6VF78.alex-helper";

    fn principal(network: AlexNetwork, value: &str) -> AlexPrincipal {
        AlexPrincipal::new(network, value).unwrap()
    }

    fn venue(network: AlexNetwork) -> AlexVenueAllowlistEntry {
        let (asset_in, asset_out, pool, helper) = match network {
            AlexNetwork::Mainnet => (
                MAINNET_ASSET_IN,
                MAINNET_ASSET_OUT,
                MAINNET_POOL,
                MAINNET_HELPER,
            ),
            AlexNetwork::Testnet => (
                TESTNET_ASSET_IN,
                TESTNET_ASSET_OUT,
                TESTNET_POOL,
                TESTNET_HELPER,
            ),
        };
        AlexVenueAllowlistEntry {
            asset_in: AlexAssetRef::new(principal(network, asset_in)),
            asset_out: AlexAssetRef::new(principal(network, asset_out)),
            pool: AlexPoolRef::new(principal(network, pool)),
            helper: AlexHelperRef::new(principal(network, helper)),
        }
    }

    fn snapshot(network: AlexNetwork, source: AlexSourceClass) -> AlexVenueSnapshot {
        let allowed = venue(network);
        AlexVenueSnapshot {
            network,
            asset_in: allowed.asset_in,
            asset_out: allowed.asset_out,
            pool: allowed.pool,
            helper: allowed.helper,
            config_revision: "config-r1".to_string(),
            helper_code_hash: "code-hash-r1".to_string(),
            admin_state: AlexAdminState::KnownActive,
            source: AlexSourceSnapshot {
                classification: source,
                observed_at_epoch_secs: 1_000,
            },
        }
    }

    fn policy(network: AlexNetwork) -> AlexSettlementPolicy {
        AlexSettlementPolicy {
            policy_id: "alex-policy".to_string(),
            policy_revision: "policy-r1".to_string(),
            supported_network: network,
            allowed_venues: vec![venue(network)],
            expected_config_revision: "config-r1".to_string(),
            expected_helper_code_hash: "code-hash-r1".to_string(),
            max_quote_age_secs: 60,
            max_price_impact_bps: 500,
            max_exposure_bps: 2_000,
            exposure_safety_ceiling_bps: ALEX_EXPOSURE_SAFETY_CEILING_BPS,
        }
    }

    fn request(network: AlexNetwork) -> AlexSettlementRequest {
        let allowed = venue(network);
        AlexSettlementRequest {
            network,
            asset_in: allowed.asset_in.clone(),
            asset_out: allowed.asset_out.clone(),
            pool: allowed.pool.clone(),
            helper: allowed.helper.clone(),
            amount_in: 100,
            min_dy: Some(90),
            quote: AlexQuoteSnapshot {
                amount_in: 100,
                amount_out: 100,
                quoted_at_epoch_secs: 1_000,
                expires_at_epoch_secs: 1_030,
                price_impact_bps: 100,
                source: AlexSourceSnapshot {
                    classification: AlexSourceClass::Observed,
                    observed_at_epoch_secs: 1_000,
                },
            },
            venue: snapshot(network, AlexSourceClass::Observed),
            policy_revision: "policy-r1".to_string(),
            requested_at_epoch_secs: 1_005,
            exposure: AlexExposureSnapshot {
                before: 0,
                after: 10,
                cap: 100,
            },
        }
    }

    fn evaluate(
        request: &AlexSettlementRequest,
    ) -> Result<AlexUnsignedSettlementIntent, AlexPolicyRejection> {
        policy(request.network).evaluate(request, 1_010)
    }

    #[test]
    fn allows_observed_request_as_unsigned_preparation() {
        let intent = evaluate(&request(AlexNetwork::Mainnet)).unwrap();
        assert_eq!(intent.decision, AlexPolicyDecision::AllowUnsigned);
        assert_eq!(intent.status, AlexIntentStatus::UnsignedPrepared);
        assert!(!intent.status.is_settled_or_completed());
        assert_eq!(intent.intent_hash.len(), 64);
        assert!(intent.dedupe_key().starts_with("alex-intent-v1:"));
    }

    #[test]
    fn rejects_wrong_network() {
        let value = request(AlexNetwork::Testnet);
        assert_eq!(
            policy(AlexNetwork::Mainnet).evaluate(&value, 1_010),
            Err(AlexPolicyRejection::UnsupportedNetwork)
        );
    }

    #[test]
    fn rejects_ticker_only_or_malformed_principals() {
        assert_eq!(
            AlexPrincipal::new(AlexNetwork::Mainnet, "sBTC"),
            Err(AlexPolicyRejection::MalformedPrincipal)
        );
        assert_eq!(
            AlexPrincipal::new(AlexNetwork::Mainnet, "SP123.bad principal"),
            Err(AlexPolicyRejection::MalformedPrincipal)
        );
    }

    #[test]
    fn rejects_wrong_asset_pool_and_helper() {
        let mut wrong_asset = request(AlexNetwork::Mainnet);
        wrong_asset.asset_in = AlexAssetRef::new(principal(
            AlexNetwork::Mainnet,
            "SP000000000000000000002Q6VF78.other-asset",
        ));
        wrong_asset.venue.asset_in = wrong_asset.asset_in.clone();
        assert_eq!(
            evaluate(&wrong_asset),
            Err(AlexPolicyRejection::AssetNotAllowlisted)
        );

        let mut wrong_pool = request(AlexNetwork::Mainnet);
        wrong_pool.pool = AlexPoolRef::new(principal(
            AlexNetwork::Mainnet,
            "SP000000000000000000002Q6VF78.other-pool",
        ));
        wrong_pool.venue.pool = wrong_pool.pool.clone();
        assert_eq!(
            evaluate(&wrong_pool),
            Err(AlexPolicyRejection::PoolNotAllowlisted)
        );

        let mut wrong_helper = request(AlexNetwork::Mainnet);
        wrong_helper.helper = AlexHelperRef::new(principal(
            AlexNetwork::Mainnet,
            "SP000000000000000000002Q6VF78.other-helper",
        ));
        wrong_helper.venue.helper = wrong_helper.helper.clone();
        assert_eq!(
            evaluate(&wrong_helper),
            Err(AlexPolicyRejection::HelperNotAllowlisted)
        );
    }

    #[test]
    fn rejects_missing_or_zero_minimum_output() {
        let mut missing = request(AlexNetwork::Mainnet);
        missing.min_dy = None;
        assert_eq!(
            evaluate(&missing),
            Err(AlexPolicyRejection::MissingMinimumOutput)
        );

        let mut zero = request(AlexNetwork::Mainnet);
        zero.min_dy = Some(0);
        assert_eq!(evaluate(&zero), Err(AlexPolicyRejection::ZeroMinimumOutput));
    }

    #[test]
    fn rejects_expired_and_stale_quotes() {
        let mut expired = request(AlexNetwork::Mainnet);
        expired.quote.expires_at_epoch_secs = 1_010;
        assert_eq!(evaluate(&expired), Err(AlexPolicyRejection::QuoteExpired));

        let mut stale = request(AlexNetwork::Mainnet);
        stale.quote.quoted_at_epoch_secs = 900;
        stale.quote.source.observed_at_epoch_secs = 900;
        stale.quote.expires_at_epoch_secs = 1_030;
        assert_eq!(evaluate(&stale), Err(AlexPolicyRejection::QuoteStale));
    }

    #[test]
    fn rejects_stale_policy_config_and_wrong_code_hash() {
        let mut stale_policy = request(AlexNetwork::Mainnet);
        stale_policy.policy_revision = "policy-old".to_string();
        assert_eq!(
            evaluate(&stale_policy),
            Err(AlexPolicyRejection::StalePolicyRevision)
        );

        let mut stale_config = request(AlexNetwork::Mainnet);
        stale_config.venue.config_revision = "config-old".to_string();
        assert_eq!(
            evaluate(&stale_config),
            Err(AlexPolicyRejection::StaleConfigRevision)
        );

        let mut wrong_hash = request(AlexNetwork::Mainnet);
        wrong_hash.venue.helper_code_hash = "code-hash-old".to_string();
        assert_eq!(
            evaluate(&wrong_hash),
            Err(AlexPolicyRejection::WrongCodeHash)
        );
    }

    #[test]
    fn rejects_paused_or_unknown_admin_state() {
        let mut paused = request(AlexNetwork::Mainnet);
        paused.venue.admin_state = AlexAdminState::Paused;
        assert_eq!(evaluate(&paused), Err(AlexPolicyRejection::Paused));

        let mut unknown = request(AlexNetwork::Mainnet);
        unknown.venue.admin_state = AlexAdminState::Unknown;
        assert_eq!(
            evaluate(&unknown),
            Err(AlexPolicyRejection::UnknownAdminState)
        );
    }

    #[test]
    fn rejects_price_impact_and_exposure_breach() {
        let mut price_impact = request(AlexNetwork::Mainnet);
        price_impact.quote.price_impact_bps = 501;
        assert_eq!(
            evaluate(&price_impact),
            Err(AlexPolicyRejection::PriceImpactExceeded)
        );

        let mut exposure = request(AlexNetwork::Mainnet);
        exposure.exposure.after = 21;
        assert_eq!(
            evaluate(&exposure),
            Err(AlexPolicyRejection::ExposureCapExceeded)
        );
    }

    #[test]
    fn fixture_and_unverified_sources_are_shadow_only() {
        let mut fixture = request(AlexNetwork::Mainnet);
        fixture.quote.source.classification = AlexSourceClass::Fixture;
        let fixture_intent = evaluate(&fixture).unwrap();
        assert_eq!(fixture_intent.decision, AlexPolicyDecision::AllowShadow);
        assert_eq!(fixture_intent.status, AlexIntentStatus::ShadowOnly);
        assert!(!fixture_intent.status.is_settled_or_completed());

        let mut unverified = request(AlexNetwork::Mainnet);
        unverified.venue.source.classification = AlexSourceClass::Unverified;
        let unverified_intent = evaluate(&unverified).unwrap();
        assert_eq!(unverified_intent.decision, AlexPolicyDecision::AllowShadow);
        assert_eq!(unverified_intent.status, AlexIntentStatus::ShadowOnly);
        assert!(!unverified_intent.status.is_settled_or_completed());
    }

    #[test]
    fn intent_hash_is_deterministic_and_changes_with_canonical_fields() {
        let first = evaluate(&request(AlexNetwork::Mainnet)).unwrap();
        let second = evaluate(&request(AlexNetwork::Mainnet)).unwrap();
        assert_eq!(first.intent_hash, second.intent_hash);
        assert_eq!(first.compute_intent_hash(), first.intent_hash);

        let mut changed_request = request(AlexNetwork::Mainnet);
        changed_request.min_dy = Some(91);
        let changed = evaluate(&changed_request).unwrap();
        assert_ne!(first.intent_hash, changed.intent_hash);
    }

    #[test]
    fn policy_rejects_exposure_policy_above_safety_ceiling() {
        let mut invalid = policy(AlexNetwork::Mainnet);
        invalid.max_exposure_bps = ALEX_EXPOSURE_SAFETY_CEILING_BPS + 1;
        assert_eq!(
            invalid.evaluate(&request(AlexNetwork::Mainnet), 1_010),
            Err(AlexPolicyRejection::InvalidPolicyConfig)
        );
    }
}
