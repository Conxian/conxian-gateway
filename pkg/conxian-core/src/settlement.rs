use serde::{Deserialize, Serialize};

const SETTLEMENT_ENVELOPE_VERSION_V2_LITERAL: &str = "2.0.0";

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettlementEnvelope {
    pub version: String,
    pub payload: NormalizedSettlement,
}
