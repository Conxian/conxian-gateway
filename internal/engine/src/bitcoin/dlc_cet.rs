//! DLC Contract Execution Transaction (CET) construction.
//!
//! This module provides CET building using the `dlc-manager` crate for
//! Discreet Log Contract state machine management.

#![cfg(feature = "dlc")]

use dlc_manager::contract::{
    Contract, ContractDescriptor, ContractInput, ContractOutcomeValue, OfferParams,
};
use dlc_manager::error::Error as DlcError;
use dlc_manager::payout_curve::PayoutFunction;
use dlc_manager::OracleInfo;
use bitcoin::OutPoint;
use serde::{Deserialize, Serialize};

/// CET construction request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CetRequest {
    pub offer_params: OfferParams,
    pub contract_descriptor: ContractDescriptor,
    pub oracle_info: OracleInfo,
    pub input: ContractInput,
}

/// CET construction response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CetResponse {
    pub contract_id: String,
    pub fund_txid: String,
    pub cet_outpoint: Option<String>,
}

/// DLC bond manager for CET lifecycle
pub struct DlcBondManager {
    manager: dlc_manager::ContractManager<dlc_manager::dummy_offchain_manager::DummyWallet>,
}

impl DlcBondManager {
    /// Create a new DLC bond manager
    pub fn new() -> Result<Self, DlcError> {
        let manager = dlc_manager::ContractManager::new()?;
        Ok(Self { manager })
    }

    /// Construct and track a new CET contract
    pub fn create_cet(
        &self,
        request: CetRequest,
    ) -> Result<CetResponse, DlcError> {
        let contract = self.manager.create_contract(
            &request.offer_params,
            &request.contract_descriptor,
            &request.oracle_info,
            &request.input,
        )?;

        let contract_id = contract.get_contract_id();
        let fund_tx = contract.get_funding_transaction();

        Ok(CetResponse {
            contract_id: contract_id.to_hex(),
            fund_txid: fund_tx.txid().to_hex(),
            cet_outpoint: None,
        })
    }

    /// Process CET execution after oracle attestation
    pub fn execute_cet(
        &self,
        contract_id: &[u8; 32],
        outcome: &ContractOutcomeValue,
    ) -> Result<OutPoint, DlcError> {
        self.manager.execute_contract(contract_id, outcome)
    }
}

impl Default for DlcBondManager {
    fn default() -> Self {
        Self::new().expect("DLC manager initialization failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cet_request_serialization() {
        let request = CetRequest {
            offer_params: OfferParams::default(),
            contract_descriptor: ContractDescriptor::TwoOfTwo {
                inputs: vec![],
                outputs: Default::default(),
            },
            oracle_info: OracleInfo {
                public_key: Default::default(),
                announcements: vec![],
            },
            input: ContractInput::BitVectorV1 {
                outcomes: Default::default(),
            },
        };
        
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("BitVectorV1") || serialized.contains("TwoOfTwo"));
    }
}
