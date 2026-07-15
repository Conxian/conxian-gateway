//! Groth16 Zero-Knowledge Proof Verifier Boundary
//!
//! Defines the internal trait boundary for Groth16 proof verification
//! used by BitVM and related ZK-based Bitcoin protocols.
//!
//! # Overview
//! Groth16 is a zk-SNARK construction (Groth, 2016) used in BitVM2,
//! GOAT-Network, and other Bitcoin L2 scaling solutions. This module
//! provides a typed boundary between Conxian adapters and verification backends.
//!
//! # Public Interface
//! - `Groth16Verifier`: Core verification trait
//! - `VerificationResult`: Structured proof verification output
//! - `PublicInput`: Normalized public inputs for Bitcoin-based proofs

use async_trait::async_trait;
use bitcoin::hashes::{sha256d, Hash};
use serde::{Deserialize, Serialize};

/// Verification key identifier (VK hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VerificationKeyId(pub [u8; 32]);

/// Public input for Groth16 proofs (normalized format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicInput {
    /// Circuit-specific public values
    pub values: Vec<bn::Fr>,
    /// Merkle root of public parameters (if applicable)
    pub merkle_root: Option<[u8; 32]>,
    /// Bitcoin block height at verification time
    pub block_height: u32,
}

/// Proof encoding (compressed Groth16 proof)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Groth16Proof {
    /// G1 element A
    pub a: ProofPoint,
    /// G2 element B
    pub b: ProofPoint,
    /// G1 element C
    pub c: ProofPoint,
}

/// Compressed elliptic curve point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofPoint {
    pub x: [u8; 32],
    pub y: [u8; 32],
}

/// Groth16 verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the proof is valid
    pub valid: bool,
    /// Verification key used
    pub vk_id: VerificationKeyId,
    /// Public inputs verified
    pub public_inputs: PublicInput,
    /// Verification transcript hash
    pub transcript: [u8; 32],
    /// Block height at verification
    pub verified_at_height: u32,
}

/// Groth16 verification error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationError {
    InvalidProof(String),
    InvalidPublicInputs(String),
    VerificationKeyNotFound(VerificationKeyId),
    ProofExpired {
        current_height: u32,
        proof_max_height: u32,
    },
    CircuitMismatch {
        expected: String,
        found: String,
    },
}

/// Core Groth16 verification trait
#[async_trait]
pub trait Groth16Verifier: Send + Sync {
    /// Verify a Groth16 proof against known verification keys
    async fn verify(
        &self,
        proof: &Groth16Proof,
        public_inputs: &PublicInput,
        vk_id: &VerificationKeyId,
    ) -> Result<VerificationResult, VerificationError>;

    /// Register a new verification key (circuit)
    async fn register_verification_key(
        &self,
        vk_id: VerificationKeyId,
        vk_bytes: Vec<u8>,
    ) -> Result<(), VerificationError>;

    /// Check if a verification key is known
    async fn is_verification_key_known(&self, vk_id: &VerificationKeyId) -> bool;
}

/// Mock verifier for testing (no actual proof verification)
pub struct MockGroth16Verifier;

#[async_trait]
impl Groth16Verifier for MockGroth16Verifier {
    async fn verify(
        &self,
        _proof: &Groth16Proof,
        public_inputs: &PublicInput,
        vk_id: &VerificationKeyId,
    ) -> Result<VerificationResult, VerificationError> {
        Ok(VerificationResult {
            valid: true,
            vk_id: vk_id.clone(),
            public_inputs: public_inputs.clone(),
            transcript: sha256d::Hash::hash(&[0u8]).to_byte_array(),
            verified_at_height: public_inputs.block_height,
        })
    }

    async fn register_verification_key(
        &self,
        _vk_id: VerificationKeyId,
        _vk_bytes: Vec<u8>,
    ) -> Result<(), VerificationError> {
        Ok(())
    }

    async fn is_verification_key_known(&self, _vk_id: &VerificationKeyId) -> bool {
        true
    }
}

/// BitVM adapter integration helper
pub trait BitVmGroth16Adapter {
    /// Convert BitVM proof format to normalized Groth16Proof
    fn from_bitvm_proof(bitvm_proof_bytes: &[u8]) -> Result<Groth16Proof, VerificationError>;

    /// Extract public inputs from BitVM execution transcript
    fn extract_public_inputs(transcript: &[u8]) -> Result<PublicInput, VerificationError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_result_serialization() {
        let result = VerificationResult {
            valid: true,
            vk_id: VerificationKeyId([0u8; 32]),
            public_inputs: PublicInput {
                values: vec![],
                merkle_root: None,
                block_height: 850000,
            },
            transcript: [1u8; 32],
            verified_at_height: 850000,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("\"valid\":true"));
        assert!(json.contains("850000"));
    }

    #[test]
    fn test_mock_verifier_accepts_any_proof() {
        let verifier = MockGroth16Verifier;
        let proof = Groth16Proof {
            a: ProofPoint {
                x: [0u8; 32],
                y: [1u8; 32],
            },
            b: ProofPoint {
                x: [2u8; 32],
                y: [3u8; 32],
            },
            c: ProofPoint {
                x: [4u8; 32],
                y: [5u8; 32],
            },
        };
        let inputs = PublicInput {
            values: vec![],
            merkle_root: None,
            block_height: 850000,
        };
        let vk_id = VerificationKeyId([0u8; 32]);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(verifier.verify(&proof, &inputs, &vk_id));
        assert!(result.is_ok());
        assert!(result.unwrap().valid);
    }
}
