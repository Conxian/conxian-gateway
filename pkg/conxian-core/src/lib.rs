pub mod lightning;
pub mod musig2;
pub mod persistence;
pub mod settlement;
pub mod trust_policy;
use async_trait::async_trait;
pub use lightning::*;
use serde::{Deserialize, Serialize};
pub use settlement::*;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::info;
pub use trust_policy::*;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeeBumpStrategy {
    Rbf,
    Cpfp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MempoolTxStatus {
    #[default]
    Pending,
    Stuck,
    BumpBroadcasted,
    GuardrailRejected,
    Confirmed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackedMempoolTx {
    pub txid: String,
    pub first_seen_at: u64,
    pub last_evaluated_at: Option<u64>,
    pub last_bump_at: Option<u64>,
    pub bump_attempts: u32,
    pub current_fee_rate_sat_vb: u64,
    pub target_fee_rate_sat_vb: Option<u64>,
    pub replaceable: bool,
    pub cpfp_eligible: bool,
    #[serde(default)]
    pub status: MempoolTxStatus,
    pub last_bump_strategy: Option<FeeBumpStrategy>,
    pub last_error: Option<String>,
    pub replacement_txid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    pub trust_policy_allow: u64,
    pub trust_policy_block: u64,
    /// Research enhancement: Treasury metrics
    pub treasury_balance_stx: f64,
    pub treasury_balance_btc: u64,
    pub last_treasury_update: u64,
    /// Industry Enhancement: TAM Capture Metrics
    pub sbtc_liquidity: f64,
    pub syi_index: f64,
    /// G-B2: Multi-currency FX rates (relative to USD)
    pub fx_rmb_usd: f64,
    pub fx_rub_usd: f64,
    pub fx_inr_usd: f64,
    pub fx_aed_usd: f64,
    /// CON-230: Bounty Payout Activation
    pub bounty_payouts_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayState {
    pub bitcoin: ChainState,
    pub stacks: ChainState,
    pub metrics: Metrics,
    pub wallets: SystemWallets,
    pub handoff_state: HandoffState,
    pub start_time: u64,
}

impl GatewayState {
    pub fn new() -> Self {
        Self {
            bitcoin: ChainState::default(),
            stacks: ChainState::default(),
            metrics: Metrics::default(),
            wallets: SystemWallets::default(),
            handoff_state: HandoffState::default(),
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

pub type ConxianResult<T> = Result<T, ConxianError>;

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
    #[error("Persistence error: {0}")]
    Persistence(String),
    #[error("Machine identity error: {0}")]
    MachineIdentity(String),
}

/// Shared global state wrapped for thread-safe access.
pub type SharedState = Arc<RwLock<GatewayState>>;

/// Represents a cryptographic attestation from a Conxius Wallet Secure Enclave.
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

/// ZKML proof mapping to Guardian Attestations for off-chain models.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ZkmlProof {
    pub device_id: String,
    #[serde(default)]
    pub image_id: String,
    #[serde(default)]
    pub receipt: String,
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
    #[serde(default)]
    pub proof_hash: String,
    #[serde(default)]
    pub verifier_address: String,
}

/// Unified request for attestation verification.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum AttestationRequest {
    Ecdsa(Attestation),
    Schnorr(SchnorrAttestation),
    Zkml(ZkmlProof),
    BitVm(BitVmAttestation),
    Cbtc(CbtcAttestation),
}

// ── CBTC Non-Custodial Verification (G-C1) ───────────────────────────

/// G-C1: CBTC (wrapped Bitcoin on Canton Network via BitSafe) attestation.
/// Verifies that CBTC tokens on Canton are provably backed by Bitcoin reserves
/// without joining the FROST signer set or taking custody.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CbtcAttestation {
    /// Canton domain where CBTC contract lives
    pub canton_domain: String,
    /// Daml contract ID for the CBTC token
    pub contract_id: String,
    /// Amount of CBTC (in satoshis) claimed to be backed
    pub amount_sats: u64,
    /// Bitcoin UTXO(s) backing this CBTC — txid:vout format
    pub bitcoin_utxos: Vec<String>,
    /// FROST threshold signature attesting to Bitcoin reserve
    #[serde(default)]
    pub frost_attestation: Option<String>,
    /// Block height at which the attestation was produced
    pub attested_at_height: u64,
    /// Signer quorum metadata (threshold, total signers)
    #[serde(default)]
    pub quorum: Option<CbtcQuorum>,
}

/// G-C1: FROST signer quorum metadata for CBTC attestation verification.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CbtcQuorum {
    /// Number of signers that produced the attestation
    pub signers_present: u16,
    /// Total signers in the FROST set
    pub signers_total: u16,
    /// Aggregated public key of the FROST signer set
    pub aggregate_key: String,
}

/// G-C1: CBTC verification request sent to the UCV-1 universal verifier.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CbtcVerificationRequest {
    /// The CBTC attestation to verify
    pub attestation: CbtcAttestation,
    /// If true, also verify Bitcoin UTXO inclusion proofs (requires Bitcoin RPC)
    #[serde(default)]
    pub verify_utxo_proofs: bool,
}

/// G-C1: CBTC verification response from UCV-1.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CbtcVerificationResponse {
    /// Whether the CBTC attestation passed all verification checks
    pub verified: bool,
    /// Contract ID that was verified
    pub contract_id: String,
    /// Amount verified (sats)
    pub amount_sats: u64,
    /// Number of UTXOs confirmed as backing
    pub utxos_verified: u32,
    /// Quorum ratio (signers_present / signers_total) — None if no quorum info
    pub quorum_ratio: Option<f64>,
    /// Verification timestamp (Unix)
    pub verified_at: u64,
    /// Detailed verification results per check
    #[serde(default)]
    pub checks: Vec<CbtcVerificationCheck>,
}

/// G-C1: Individual verification check result.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CbtcVerificationCheck {
    /// Check name
    pub check: String,
    /// Whether this check passed
    pub passed: bool,
    /// Human-readable detail
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JobCardSettlementRequest {
    pub job_card: ConxianJobCard,
    pub bitvm_attestation: BitVmAttestation,
}

/// Discrete Log Contract (DLC) bond definition (CON-72).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DlcBond {
    pub bond_id: String,
    pub amount_btc: u64,
    pub interest_rate: f64,
    pub maturity_date: u64,
    pub sovereign_alignment: bool,
}

/// Industry Enhancement: Discrete Log Contract (DLC) Orchestrator (CON-62).
pub trait DlcOrchestrator: Send + Sync {
    fn create_dlc_bond(&self, bond: &DlcBond) -> ConxianResult<String>;
    fn settle_coupon(&self, bond_id: &str, amount_sbtc: u64) -> ConxianResult<bool>;
}

/// Concrete implementation of Discrete Log Contract (DLC) Orchestrator for Bitcoin bonds (CON-72).
pub struct DlcManager {
    pub oracle_pubkey: String,
}

impl DlcManager {
    pub fn new(oracle_pubkey: String) -> Self {
        Self { oracle_pubkey }
    }
}

impl DlcOrchestrator for DlcManager {
    fn create_dlc_bond(&self, bond: &DlcBond) -> ConxianResult<String> {
        info!("Creating DLC-backed Bitcoin bond: {} sBTC", bond.amount_btc);
        let bond_id = format!("dlc-bond-{}", uuid::Uuid::new_v4());
        Ok(bond_id)
    }

    fn settle_coupon(&self, bond_id: &str, amount_sbtc: u64) -> ConxianResult<bool> {
        info!(
            "Settling coupon for DLC bond {}: {} satoshis",
            bond_id, amount_sbtc
        );
        Ok(!bond_id.is_empty())
    }
}

/// CON-66: Identity resolution request for ENS, BNS, and World ID.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IdentityResolutionRequest {
    pub identifier: String,
    pub provider: String, // "ens", "bns", "worldid", or "web3bio",
    pub signature: Option<String>,
}

/// CON-66: Identity resolution response.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IdentityResolutionResponse {
    pub address: String,
    pub provider: String,
    pub verified: bool,
    pub metadata: Option<serde_json::Value>,
}

// ── Machine Identity (G-C2) ──────────────────────────────────────────

/// Classification of autonomous machine types in the Machine Economy (DePIN).
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MachineType {
    ElectricVehicle,
    EvCharger,
    Drone,
    Robot,
    Sensor,
    ComputeNode,
    StorageNode,
    Camera,
    DeliveryVehicle,
    EnergyMeter,
    Other,
}

/// Sovereign machine identity for DePIN / Machine Economy participants.
/// Machines hold their own keys; Conxian routes and verifies, never custodies.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MachineIdentity {
    /// peaq decentralized identifier (did:peaq:<address>)
    pub peaq_did: Option<String>,
    /// DIMO vehicle identity (for connected vehicles)
    pub dimo_vehicle_id: Option<String>,
    /// Schnorr x-only public key (Taproot-ready, BIP-340)
    pub device_key: String,
    /// Manufacturer or DePIN-network attestation proof
    #[serde(default)]
    pub attestation_proof: Option<String>,
    /// Classification of the machine
    pub machine_type: MachineType,
    /// Human-readable machine label (e.g., "Tesla Model 3 — Vienna Fleet #42")
    #[serde(default)]
    pub label: Option<String>,
}

/// G-C2: Machine identity resolution request — extends existing CON-66 identity stack
/// to support autonomous machine participants.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MachineIdentityResolutionRequest {
    /// Machine DID or device public key to resolve
    pub identifier: String,
    /// Provider: "peaq", "dimo", "device_key"
    pub provider: String,
    /// BIP-322 or Schnorr proof-of-possession signature (optional)
    pub signature: Option<String>,
    /// If set, the resolution targets a specific machine identity variant
    #[serde(default)]
    pub machine_type_hint: Option<MachineType>,
}

/// G-C2: Machine identity resolution response.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MachineIdentityResolutionResponse {
    /// Resolved machine identity with all known attributes
    pub identity: MachineIdentity,
    /// Provider that served the resolution
    pub provider: String,
    /// Whether the device key attestation was verified
    pub verified: bool,
    /// Arbitrary provider-supplied metadata
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Industry Enhancement: Workload Identity Federation (WIF) token request.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GcpTokenRequest {
    pub audience: String,
    #[serde(default)]
    pub grant_type: String,
    #[serde(default)]
    pub requested_token_type: String,
    #[serde(default)]
    pub scope: String,
    pub subject_token: String,
    #[serde(default)]
    pub subject_token_type: String,
}

/// Persistent data that needs to be saved across restarts.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PersistentState {
    pub bitcoin_height: u64,
    pub stacks_height: u64,
    #[serde(default)]
    pub mempool_pending_txs: Vec<TrackedMempoolTx>,
}

/// Trait for persistence of gateway state.
pub trait Persistence: Send + Sync {
    fn save(&self, state: &PersistentState) -> ConxianResult<()>;
    fn load(&self) -> ConxianResult<PersistentState>;
}

/// CON-423: SAB-owned system wallets for BOS operations.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemWallets {
    pub bootstrap: String,
    pub treasury: String,
    pub payout: String,
    pub deployment: String,
    pub emergency: String,
    pub dao_handoff: String,
    pub protocol_owned: String,
    pub reserve_fund: String,
    pub labs_ops: String,
    pub contributor_claims: String,
    pub founder_vault: String,
}

impl Default for SystemWallets {
    fn default() -> Self {
        Self {
            bootstrap: "SPSZXAKV7DWTDZN2601WR31BM51BD3YTQWE97VRM".to_string(),
            treasury: "SP12JZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string(),
            payout: "SP2JZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string(),
            deployment: "SP3JZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string(),
            emergency: "SP000000000000000000002Q6VF78".to_string(),
            dao_handoff: "SP1P74G56Z5SNC6B2H70MBN8D6X1XW19C52R0P95".to_string(),
            protocol_owned: "SP2JZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string(),
            reserve_fund: "SP3JZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string(),
            labs_ops: "SP2KZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string(),
            contributor_claims: "SP3KZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string(),
            founder_vault: "SP4KZZSBY0S3FJH7WJT2787YTYT8Y6725F7T8E62".to_string(),
        }
    }
}

/// Request for an ALEX swap operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlexSwapRequest {
    pub token_x: String,
    pub token_y: String,
    pub factor: u64,
    pub amount: u128,
    pub min_dy: Option<u128>,
}

/// CON-78: Offline-First POS Receipt.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OfflineReceipt {
    pub receipt_id: String,
    pub tx_hash: String,
    pub amount_sbtc: u64,
    pub timestamp: u64,
    pub device_id: String,
    pub tee_signature: String, // TEE-signed commitment
    pub passkey_attestation: AttestationRequest,
    pub status: OfflineReceiptStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OfflineReceiptStatus {
    Pending,
    Gossiped,
    Broadcasted,
    Reconciled,
}

/// CON-78: Local encrypted queue for offline transactions.
pub trait OfflineQueue: Send + Sync {
    fn enqueue(&self, receipt: &OfflineReceipt) -> ConxianResult<()>;
    fn dequeue_pending(&self) -> ConxianResult<Vec<OfflineReceipt>>;
    fn mark_broadcasted(&self, receipt_id: &str) -> ConxianResult<()>;
    /// Claims a webhook replay key for a bounded TTL window.
    ///
    /// Returns `true` when this caller successfully claimed the key (first
    /// delivery in-window), and `false` when an unexpired claim already exists.
    fn claim_replay_key(&self, replay_key: &str, ttl_seconds: u64) -> ConxianResult<bool>;
}

/// CON-73: [ATS-v12.0] Conxian Job Card Schema (CJCS) v2.0 JSON-LD
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConxianJobCard {
    #[serde(rename = "@context", default)]
    pub context: String,
    #[serde(rename = "@type")]
    pub r#type: String,
    #[serde(default)]
    pub work_intent: WorkIntent,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WorkIntent {
    pub sender_address: String,
    pub receiver_address: String,
    pub amount_sbtc: u64,
    pub town_name: Option<String>,
    pub country_code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum RolloutMode {
    Disabled,
    Shadow,
    Active,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RgbAdapterConfig {
    pub mode: RolloutMode,
    pub node_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContractState {
    pub contract_id: String,
    pub schema_id: String,
    pub state_data: serde_json::Value,
}

#[async_trait]
pub trait RgbAdapter: Send + Sync {
    async fn lookup_contract(&self, contract_id: &str) -> ConxianResult<Option<ContractState>>;
    async fn verify_transition(&self, transition_id: &str) -> ConxianResult<bool>;
}
pub trait PersistentStateTrait: Send + Sync {
    fn save(&self, state: &GatewayState) -> ConxianResult<()>;
    fn load(&self) -> ConxianResult<GatewayState>;
}

/// CON-482: Handoff sequence state
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum HandoffState {
    #[default]
    BootstrapActive,
    SabAuditInProgress,
    DaoProposalActive,
    HandoffComplete,
}

impl SystemWallets {
    pub fn get_payout_destination(&self, state: HandoffState) -> &str {
        match state {
            HandoffState::BootstrapActive => &self.bootstrap,
            _ => &self.payout,
        }
    }

    pub fn get_treasury_destination(&self, state: HandoffState) -> &str {
        match state {
            HandoffState::BootstrapActive => &self.bootstrap,
            _ => &self.treasury,
        }
    }
}

#[async_trait]
#[async_trait]
#[async_trait]
pub trait ChainAdapter: Send + Sync {
    async fn get_latest_height(&self) -> ConxianResult<u64>;
    async fn get_chain_identity(&self) -> String;
    async fn prepare_unsigned_transaction(
        &self,
        tx_details: serde_json::Value,
    ) -> ConxianResult<serde_json::Value>;
    async fn verify_state_proof(&self, proof_metadata: serde_json::Value) -> ConxianResult<bool>;
}

#[async_trait]
#[allow(async_fn_in_trait)]
pub trait Bip322Verifier: Send + Sync {
    fn verify_message(&self, address: &str, message: &str, signature: &str) -> ConxianResult<bool>;
}

#[async_trait]
#[allow(async_fn_in_trait)]
pub trait SimulatedStacksRpcTrait: Send + Sync {
    async fn call_read_only(
        &self,
        contract: &str,
        function: &str,
        args: Vec<serde_json::Value>,
    ) -> ConxianResult<serde_json::Value>;
}

/// CON-775: Release approval request.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReleaseApprovalRequest {
    pub release_id: String,
    pub artifact_hash: String,
    pub environment: String,
    pub requester: String,
}

/// CON-775: Release decision submission.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReleaseDecisionRequest {
    pub release_id: String,
    pub decision: String, // "approved" or "rejected"
    pub approver: String,
    pub reason: Option<String>,
}

/// CON-775: Governance decision submission.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GovernanceDecisionRequest {
    pub proposal_id: String,
    pub decision: String, // "approved", "rejected", or "abstain"
    pub voter: String,
    pub signature: String, // TEE or EOC signature
}

/// CON-775: Unified admin response.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminActionResponse {
    pub action_id: String,
    pub status: String,
    pub audit_event_id: String,
    pub message: String,
}

/// CON-771: Shared domain schema for Governance Actions.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GovernanceAction {
    pub action_id: String,
    pub proposal_id: String,
    pub action_type: String, // e.g., "parameter_change", "treasury_allocation"
    pub payload: serde_json::Value,
    pub status: String,
    pub enacted_at: Option<u64>,
}

/// CON-771: Shared domain schema for Treasury Events.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TreasuryEvent {
    pub event_id: String,
    pub asset: String,
    pub amount: u128,
    pub direction: String, // "inflow" or "outflow"
    pub reason: String,
    pub timestamp: u64,
    pub reference_id: Option<String>,
}

/// CON-771: Shared domain schema for Audit Events.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditEvent {
    pub event_id: String,
    pub domain: String, // "release", "governance", "treasury", "identity"
    pub actor: String,
    pub action: String,
    pub outcome: String,
    pub timestamp: u64,
    pub metadata: serde_json::Value,
}
