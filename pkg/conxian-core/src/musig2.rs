use crate::ConxianResult;
use serde::{Deserialize, Serialize};

/// CON-1270: MuSig2 (BIP-327) Aggregated Public Key
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MuSig2AggregatedKey {
    pub aggregated_pubkey: String,
    pub participant_pubkeys: Vec<String>,
}

/// MuSig2 Partial Signature for aggregation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MuSig2PartialSignature {
    pub participant_pubkey: String,
    pub partial_signature: String,
    pub nonce: String,
}

/// Trait for MuSig2 (BIP-327) operations
pub trait MuSig2Orchestrator: Send + Sync {
    fn aggregate_pubkeys(&self, pubkeys: &[String]) -> ConxianResult<MuSig2AggregatedKey>;
    fn aggregate_signatures(
        &self,
        aggregated_key: &MuSig2AggregatedKey,
        partial_sigs: &[MuSig2PartialSignature],
        message_hash: &[u8; 32],
    ) -> ConxianResult<String>;
}
