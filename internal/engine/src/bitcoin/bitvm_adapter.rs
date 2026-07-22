use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianError, ConxianResult};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{str::FromStr, sync::Arc};
use tracing::{info, warn};

use super::groth16_verifier::{
    BitcoinBlockContext, BitcoinNetwork, FieldElement, Groth16Curve, Groth16Proof,
    Groth16Statement, Groth16VerificationRequest, Groth16Verifier, InvalidProofReason, PublicInput,
    VerificationError, VerificationKeyId, VerificationResult, GROTH16_COMPRESSED_PROOF_BYTES,
    GROTH16_SCHEMA_VERSION, MAX_FIELD_ELEMENTS,
};

/// Protocol adapter for BitVM.
pub struct BitVmAdapter {
    /// Configured Bitcoin network name, such as `mainnet` or `regtest`.
    pub network: String,
    verifier: Option<Arc<dyn Groth16Verifier>>,
}

impl BitVmAdapter {
    /// Construct a metadata-compatible adapter without a cryptographic backend.
    pub fn new(network: String) -> Self {
        Self {
            network,
            verifier: None,
        }
    }

    /// Construct an adapter with an injected verifier backend.
    pub fn with_verifier<V>(network: String, verifier: Arc<V>) -> Self
    where
        V: Groth16Verifier + 'static,
    {
        let verifier: Arc<dyn Groth16Verifier> = verifier;
        Self {
            network,
            verifier: Some(verifier),
        }
    }

    /// Parse, validate, and delegate a BitVM Groth16 envelope to the injected
    /// backend. The envelope contains no raw witness material.
    pub async fn verify_groth16_envelope(
        &self,
        envelope: Value,
        current_block_height: u64,
    ) -> ConxianResult<VerificationResult> {
        let verifier = self.verifier.as_deref().ok_or_else(|| {
            ConxianError::Security(VerificationError::VerifierUnavailable.to_string())
        })?;
        self.verify_groth16_envelope_with(verifier, envelope, current_block_height)
            .await
    }

    /// Borrowed-verifier form used by callers that own the backend lifecycle.
    pub async fn verify_groth16_envelope_with<V>(
        &self,
        verifier: &V,
        envelope: Value,
        current_block_height: u64,
    ) -> ConxianResult<VerificationResult>
    where
        V: Groth16Verifier + ?Sized,
    {
        let request = parse_bitvm_groth16_envelope(envelope, current_block_height)
            .map_err(|error| ConxianError::Security(error.to_string()))?;
        request
            .validate()
            .map_err(|error| ConxianError::Security(error.to_string()))?;
        let adapter_network = BitcoinNetwork::from_str(&self.network)
            .map_err(|error| ConxianError::Security(error.to_string()))?;
        if request.statement.block_context.network != adapter_network {
            return Err(ConxianError::Security(
                VerificationError::InvalidBlockContext(format!(
                    "BitVM envelope network {:?} does not match adapter network {:?}",
                    request.statement.block_context.network, adapter_network
                ))
                .to_string(),
            ));
        }
        verifier
            .validate_verification_key_association(&request.statement)
            .await
            .map_err(|error| ConxianError::Security(error.to_string()))?;

        info!(
            chain = "bitvm",
            block_height = request.statement.block_context.block_height,
            statement_hash = %hex::encode(request.statement_hash),
            "delegating validated Groth16 statement to injected verifier"
        );

        let result = verifier
            .verify(&request)
            .await
            .map_err(|error| ConxianError::Security(error.to_string()))?;
        if !result.valid {
            return Err(ConxianError::Security(
                VerificationError::InvalidProof(InvalidProofReason::BackendRejected).to_string(),
            ));
        }

        Ok(result)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BitVmGroth16Envelope {
    schema_version: u16,
    curve: String,
    circuit_id: String,
    verification_key_id: String,
    public_inputs: Vec<String>,
    witness_commitment: String,
    block_context: BitVmBlockContext,
    proof: String,
    statement_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BitVmBlockContext {
    network: String,
    block_height: u64,
    block_hash: String,
    max_valid_height: Option<u64>,
}

/// Parse a BitVM envelope into the canonical request and reject malformed
/// input before any backend call.
pub fn parse_bitvm_groth16_envelope(
    envelope: Value,
    current_block_height: u64,
) -> Result<Groth16VerificationRequest, VerificationError> {
    if envelope.get("witness").is_some() || envelope.get("raw_witness").is_some() {
        return Err(VerificationError::RawWitnessProvided);
    }

    let raw: BitVmGroth16Envelope = serde_json::from_value(envelope)
        .map_err(|error| VerificationError::MalformedEnvelope(error.to_string()))?;
    if raw.schema_version != GROTH16_SCHEMA_VERSION {
        return Err(VerificationError::UnsupportedSchemaVersion(
            raw.schema_version,
        ));
    }

    let curve = Groth16Curve::from_str(&raw.curve)?;
    let network = BitcoinNetwork::from_str(&raw.block_context.network)?;
    let verification_key_id = VerificationKeyId(decode_fixed_hex::<32>(
        &raw.verification_key_id,
        "verification_key_id",
    )?);
    let witness_commitment = decode_fixed_hex::<32>(&raw.witness_commitment, "witness_commitment")?;
    let block_hash = decode_fixed_hex::<32>(&raw.block_context.block_hash, "block_hash")?;
    let statement_hash = decode_fixed_hex::<32>(&raw.statement_hash, "statement_hash")?;
    if raw.public_inputs.is_empty() || raw.public_inputs.len() > MAX_FIELD_ELEMENTS {
        return Err(VerificationError::InvalidPublicInputs(format!(
            "public_inputs must contain between 1 and {MAX_FIELD_ELEMENTS} elements"
        )));
    }
    let proof_bytes = decode_fixed_hex::<GROTH16_COMPRESSED_PROOF_BYTES>(&raw.proof, "proof")?;
    let proof = Groth16Proof::from_bytes(proof_bytes.to_vec())?;

    let mut public_inputs = Vec::with_capacity(raw.public_inputs.len());
    for (index, input) in raw.public_inputs.iter().enumerate() {
        let bytes = decode_fixed_hex::<32>(input, &format!("public_inputs[{index}]"))?;
        let value = FieldElement::from_bytes(bytes).map_err(|error| match error {
            VerificationError::InvalidFieldElement { reason, .. } => {
                VerificationError::InvalidFieldElement {
                    index: Some(index),
                    reason,
                }
            }
            other => other,
        })?;
        public_inputs.push(value);
    }

    let statement = Groth16Statement {
        schema_version: raw.schema_version,
        curve,
        circuit_id: raw.circuit_id,
        verification_key_id,
        public_inputs: PublicInput::new(public_inputs)?,
        witness_commitment,
        block_context: BitcoinBlockContext {
            network,
            block_height: raw.block_context.block_height,
            block_hash,
            max_valid_height: raw.block_context.max_valid_height,
        },
    };
    let request = Groth16VerificationRequest {
        statement,
        proof,
        statement_hash,
        current_block_height,
    };
    request.validate()?;
    Ok(request)
}

fn decode_fixed_hex<const N: usize>(
    value: &str,
    field: &str,
) -> Result<[u8; N], VerificationError> {
    if value.len() != N * 2 {
        return Err(VerificationError::MalformedEnvelope(format!(
            "{field} must contain exactly {} hexadecimal characters",
            N * 2
        )));
    }
    let bytes = hex::decode(value)
        .map_err(|error| VerificationError::MalformedEnvelope(format!("{field}: {error}")))?;
    bytes.try_into().map_err(|_| {
        VerificationError::MalformedEnvelope(format!("{field} has the wrong decoded length"))
    })
}

#[async_trait]
impl ChainAdapter for BitVmAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        Ok(0)
    }

    async fn get_chain_identity(&self) -> String {
        format!("bitvm:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(chain = "bitvm", "Preparing BitVM commitment transaction");
        Ok(json!({
            "chain": "bitvm",
            "status": "prepared",
            "payload": tx_details,
            "type": "commitment"
        }))
    }

    async fn verify_state_proof(&self, _proof_metadata: Value) -> ConxianResult<bool> {
        warn!(
            chain = "bitvm",
            "generic BitVM state-proof verification is unavailable; use the canonical Groth16 envelope handoff with a reviewed backend"
        );
        // This legacy ChainAdapter method cannot provide cryptographic
        // verification. The concrete Groth16 handoff is
        // verify_groth16_envelope(_with), above.
        Err(ConxianError::VerifierUnavailable)
    }
}
