use serde::{Deserialize, Serialize};

use crate::{ConxianError, ConxianResult};

const SETTLEMENT_ENVELOPE_VERSION_V2_LITERAL: &str = "2.0.0";

/// Institutional settlements above the regulatory threshold must be held in a burn-block timelock
/// window before any execution is permitted.
pub const INSTITUTIONAL_TIMELOCK_BURN_BLOCKS: u64 = 144;
pub const INSTITUTIONAL_ZAR_THRESHOLD_MAJOR: u64 = 100_000_000;

/// Current settlement envelope protocol version.
pub const SETTLEMENT_ENVELOPE_VERSION_CURRENT: &str = SETTLEMENT_ENVELOPE_VERSION_V2_LITERAL;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementSource {
    Iso20022Pacs008,
    Iso20022Pacs009,
    Papss,
    Brics,
    Erp,
}

impl SettlementSource {
    pub fn as_rail_name(&self) -> &'static str {
        match self {
            Self::Iso20022Pacs008 | Self::Iso20022Pacs009 => "ISO20022",
            Self::Papss => "PAPSS",
            Self::Brics => "BRICS",
            Self::Erp => "ERP",
        }
    }
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

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementFinality {
    Final,
    Provisional,
    #[default]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementStatus {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_to_end_id: Option<String>,
    pub settlement_amount: String,
    pub settlement_currency: String,
    pub settlement_date: String, // YYYY-MM-DD
}

/// CON-451: Industrial Intent Metadata for x402 alignment
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct IndustrialIntent {
    pub sector: String,
    pub project_id: String,
    pub x402_payment_required: bool,
    pub invoice_id: Option<String>,
    pub device_id: Option<String>,
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
    pub identifiers: SettlementIdentifiers,
    pub raw_payload_hash: String,
    #[serde(default)]
    pub industrial_intent: IndustrialIntent,
}

impl NormalizedSettlement {
    /// Returns `true` when this settlement requires an institutional timelock window.
    pub fn requires_institutional_timelock(&self) -> bool {
        if !self.currency.eq_ignore_ascii_case("ZAR") {
            return false;
        }

        match institutional_threshold_minor(self.amount_scale) {
            Some(threshold_minor) => u128::from(self.amount_minor) >= threshold_minor,
            None => true,
        }
    }
}

fn institutional_threshold_minor(scale: u32) -> Option<u128> {
    const MAX_SCALE: u32 = 38;
    if scale > MAX_SCALE {
        return None;
    }

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProductiveStreaming {
    pub founder_royalty_bps: u16,   // 5% (500 bps)
    pub ecosystem_reserve_bps: u16, // 5% (500 bps)
    pub productive_yield_bps: u16,  // 90% (9000 bps)
    pub is_active: bool,
}

impl Default for ProductiveStreaming {
    fn default() -> Self {
        Self {
            founder_royalty_bps: 500,
            ecosystem_reserve_bps: 500,
            productive_yield_bps: 9000,
            is_active: true,
        }
    }
}

/// CON-452: Structured Finance Tranches
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinanceTranche {
    #[default]
    Senior,
    Junior,
}

/// CON-452: Operational Loan Metadata
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct OpsLoanMetadata {
    pub loan_id: String,
    pub tranche: FinanceTranche,
    pub interest_accrued: f64,
    pub principal_remaining: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettlementProposal {
    pub proposal_id: String,
    pub envelope: SettlementEnvelope,
    pub tee_attestation: crate::AttestationRequest,
    pub stacks_burn_block_height: u64,
    pub timelock_release_burn_block_height: u64,
    pub created_at: u64,
    pub state: SettlementProposalState,
    pub streaming: ProductiveStreaming,
    pub ops_loan: Option<OpsLoanMetadata>,
}

impl SettlementProposal {
    pub fn new(
        proposal_id: String,
        envelope: SettlementEnvelope,
        tee_attestation: crate::AttestationRequest,
        stacks_burn_block_height: u64,
        created_at: u64,
    ) -> ConxianResult<Self> {
        let requires_timelock = envelope.payload.requires_institutional_timelock();

        let timelock_release_burn_block_height = if requires_timelock {
            stacks_burn_block_height
                .checked_add(INSTITUTIONAL_TIMELOCK_BURN_BLOCKS)
                .ok_or_else(|| {
                    ConxianError::Internal(format!(
                        "Burn-block timelock release height overflow (proposal_id={proposal_id}, transaction_id={}, raw_payload_hash={}, base={stacks_burn_block_height}, delta={INSTITUTIONAL_TIMELOCK_BURN_BLOCKS})",
                        envelope.payload.transaction_id,
                        envelope.payload.raw_payload_hash
                    ))
                })?
        } else {
            stacks_burn_block_height
        };

        let state = if requires_timelock {
            SettlementProposalState::Timelocked
        } else {
            SettlementProposalState::Proposed
        };

        Ok(Self {
            proposal_id,
            envelope,
            tee_attestation,
            stacks_burn_block_height,
            timelock_release_burn_block_height,
            created_at,
            state,
            streaming: ProductiveStreaming::default(),
            ops_loan: None,
        })
    }
}
