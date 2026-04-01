pub mod settlement;
pub mod persistence;
pub use settlement::*;
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
    pub last_sync_time: u64,
    pub best_block_hash: String,
    pub network: String,
    /// Research enhancement: Nakamoto-specific fields
    pub epoch: Option<String>,
    pub mode: Option<String>,
    pub burn_block_height: Option<u64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub health_requests: u64,
    pub state_requests: u64,
    pub metrics_requests: u64,
    pub verification_requests: u64,
    pub verification_success: u64,
    pub verification_failure: u64,
    pub total_requests: u64,
    /// Research enhancement: Treasury metrics
    pub treasury_balance_stx: f64,
    pub treasury_balance_btc: f64,
    pub last_treasury_update: u64,
    /// Industry Enhancement: TAM Capture Metrics
    pub sbtc_liquidity: f64,
    pub syi_index: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayState {
    pub bitcoin: ChainState,
    pub stacks: ChainState,
    pub metrics: Metrics,
    pub start_time: u64,
}

impl Default for GatewayState {
    fn default() -> Self {
        Self {
            bitcoin: ChainState::default(),
            stacks: ChainState::default(),
            metrics: Metrics::default(),
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

impl Default for ChainState {
    fn default() -> Self {
        Self {
            height: 0,
            status: "initializing".to_string(),
            last_updated: 0,
            last_sync_time: 0,
            best_block_hash: "".to_string(),
            network: "unknown".to_string(),
            epoch: None,
            mode: None,
            burn_block_height: None,
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

/// Unified request for attestation verification.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum AttestationRequest {
    Ecdsa(Attestation),
    Schnorr(SchnorrAttestation),
    Zkml(ZkmlProof),
    BitVm(BitVmAttestation),
}

/// ZKML proof mapping to Guardian Attestations for off-chain models.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ZkmlProof {
    pub device_id: String,
    pub receipt_hash: String,
    pub public_inputs: String,
    pub journal: String,
}

/// Industry Enhancement: BitVM Attestation for trustless state verification.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BitVmAttestation {
    pub prover_id: String,
    pub commitment_hash: String,
    pub state_root: String,
}

/// Industry Enhancement: Workload Identity Federation (WIF) token request.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GcpTokenRequest {
    pub audience: String,
    pub grant_type: String,
    pub requested_token_type: String,
    pub scope: String,
    pub subject_token: String,
    pub subject_token_type: String,
}

/// Industry Enhancement: Discrete Log Contract (DLC) Bond.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DlcBond {
    pub bond_id: String,
    pub amount_btc: f64,
    pub interest_rate: f64,
    pub maturity_date: u64,
    pub sovereign_alignment: bool,
}

/// Industry Enhancement: Sovereign Yield Index (SYI) tracking.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SovereignYield {
    pub sbtc_liquidity: f64,
    pub syi_index: f64,
    pub yield_multiplier: f64,
}

/// Persistent data that needs to be saved across restarts.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PersistentState {
    pub bitcoin_height: u64,
    pub stacks_height: u64,
}

/// Trait for persistence of gateway state.
pub trait Persistence: Send + Sync {
    fn save(&self, state: &PersistentState) -> ConxianResult<()>;
    fn load(&self) -> ConxianResult<PersistentState>;
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
    #[error("IO error: {0}")]
    Io(String),
}

pub type ConxianResult<T> = Result<T, ConxianError>;

/// CON-73: [ATS-v12.0] Conxian Job Card Schema (CJCS) v2.0 JSON-LD
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConxianJobCard {
    #[serde(rename = "@context")]
    pub context: String,
    #[serde(rename = "@type")]
    pub r#type: String,
    pub work_intent: WorkIntent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkIntent {
    pub sender_address: String,
    pub receiver_address: String,
    pub amount_sbtc: f64,
    pub town_name: Option<String>,
    pub country_code: Option<String>,
}

/// CON-66: Identity resolution request for ENS, BNS, and World ID.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IdentityResolutionRequest {
    pub identifier: String,
    pub provider: String, // "ens", "bns", "worldid", or "web3bio"
}

/// CON-66: Identity resolution response.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IdentityResolutionResponse {
    pub address: String,
    pub provider: String,
    pub verified: bool,
    pub metadata: Option<serde_json::Value>,
}

/// Industry Enhancement: Discrete Log Contract (DLC) Orchestrator (CON-62).
pub trait DlcOrchestrator: Send + Sync {
    fn create_dlc_bond(&self, bond: &DlcBond) -> ConxianResult<String>;
    fn settle_coupon(&self, bond_id: &str, amount_sbtc: f64) -> ConxianResult<bool>;
}
