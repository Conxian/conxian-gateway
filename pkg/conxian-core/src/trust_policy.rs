use serde::{Deserialize, Serialize};

pub const TRUST_METADATA_MISSING: &str = "TRUST_METADATA_MISSING";
pub const TRUST_METADATA_INVALID: &str = "TRUST_METADATA_INVALID";
pub const TRUST_METADATA_STALE: &str = "TRUST_METADATA_STALE";
pub const TRUST_POLICY_BLOCKED: &str = "TRUST_POLICY_BLOCKED";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustSystem {
    #[serde(rename = "IBC", alias = "ibc")]
    Ibc,
    #[serde(rename = "HYPERLANE", alias = "hyperlane")]
    Hyperlane,
    #[serde(
        rename = "LAYERZERO_V2",
        alias = "layerzero_v2",
        alias = "LAYER_ZERO_V2"
    )]
    LayerZeroV2,
    #[serde(
        rename = "WORMHOLE_NTT",
        alias = "wormhole_ntt",
        alias = "wormhole-ntt"
    )]
    WormholeNtt,
    #[serde(rename = "AXELAR", alias = "axelar")]
    Axelar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustTier {
    #[serde(rename = "T1", alias = "t1")]
    T1,
    #[serde(rename = "T2", alias = "t2")]
    T2,
    #[serde(rename = "T3", alias = "t3")]
    T3,
    #[serde(rename = "T4", alias = "t4")]
    T4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustPolicyContext {
    pub policy_id: String,
    pub policy_version: String,
    #[serde(default)]
    pub allowed_systems: Vec<TrustSystem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustEvidence {
    pub source: String,
    #[serde(default)]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustFreshness {
    pub observed_at_epoch_secs: u64,
    pub max_age_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustMetadata {
    pub system: TrustSystem,
    pub trust_tier: TrustTier,
    pub policy: TrustPolicyContext,
    pub evidence: TrustEvidence,
    pub freshness: TrustFreshness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPolicyReasonCode {
    MetadataMissing,
    MetadataInvalid,
    MetadataStale,
    PolicyBlocked,
}

impl TrustPolicyReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            TrustPolicyReasonCode::MetadataMissing => TRUST_METADATA_MISSING,
            TrustPolicyReasonCode::MetadataInvalid => TRUST_METADATA_INVALID,
            TrustPolicyReasonCode::MetadataStale => TRUST_METADATA_STALE,
            TrustPolicyReasonCode::PolicyBlocked => TRUST_POLICY_BLOCKED,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPolicyDecision {
    Allow,
    Block(TrustPolicyReasonCode),
}

impl TrustPolicyDecision {
    pub fn reason(self) -> Option<TrustPolicyReasonCode> {
        match self {
            TrustPolicyDecision::Allow => None,
            TrustPolicyDecision::Block(reason) => Some(reason),
        }
    }

    pub fn reason_code(self) -> Option<&'static str> {
        self.reason().map(TrustPolicyReasonCode::as_str)
    }
}

pub fn evaluate_trust_metadata_json(
    raw_metadata: Option<&str>,
    now_epoch_secs: u64,
) -> TrustPolicyDecision {
    let Some(raw_metadata) = raw_metadata
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return TrustPolicyDecision::Block(TrustPolicyReasonCode::MetadataMissing);
    };

    let metadata: TrustMetadata = match serde_json::from_str(raw_metadata) {
        Ok(metadata) => metadata,
        Err(_) => return TrustPolicyDecision::Block(TrustPolicyReasonCode::MetadataInvalid),
    };

    evaluate_trust_metadata(&metadata, now_epoch_secs)
}

pub fn evaluate_trust_metadata(
    metadata: &TrustMetadata,
    now_epoch_secs: u64,
) -> TrustPolicyDecision {
    if !is_metadata_fresh(metadata, now_epoch_secs) {
        return TrustPolicyDecision::Block(TrustPolicyReasonCode::MetadataStale);
    }

    if !is_system_allowed_for_tier(metadata) {
        return TrustPolicyDecision::Block(TrustPolicyReasonCode::PolicyBlocked);
    }

    TrustPolicyDecision::Allow
}

fn is_metadata_fresh(metadata: &TrustMetadata, now_epoch_secs: u64) -> bool {
    let observed = metadata.freshness.observed_at_epoch_secs;
    let max_age = metadata.freshness.max_age_secs;

    if max_age == 0 || observed > now_epoch_secs {
        return false;
    }

    now_epoch_secs.saturating_sub(observed) <= max_age
}

fn is_system_allowed_for_tier(metadata: &TrustMetadata) -> bool {
    match metadata.trust_tier {
        TrustTier::T1 => metadata.system == TrustSystem::Ibc,
        TrustTier::T2 => matches!(
            metadata.system,
            TrustSystem::Ibc
                | TrustSystem::Hyperlane
                | TrustSystem::LayerZeroV2
                | TrustSystem::WormholeNtt
                | TrustSystem::Axelar
        ),
        TrustTier::T3 => match metadata.system {
            TrustSystem::Ibc => metadata.policy.allowed_systems.contains(&TrustSystem::Ibc),
            TrustSystem::Hyperlane
            | TrustSystem::LayerZeroV2
            | TrustSystem::WormholeNtt
            | TrustSystem::Axelar => true,
        },
        TrustTier::T4 => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata(system: TrustSystem, trust_tier: TrustTier) -> TrustMetadata {
        TrustMetadata {
            system,
            trust_tier,
            policy: TrustPolicyContext {
                policy_id: "CON-791".to_string(),
                policy_version: "2026-06-01".to_string(),
                allowed_systems: vec![],
            },
            evidence: TrustEvidence {
                source: "unit-test".to_string(),
                reference: Some("test-ref".to_string()),
            },
            freshness: TrustFreshness {
                observed_at_epoch_secs: 1_700_000_000,
                max_age_secs: 300,
            },
        }
    }

    #[test]
    fn t1_allows_ibc_only() {
        let now = 1_700_000_100;
        let allowed = sample_metadata(TrustSystem::Ibc, TrustTier::T1);
        let denied = sample_metadata(TrustSystem::Hyperlane, TrustTier::T1);

        assert_eq!(
            evaluate_trust_metadata(&allowed, now),
            TrustPolicyDecision::Allow
        );
        assert_eq!(
            evaluate_trust_metadata(&denied, now),
            TrustPolicyDecision::Block(TrustPolicyReasonCode::PolicyBlocked)
        );
    }

    #[test]
    fn t2_allows_supported_systems() {
        let now = 1_700_000_100;
        let systems = [
            TrustSystem::Ibc,
            TrustSystem::Hyperlane,
            TrustSystem::LayerZeroV2,
            TrustSystem::WormholeNtt,
            TrustSystem::Axelar,
        ];

        for system in systems {
            let metadata = sample_metadata(system, TrustTier::T2);
            assert_eq!(
                evaluate_trust_metadata(&metadata, now),
                TrustPolicyDecision::Allow
            );
        }
    }

    #[test]
    fn t3_blocks_ibc_by_default() {
        let now = 1_700_000_100;
        let metadata = sample_metadata(TrustSystem::Ibc, TrustTier::T3);

        assert_eq!(
            evaluate_trust_metadata(&metadata, now),
            TrustPolicyDecision::Block(TrustPolicyReasonCode::PolicyBlocked)
        );
    }

    #[test]
    fn t3_allows_ibc_when_explicitly_listed() {
        let now = 1_700_000_100;
        let mut metadata = sample_metadata(TrustSystem::Ibc, TrustTier::T3);
        metadata.policy.allowed_systems.push(TrustSystem::Ibc);

        assert_eq!(
            evaluate_trust_metadata(&metadata, now),
            TrustPolicyDecision::Allow
        );
    }

    #[test]
    fn t4_is_always_blocked() {
        let now = 1_700_000_100;
        let metadata = sample_metadata(TrustSystem::Hyperlane, TrustTier::T4);

        assert_eq!(
            evaluate_trust_metadata(&metadata, now),
            TrustPolicyDecision::Block(TrustPolicyReasonCode::PolicyBlocked)
        );
    }

    #[test]
    fn missing_metadata_fails_closed() {
        let decision = evaluate_trust_metadata_json(None, 1_700_000_100);
        assert_eq!(
            decision,
            TrustPolicyDecision::Block(TrustPolicyReasonCode::MetadataMissing)
        );
    }

    #[test]
    fn invalid_metadata_fails_closed() {
        let decision = evaluate_trust_metadata_json(Some("not-json"), 1_700_000_100);
        assert_eq!(
            decision,
            TrustPolicyDecision::Block(TrustPolicyReasonCode::MetadataInvalid)
        );
    }

    #[test]
    fn stale_metadata_fails_closed() {
        let mut metadata = sample_metadata(TrustSystem::Ibc, TrustTier::T1);
        metadata.freshness.max_age_secs = 10;
        let raw = serde_json::to_string(&metadata).unwrap();

        let decision = evaluate_trust_metadata_json(Some(&raw), 1_700_000_100);
        assert_eq!(
            decision,
            TrustPolicyDecision::Block(TrustPolicyReasonCode::MetadataStale)
        );
    }
}
