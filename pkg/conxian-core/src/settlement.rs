use serde::{Deserialize, Serialize};

const SETTLEMENT_ENVELOPE_VERSION_V2_LITERAL: &str = "2.0.0";

/// Institutional settlements above the regulatory threshold must be held in a burn-block timelock
/// window before any execution is permitted.
pub const INSTITUTIONAL_TIMELOCK_BURN_BLOCKS: u64 = 144;
pub const INSTITUTIONAL_ZAR_THRESHOLD_MAJOR: u64 = 100_000_000;

/// Current settlement envelope protocol version.
///
/// Serialized into `SettlementEnvelope::version` when the gateway constructs a new settlement
/// envelope.
pub const SETTLEMENT_ENVELOPE_VERSION_CURRENT: &str = SETTLEMENT_ENVELOPE_VERSION_V2_LITERAL;

/// Deprecated alias for the settlement envelope protocol v2.
#[deprecated(
    since = "0.1.0",
    note = "Use SETTLEMENT_ENVELOPE_VERSION_CURRENT instead"
)]
pub const SETTLEMENT_ENVELOPE_VERSION_V2: &str = SETTLEMENT_ENVELOPE_VERSION_V2_LITERAL;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SettlementSource {
    Iso20022Pacs008,
    Iso20022Pacs009,
    Papss,
    Brics,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementRailFamily {
    Rtgs,
    Instant,
    Ach,
    Netting,
    Other,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettlementRail {
    pub family: SettlementRailFamily,
    pub name: String,
    pub region: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementFinality {
    Final,
    Provisional,
    Unknown,
}

impl Default for SettlementFinality {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementStatus {
    #[serde(alias = "Ingested")]
    Ingested,
    Accepted,
    Rejected,
    Settled,
    Returned,
    Reversed,
    Recalled,
}

impl SettlementStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "INGESTED" => Some(Self::Ingested),
            "ACCEPTED" => Some(Self::Accepted),
            "REJECTED" => Some(Self::Rejected),
            "SETTLED" => Some(Self::Settled),
            "RETURNED" => Some(Self::Returned),
            "REVERSED" => Some(Self::Reversed),
            "RECALLED" => Some(Self::Recalled),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SettlementIdentifiers {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_to_end_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uetr: Option<String>,
}

impl SettlementIdentifiers {
    fn is_empty(&self) -> bool {
        self.msg_id.is_none()
            && self.instruction_id.is_none()
            && self.end_to_end_id.is_none()
            && self.uetr.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NormalizedSettlement {
    pub source: SettlementSource,
    pub transaction_id: String,
    pub amount_minor: u64,
    pub amount_scale: u32,
    pub currency: String,
    pub sender: String,
    pub receiver: String,
    pub timestamp: u64,
    pub status: SettlementStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rail: Option<SettlementRail>,
    #[serde(default)]
    pub finality: SettlementFinality,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled_at: Option<u64>,
    #[serde(default, skip_serializing_if = "SettlementIdentifiers::is_empty")]
    pub identifiers: SettlementIdentifiers,
    pub raw_payload_hash: String,
}

impl NormalizedSettlement {
    pub fn requires_institutional_timelock(&self) -> bool {
        if !self.currency.eq_ignore_ascii_case("ZAR") {
            return false;
        }

        let Some(threshold_minor) = institutional_threshold_minor(self.amount_scale) else {
            return true;
        };

        u128::from(self.amount_minor) >= threshold_minor
    }
}

fn institutional_threshold_minor(scale: u32) -> Option<u128> {
    let factor = 10u128.checked_pow(scale)?;
    u128::from(INSTITUTIONAL_ZAR_THRESHOLD_MAJOR).checked_mul(factor)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettlementEnvelope {
    pub version: String,
    pub payload: NormalizedSettlement,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementProposalState {
    Proposed,
    Timelocked,
    Ready,
    Executed,
    Rejected,
}

impl Default for SettlementProposalState {
    fn default() -> Self {
        Self::Proposed
    }
}

/// A proposal-only representation of an external settlement signal.
///
/// This is intentionally separate from any execution path: consuming services are expected to map
/// proposals into the existing multi-sig + timelock executor when/if the proposal is approved.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettlementProposal {
    pub proposal_id: String,
    pub envelope: SettlementEnvelope,
    pub tee_attestation: crate::AttestationRequest,
    pub stacks_burn_block_height: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timelock_release_burn_block_height: Option<u64>,
    pub created_at: u64,
    #[serde(default)]
    pub state: SettlementProposalState,
}

impl SettlementProposal {
    pub fn new(
        envelope: SettlementEnvelope,
        tee_attestation: crate::AttestationRequest,
        stacks_burn_block_height: u64,
        created_at: u64,
    ) -> Self {
        let timelock_release_burn_block_height = envelope
            .payload
            .requires_institutional_timelock()
            .then(|| stacks_burn_block_height.saturating_add(INSTITUTIONAL_TIMELOCK_BURN_BLOCKS));

        let state = if timelock_release_burn_block_height.is_some() {
            SettlementProposalState::Timelocked
        } else {
            SettlementProposalState::Proposed
        };

        Self {
            proposal_id: envelope.payload.raw_payload_hash.clone(),
            envelope,
            tee_attestation,
            stacks_burn_block_height,
            timelock_release_burn_block_height,
            created_at,
            state,
        }
    }
}
