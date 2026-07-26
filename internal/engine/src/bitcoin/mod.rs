pub mod fee_bump_policy;
pub mod listener;
pub mod mempool_orchestrator;
pub mod rpc;
pub mod shadow_observation;

pub use fee_bump_policy::FeeBumpPolicyConfig;
pub use listener::BitcoinListener;
pub use mempool_orchestrator::MempoolOrchestrator;
pub use rpc::{BitcoinRpc, BitcoinRpcClient};
pub use shadow_observation::{
    BitcoinCoreShadowObservation, BitcoinCoreShadowObserver, BitcoinCoreShadowObserverClient,
    CoreBestBlockStats, CoreBlockchainInfo, CoreMempoolInfo, CoreNetworkInfo, DeploymentAlias,
    DeploymentObservation, DeploymentObservationStatus, DeploymentReportedState,
    DeploymentSourceScope, FeeEstimateAvailability, FeeEstimateObservation, FeeRateUnit,
    ObservationAvailability, ObservationErrorCategory, ShadowObserverFailure, SourceObservation,
    SHADOW_FEE_TARGETS,
};
pub mod rgb_adapter;
pub use rgb_adapter::NodeRgbAdapter;
pub mod liquid_adapter;
pub use liquid_adapter::LiquidAdapter;
pub mod babylon_adapter;
pub use babylon_adapter::{
    BabylonAdapter, BabylonHeaderInfoResponse, BabylonHeaderSource, BabylonHttpClient,
    BabylonMainChainResponse, BabylonPagination, BabylonTipResponse, BtcHeaderInfo,
};
pub mod bitvm_adapter;
pub use bitvm_adapter::BitVmAdapter;
pub mod fedimint_adapter;
pub use fedimint_adapter::FedimintAdapter;
pub mod strata_adapter;
pub use strata_adapter::StrataAdapter;
pub mod rgb_native;
pub mod rgb_stash;
pub use rgb_stash::StashResolver;
pub mod risc0_verifier;
pub use risc0_verifier::{Risc0Mode, Risc0StfVerifier, Risc0VerificationReceipt};
pub mod dlc_oracle;
pub use dlc_oracle::{
    DlcOracleClient, OracleAnnouncement, OracleAttestation, ThresholdOracleCoordinator,
};

pub mod groth16_verifier;
pub use bitvm_adapter::parse_bitvm_groth16_envelope;
pub use groth16_verifier::{
    compute_witness_commitment, witness_commitment_public_inputs, BitVmGroth16Adapter,
    BitcoinBlockContext, BitcoinNetwork, FieldElement, Groth16Curve, Groth16Proof,
    Groth16Statement, Groth16VerificationRequest, Groth16Verifier, InvalidProofReason, PublicInput,
    VerificationError, VerificationKeyId, VerificationResult, BN254_FIELD_ELEMENT_BYTES,
    BN254_SCALAR_MODULUS, GROTH16_COMPRESSED_PROOF_BYTES, GROTH16_SCHEMA_VERSION,
    MAX_CIRCUIT_ID_BYTES, MAX_FIELD_ELEMENTS, MAX_VERIFICATION_KEY_BYTES,
    WITNESS_COMMITMENT_PUBLIC_INPUT_LIMBS, WITNESS_COMMITMENT_PUBLIC_INPUT_LIMB_BYTES,
};

#[cfg(any(test, feature = "mock-integrations"))]
pub use groth16_verifier::MockGroth16Verifier;
