use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Current version of the Conxian Gateway core library.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    pub hash: String,
    pub height: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub txid: String,
    pub confirmations: u32,
    pub block_hash: Option<String>,
    pub block_height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    pub height: u64,
    pub status: String,
    pub last_updated: u64,
    pub best_block_hash: String,
    pub network: String,
    /// Research enhancement: Nakamoto-specific fields
    pub epoch: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GatewayState {
    pub bitcoin: ChainState,
    pub stacks: ChainState,
}

impl Default for ChainState {
    fn default() -> Self {
        Self {
            height: 0,
            status: "initializing".to_string(),
            last_updated: 0,
            best_block_hash: "".to_string(),
            network: "unknown".to_string(),
            epoch: None,
            mode: None,
        }
    }
}

pub type SharedState = Arc<RwLock<GatewayState>>;

/// Represents a cryptographic attestation from a Conxius Wallet Secure Enclave.
/// Moved to core as it is a foundational type for the Compliance Pipe.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Attestation {
    pub device_id: String,
    pub signature: String, // Hex encoded
    pub payload: String,
    pub public_key: String, // Hex encoded
}

/// Research enhancement: Schnorr signature support for Taproot-compatible attestations.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchnorrAttestation {
    pub device_id: String,
    pub signature: String, // 64-byte Schnorr signature in hex
    pub payload: String,
    pub x_only_public_key: String, // 32-byte X-only public key in hex
}

#[derive(Error, Debug)]
pub enum ConxianError {
    #[error("Bitcoin error: {0}")]
    Bitcoin(String),
    #[error("Stacks error: {0}")]
    Stacks(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Compliance error: {0}")]
    Compliance(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Security/Verification error: {0}")]
    Security(String),
}

pub type ConxianResult<T> = Result<T, ConxianError>;
