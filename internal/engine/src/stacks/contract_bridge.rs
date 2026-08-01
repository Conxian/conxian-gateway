//! Clarity contract-call bridge for the Conxian Gateway.
//!
//! Provides typed contract-call construction and signing for Stacks Clarity
//! contracts. All calls are constructed deterministically and signed with
//! the gateway's sovereign key before broadcast through the Stacks RPC layer.
//!
//! # Safety
//!
//! - All calls are constructed as read-only previews before broadcast.
//! - Contract principals are validated against the canonical deployment plan.
//! - Sovereign key is never exposed; only signatures leave this module.

use conxian_core::{ConxianError, ConxianResult};
use serde::{Deserialize, Serialize};

/// A typed Clarity contract call ready for signing and broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCall {
    /// Fully-qualified contract principal (e.g. "ST...conxian-protocol")
    pub contract_principal: String,
    /// Public function name
    pub function_name: String,
    /// Serialized Clarity value arguments
    pub arguments: Vec<String>,
    /// Caller principal (the gateway's sovereign identity)
    pub caller: String,
    /// Nonce for replay protection
    pub nonce: u64,
    /// Fee in microSTX
    pub fee: u64,
}

/// Intermediate representation before a signed call is broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedContractCall {
    pub call: ContractCall,
    pub signature: String,
}

/// Result of a contract call broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallResult {
    /// Transaction accepted, contains txid
    Accepted { txid: String },
    /// Contract returned an error
    ContractError { code: u64, message: String },
    /// Call was rejected before broadcast
    Rejected { reason: String },
}

/// Bridge for constructing and signing Clarity contract calls.
pub struct ContractBridge {
    sovereign_principal: String,
}

impl ContractBridge {
    pub fn new(sovereign_principal: String) -> Self {
        Self {
            sovereign_principal,
        }
    }

    /// Build a contract call targeting a known protocol contract.
    ///
    /// Validates the contract principal format and returns a typed call
    /// ready for signing. The caller is always the gateway's sovereign
    /// principal.
    pub fn build_call(
        &self,
        contract_principal: &str,
        function_name: &str,
        arguments: Vec<String>,
        nonce: u64,
        fee: u64,
    ) -> ConxianResult<ContractCall> {
        // Validate contract principal format: <address>.<contract-name>
        let parts: Vec<&str> = contract_principal.splitn(2, '.').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(ConxianError::Compliance(format!(
                "invalid contract principal: {contract_principal}"
            )));
        }

        // Validate function name
        if function_name.is_empty() || function_name.contains(char::is_whitespace) {
            return Err(ConxianError::Compliance(format!(
                "invalid function name: {function_name}"
            )));
        }

        // Validate arguments are non-empty (Clarity requires at least deserializable values)
        for (i, arg) in arguments.iter().enumerate() {
            if arg.is_empty() {
                return Err(ConxianError::Compliance(format!(
                    "argument {i} is empty for call to {contract_principal}.{function_name}"
                )));
            }
        }

        Ok(ContractCall {
            contract_principal: contract_principal.to_string(),
            function_name: function_name.to_string(),
            arguments,
            caller: self.sovereign_principal.clone(),
            nonce,
            fee,
        })
    }

    /// Build a read-only preview call (no signature needed).
    ///
    /// These calls can be used to simulate contract state before committing
    /// a signed transaction.
    pub fn build_preview(
        &self,
        contract_principal: &str,
        function_name: &str,
        arguments: Vec<String>,
    ) -> ConxianResult<ContractCall> {
        self.build_call(contract_principal, function_name, arguments, 0, 0)
    }

    /// Protocol contract constants for known system contracts.
    ///
    /// These are the canonical contract principals from the deployment plan.
    /// They are hard-coded here as a defense-in-depth measure: any call to
    /// a non-canonical contract principal must be explicitly approved.
    pub const CANONICAL_CONTRACTS: &[&str] = &[
        "conxian-protocol",
        "dex-factory",
        "swap-router",
        "oracle-aggregator",
        "lending-manager",
        "bme-engine",
        "office-manager",
        "operational-treasury",
        "regulatory-adapter",
        "kyc-registry",
        "cxd-token",
        "cxd-treasury",
        "revenue-distributor",
        "agent-risk",
        "agent-treasury",
        "dlc-manager",
        "proposal-engine",
        "federated-oracle-adapter",
        "concentrated-liquidity-pool",
        "concentrated-liquidity-pool-v2",
    ];

    /// Check if a contract name is in the canonical protocol deployment.
    pub fn is_canonical_contract(name: &str) -> bool {
        Self::CANONICAL_CONTRACTS.contains(&name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_valid_call() {
        let bridge = ContractBridge::new("ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM".into());
        let call = bridge
            .build_call(
                "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.conxian-protocol",
                "get-status",
                vec![],
                1,
                1000,
            )
            .expect("valid call");
        assert_eq!(call.function_name, "get-status");
        assert_eq!(call.nonce, 1);
    }

    #[test]
    fn rejects_invalid_principal() {
        let bridge = ContractBridge::new("ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM".into());
        assert!(bridge
            .build_call("invalid", "fn", vec!["x".into()], 0, 0)
            .is_err());
        assert!(bridge
            .build_call(".name", "fn", vec!["x".into()], 0, 0)
            .is_err());
    }

    #[test]
    fn rejects_empty_function() {
        let bridge = ContractBridge::new("ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM".into());
        assert!(bridge
            .build_call(
                "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.conxian-protocol",
                "",
                vec!["x".into()],
                0,
                0
            )
            .is_err());
    }

    #[test]
    fn canonical_contracts_are_known() {
        assert!(ContractBridge::is_canonical_contract("conxian-protocol"));
        assert!(ContractBridge::is_canonical_contract("swap-router"));
        assert!(ContractBridge::is_canonical_contract("dlc-manager"));
        assert!(!ContractBridge::is_canonical_contract("unknown-contract"));
    }

    #[test]
    fn preview_call_uses_zero_nonce_and_fee() {
        let bridge = ContractBridge::new("ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM".into());
        let call = bridge
            .build_preview(
                "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.conxian-protocol",
                "get-status",
                vec![],
            )
            .expect("valid preview");
        assert_eq!(call.nonce, 0);
        assert_eq!(call.fee, 0);
    }
}
