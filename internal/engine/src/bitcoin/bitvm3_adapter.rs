//! BitVM3 Adapter — Garbled Circuits & Recursive Proof Verification
//!
//! **Research / Horizon-Scanning Only**
//!
//! Tracks [Gateway issue #189](https://github.com/Conxian/conxian-gateway/issues/189).
//! BitVM3 is a paper/protocol family (IACR ePrint 2026/933) that uses garbled
//! circuits and recursive proof verification. No stable SDK, release, audit,
//! or verified production deployment exists as of the last evidence refresh.
//!
//! This adapter reserves the ChainAdapter surface so that Gateway's adapter
//! registry can acknowledge the BitVM3 lane without implying production
//! readiness. All verification paths fail closed with
//! [`ConxianError::VerifierUnavailable`].
//!
//! ## Promotion gates (from canonical triage)
//!
//! Do not return production `verified: true` until all of these exist:
//!
//! - stable maintained API/release and reconciled license;
//! - explicit curve, circuit, VK registry, public-input, and root/state-transition contract;
//! - pairing, curve-point, and subgroup validation;
//! - positive and negative vectors, including mutated proof/input/root and malformed envelope cases;
//! - complete SPV/dispute/disablement semantics;
//! - reproducible resource measurements on approved hardware;
//! - independent security review and verified deployment evidence;
//! - explicit ownership for cryptographic verification, evidence normalization,
//!   enclave attestation, policy enforcement, and client presentation.
//!
//! Until then, this adapter is a horizon-scanning tracker and must not be wired
//! into settlement, compliance, or any value-bearing production path.

use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianError, ConxianResult};
use serde_json::{json, Value};
use tracing::warn;

/// BitVM3 protocol adapter — research-only, fail-closed.
///
/// Represents the BitVM3 lane (garbled circuits + recursive proof
/// verification) in the Gateway's adapter registry. All cryptographic
/// verification paths fail closed because no stable BitVM3 SDK, GC
/// backend, audited implementation, or verified production deployment
/// exists.
///
/// Tracked by [Gateway issue #189](https://github.com/Conxian/conxian-gateway/issues/189).
pub struct BitVm3Adapter {
    /// Configured Bitcoin network name, such as `mainnet` or `regtest`.
    pub network: String,
}

impl BitVm3Adapter {
    /// Construct a research-only BitVM3 adapter.
    ///
    /// The adapter acknowledges the BitVM3 lane structurally but does
    /// not perform garbled-circuit generation, recursive proof
    /// verification, or any cryptographic operation.
    pub fn new(network: String) -> Self {
        Self { network }
    }
}

#[async_trait]
impl ChainAdapter for BitVm3Adapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        Ok(0)
    }

    async fn get_chain_identity(&self) -> String {
        format!("bitvm3:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        warn!(
            chain = "bitvm3",
            "BitVM3 transaction preparation is research-only; no garbled-circuit or recursive-proof backend is available"
        );
        Ok(json!({
            "chain": "bitvm3",
            "status": "research_only",
            "payload": tx_details,
            "type": "commitment",
            "experimental": true,
            "production_supported": false,
            "cryptographic_verification": false,
            "issue": "https://github.com/Conxian/conxian-gateway/issues/189"
        }))
    }

    async fn verify_state_proof(&self, _proof_metadata: Value) -> ConxianResult<bool> {
        warn!(
            chain = "bitvm3",
            "BitVM3 state-proof verification is unavailable: no stable garbled-circuit SDK, recursive-proof backend, or production deployment exists. See https://github.com/Conxian/conxian-gateway/issues/189"
        );
        Err(ConxianError::VerifierUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_identity_includes_network() {
        let adapter = BitVm3Adapter::new("regtest".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let identity = rt.block_on(adapter.get_chain_identity());
        assert_eq!(identity, "bitvm3:regtest");
    }

    #[test]
    fn latest_height_returns_zero() {
        let adapter = BitVm3Adapter::new("mainnet".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let height = rt.block_on(adapter.get_latest_height()).unwrap();
        assert_eq!(height, 0);
    }

    #[test]
    fn verify_state_proof_fails_closed() {
        let adapter = BitVm3Adapter::new("mainnet".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(adapter.verify_state_proof(json!({})));
        assert!(result.is_err());
        match result.unwrap_err() {
            ConxianError::VerifierUnavailable => {}
            other => panic!("expected VerifierUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn prepare_transaction_returns_research_only_marker() {
        let adapter = BitVm3Adapter::new("regtest".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tx = rt
            .block_on(adapter.prepare_unsigned_transaction(json!({"dummy": true})))
            .unwrap();
        assert_eq!(tx["chain"], "bitvm3");
        assert_eq!(tx["status"], "research_only");
        assert_eq!(tx["experimental"], true);
        assert_eq!(tx["production_supported"], false);
        assert_eq!(tx["cryptographic_verification"], false);
        assert!(tx["issue"].as_str().unwrap().contains("189"));
    }
}
