use serde::{Deserialize, Serialize};

const SETTLEMENT_ENVELOPE_VERSION_V2_LITERAL: &str = "2.0.0";

/// Current settlement envelope protocol version.
///
/// Serialized into `SettlementEnvelope::version` when the gateway constructs a new settlement
/// envelope.
pub const SETTLEMENT_ENVELOPE_VERSION_CURRENT: &str = SETTLEMENT_ENVELOPE_VERSION_V2_LITERAL;

/// Deprecated alias for the settlement envelope protocol v2.
#[deprecated(note = "Use SETTLEMENT_ENVELOPE_VERSION_CURRENT instead")]
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
pub enum SettlementStatus {
    #[serde(alias = "Ingested")]
    Ingested,
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
    pub raw_payload_hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettlementEnvelope {
    pub version: String,
    pub payload: NormalizedSettlement,
}
