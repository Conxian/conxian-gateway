//! Backend-neutral Groth16 verification contract.
//!
//! This module deliberately stops at the verifier boundary. It defines the
//! bytes that a backend must receive, validates them before delegation, and
//! provides a deterministic test-only verifier. It does not contain a prover,
//! an elliptic-curve implementation, or a claim that a proof has been
//! cryptographically verified.
//!
//! The canonical statement encoding is documented in
//! `docs/GROTH16_VERIFIER_CONTRACT.md`. In particular, it is not derived from
//! JSON serialization: field order, lengths, byte order, and schema tags are
//! explicit.

use async_trait::async_trait;
use bitcoin::hashes::{sha256, Hash};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
#[cfg(any(test, feature = "mock-integrations"))]
use std::collections::HashMap;
#[cfg(any(test, feature = "mock-integrations"))]
use std::sync::RwLock;
use std::{fmt, str::FromStr};
use thiserror::Error;

/// Current canonical statement schema version.
pub const GROTH16_SCHEMA_VERSION: u16 = 1;

/// BN254 scalar-field element width in the canonical wire format.
pub const BN254_FIELD_ELEMENT_BYTES: usize = 32;

/// Compressed BN254 Groth16 proof width: A (G1), B (G2), and C (G1).
pub const GROTH16_COMPRESSED_PROOF_BYTES: usize = 32 + 64 + 32;

/// Maximum accepted circuit identifier length in UTF-8 bytes.
pub const MAX_CIRCUIT_ID_BYTES: usize = 128;

/// Maximum number of public or private witness field elements.
pub const MAX_FIELD_ELEMENTS: usize = 256;

/// Maximum verification-key byte length accepted at registration.
pub const MAX_VERIFICATION_KEY_BYTES: usize = 1024 * 1024;

/// Number of reserved public-input slots used for the witness commitment.
pub const WITNESS_COMMITMENT_PUBLIC_INPUT_LIMBS: usize = 2;

/// Width of each witness-commitment limb before zero-extension to a field
/// element.
pub const WITNESS_COMMITMENT_PUBLIC_INPUT_LIMB_BYTES: usize = 16;

const FIELD_ENCODING_BN254_BIG_ENDIAN_32: u8 = 1;
const STATEMENT_ENCODING_DOMAIN: &[u8] = b"CONXIAN-GROTH16-STATEMENT-ENCODING-V1";
const STATEMENT_HASH_DOMAIN: &[u8] = b"CONXIAN-GROTH16-STATEMENT-HASH-V1";
const VERIFICATION_KEY_ID_DOMAIN: &[u8] = b"CONXIAN-GROTH16-VERIFICATION-KEY-ID-V1";
const WITNESS_COMMITMENT_DOMAIN: &[u8] = b"CONXIAN-GROTH16-WITNESS-COMMITMENT-V1";
const PROOF_DIGEST_DOMAIN: &[u8] = b"CONXIAN-GROTH16-PROOF-DIGEST-V1";
#[cfg(any(test, feature = "mock-integrations"))]
const TRANSCRIPT_DOMAIN: &[u8] = b"CONXIAN-GROTH16-TRANSCRIPT-V1";

/// BN254 scalar-field modulus, encoded as a 32-byte big-endian integer.
///
/// Field elements in this contract are canonical representatives in the
/// interval `[0, modulus)`. Redundant encodings are rejected before a backend
/// is called.
pub const BN254_SCALAR_MODULUS: [u8; BN254_FIELD_ELEMENT_BYTES] = [
    0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29, 0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
    0x28, 0x33, 0xe8, 0x48, 0x79, 0xb9, 0x70, 0x91, 0x4e, 0x3e, 0x1f, 0x59, 0x3f, 0x00, 0x00, 0x01,
];

/// A BN254 scalar-field element in canonical big-endian form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct FieldElement([u8; BN254_FIELD_ELEMENT_BYTES]);

impl FieldElement {
    /// Construct a field element and reject non-canonical values.
    pub fn from_bytes(bytes: [u8; BN254_FIELD_ELEMENT_BYTES]) -> Result<Self, VerificationError> {
        if bytes >= BN254_SCALAR_MODULUS {
            return Err(VerificationError::InvalidFieldElement {
                index: None,
                reason: "value is not below the BN254 scalar-field modulus".to_owned(),
            });
        }

        Ok(Self(bytes))
    }

    /// Return the fixed-width big-endian representation.
    pub fn as_bytes(&self) -> &[u8; BN254_FIELD_ELEMENT_BYTES] {
        &self.0
    }

    fn validate(&self, index: Option<usize>) -> Result<(), VerificationError> {
        if self.0 >= BN254_SCALAR_MODULUS {
            return Err(VerificationError::InvalidFieldElement {
                index,
                reason: "value is not below the BN254 scalar-field modulus".to_owned(),
            });
        }

        Ok(())
    }
}

impl<'de> Deserialize<'de> for FieldElement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <[u8; BN254_FIELD_ELEMENT_BYTES]>::deserialize(deserializer)?;
        Self::from_bytes(bytes).map_err(D::Error::custom)
    }
}

/// Identifier for the only curve/field encoding currently admitted by this
/// version of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Groth16Curve {
    /// BN254 with scalar-field elements encoded as 32-byte big-endian values.
    Bn254,
}

impl Groth16Curve {
    fn tag(self) -> u8 {
        match self {
            Self::Bn254 => 1,
        }
    }
}

impl FromStr for Groth16Curve {
    type Err = VerificationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "bn254" => Ok(Self::Bn254),
            other => Err(VerificationError::UnsupportedCurve(other.to_owned())),
        }
    }
}

/// Bitcoin network tag included in the statement hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BitcoinNetwork {
    /// Bitcoin mainnet.
    Mainnet,
    /// Bitcoin public testnet.
    Testnet,
    /// Bitcoin signet.
    Signet,
    /// Bitcoin regtest.
    Regtest,
}

impl BitcoinNetwork {
    fn tag(self) -> u8 {
        match self {
            Self::Mainnet => 1,
            Self::Testnet => 2,
            Self::Signet => 3,
            Self::Regtest => 4,
        }
    }
}

impl FromStr for BitcoinNetwork {
    type Err = VerificationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            "signet" => Ok(Self::Signet),
            "regtest" => Ok(Self::Regtest),
            other => Err(VerificationError::InvalidBlockContext(format!(
                "unsupported Bitcoin network `{other}`"
            ))),
        }
    }
}

/// Bitcoin anchor context bound into a Groth16 statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitcoinBlockContext {
    /// Bitcoin network on which the BitVM state was anchored.
    pub network: BitcoinNetwork,
    /// Exact anchor height, not a relative confirmation count.
    pub block_height: u64,
    /// 32 bytes in canonical Bitcoin display order (the order used by the
    /// envelope's hexadecimal representation).
    pub block_hash: [u8; 32],
    /// Optional last height at which the statement may be verified.
    pub max_valid_height: Option<u64>,
}

impl BitcoinBlockContext {
    fn validate(&self) -> Result<(), VerificationError> {
        if self.block_height == 0 {
            return Err(VerificationError::InvalidBlockContext(
                "block height must be greater than zero".to_owned(),
            ));
        }
        if self.block_hash == [0u8; 32] {
            return Err(VerificationError::InvalidBlockContext(
                "block hash must be non-zero".to_owned(),
            ));
        }
        if let Some(max_valid_height) = self.max_valid_height {
            if max_valid_height < self.block_height {
                return Err(VerificationError::InvalidBlockContext(
                    "max_valid_height must be at least block_height".to_owned(),
                ));
            }
        }

        Ok(())
    }

    fn validate_at(&self, current_height: u64) -> Result<(), VerificationError> {
        self.validate()?;
        if current_height == 0 {
            return Err(VerificationError::InvalidBlockContext(
                "current verification height must be greater than zero".to_owned(),
            ));
        }
        if current_height < self.block_height {
            return Err(VerificationError::ProofFromFuture {
                current_height,
                proof_block_height: self.block_height,
            });
        }
        if let Some(proof_max_height) = self.max_valid_height {
            if current_height > proof_max_height {
                return Err(VerificationError::ProofExpired {
                    current_height,
                    proof_max_height,
                });
            }
        }

        Ok(())
    }
}

/// Public input vector. The vector order is consensus-critical and is
/// preserved exactly in canonical encoding; it is never sorted.
///
/// Schema version 1 reserves the final two slots for the witness commitment:
/// the first is the high 128-bit limb and the second is the low 128-bit limb.
/// Both limbs are encoded as 32-byte big-endian BN254 field elements with 16
/// leading zero bytes. Circuit-specific public inputs precede these slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInput {
    /// Ordered BN254 scalar-field public inputs, ending with the reserved
    /// witness-commitment limbs for schema version 1.
    pub values: Vec<FieldElement>,
}

impl PublicInput {
    /// Construct an ordered public-input vector and validate every field.
    pub fn new(values: Vec<FieldElement>) -> Result<Self, VerificationError> {
        let inputs = Self { values };
        inputs.validate()?;
        Ok(inputs)
    }

    fn validate(&self) -> Result<(), VerificationError> {
        if self.values.is_empty() {
            return Err(VerificationError::InvalidPublicInputs(
                "at least one public input is required".to_owned(),
            ));
        }
        if self.values.len() > MAX_FIELD_ELEMENTS {
            return Err(VerificationError::InvalidPublicInputs(format!(
                "{} public inputs exceeds the limit of {MAX_FIELD_ELEMENTS}",
                self.values.len()
            )));
        }
        for (index, value) in self.values.iter().enumerate() {
            value.validate(Some(index))?;
        }

        Ok(())
    }
}

/// Verification-key identifier. It is not an arbitrary label: it must equal
/// the domain-separated SHA-256 digest of the registered key bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VerificationKeyId(pub [u8; 32]);

impl VerificationKeyId {
    /// Derive an identifier from the exact verification-key bytes.
    pub fn from_key_bytes(vk_bytes: &[u8]) -> Result<Self, VerificationError> {
        if vk_bytes.is_empty() {
            return Err(VerificationError::InvalidVerificationKey(
                "verification key bytes must not be empty".to_owned(),
            ));
        }
        if vk_bytes.len() > MAX_VERIFICATION_KEY_BYTES {
            return Err(VerificationError::InvalidVerificationKey(format!(
                "verification key is larger than the {MAX_VERIFICATION_KEY_BYTES}-byte limit"
            )));
        }

        Ok(Self(hash_domain_separated(
            VERIFICATION_KEY_ID_DOMAIN,
            vk_bytes,
        )))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Encode a 32-byte witness commitment as the two schema-v1 public-input
/// limbs. The commitment bytes are interpreted in big-endian order: bytes
/// `0..16` become the high 128-bit limb and bytes `16..32` become the low
/// 128-bit limb. Each limb is zero-extended on the left to a 32-byte BN254
/// field element; no modulus reduction is performed.
pub fn witness_commitment_public_inputs(
    witness_commitment: [u8; 32],
) -> Result<[FieldElement; WITNESS_COMMITMENT_PUBLIC_INPUT_LIMBS], VerificationError> {
    let mut high = [0u8; BN254_FIELD_ELEMENT_BYTES];
    let mut low = [0u8; BN254_FIELD_ELEMENT_BYTES];
    let high_start = BN254_FIELD_ELEMENT_BYTES - WITNESS_COMMITMENT_PUBLIC_INPUT_LIMB_BYTES;
    let split = WITNESS_COMMITMENT_PUBLIC_INPUT_LIMB_BYTES;
    high[high_start..].copy_from_slice(&witness_commitment[..split]);
    low[high_start..].copy_from_slice(&witness_commitment[split..]);

    Ok([
        FieldElement::from_bytes(high)?,
        FieldElement::from_bytes(low)?,
    ])
}

fn validate_circuit_id(circuit_id: &str) -> Result<(), VerificationError> {
    if circuit_id.is_empty() {
        return Err(VerificationError::InvalidCircuitId(
            "circuit_id must not be empty".to_owned(),
        ));
    }
    if circuit_id.len() > MAX_CIRCUIT_ID_BYTES {
        return Err(VerificationError::InvalidCircuitId(format!(
            "circuit_id is larger than the {MAX_CIRCUIT_ID_BYTES}-byte limit"
        )));
    }
    if !circuit_id.is_ascii()
        || circuit_id
            .chars()
            .any(|character| !character.is_ascii_graphic())
    {
        return Err(VerificationError::InvalidCircuitId(
            "circuit_id must contain only non-whitespace ASCII graphic characters".to_owned(),
        ));
    }

    Ok(())
}

/// Canonical Groth16 statement. No raw witness material is present here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Groth16Statement {
    /// Canonical schema version for this statement.
    pub schema_version: u16,
    /// Curve and scalar-field encoding selected by the schema.
    pub curve: Groth16Curve,
    /// Stable circuit identifier included in the statement hash.
    pub circuit_id: String,
    /// Domain-separated identifier of the exact verification-key bytes.
    pub verification_key_id: VerificationKeyId,
    /// Ordered public inputs supplied to the circuit.
    pub public_inputs: PublicInput,
    /// Commitment to prover-side witness field elements.
    pub witness_commitment: [u8; 32],
    /// Bitcoin anchor and freshness context bound into the statement.
    pub block_context: BitcoinBlockContext,
}

impl Groth16Statement {
    /// Validate all statement fields without invoking a backend.
    pub fn validate(&self) -> Result<(), VerificationError> {
        if self.schema_version != GROTH16_SCHEMA_VERSION {
            return Err(VerificationError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if self.curve != Groth16Curve::Bn254 {
            return Err(VerificationError::UnsupportedCurve(format!(
                "unsupported curve tag for schema version {}",
                self.schema_version
            )));
        }
        validate_circuit_id(&self.circuit_id)?;
        if self.verification_key_id.0 == [0u8; 32] {
            return Err(VerificationError::InvalidVerificationKey(
                "verification_key_id must be non-zero".to_owned(),
            ));
        }
        if self.witness_commitment == [0u8; 32] {
            return Err(VerificationError::InvalidWitnessCommitment(
                "witness_commitment must be non-zero".to_owned(),
            ));
        }

        self.public_inputs.validate()?;
        self.validate_witness_commitment_public_inputs()?;
        self.block_context.validate()
    }

    fn validate_witness_commitment_public_inputs(&self) -> Result<(), VerificationError> {
        let expected = witness_commitment_public_inputs(self.witness_commitment)?;
        let start = self
            .public_inputs
            .values
            .len()
            .checked_sub(WITNESS_COMMITMENT_PUBLIC_INPUT_LIMBS)
            .ok_or_else(|| {
                VerificationError::InvalidPublicInputs(format!(
                    "schema version {GROTH16_SCHEMA_VERSION} reserves the final {} public-input slots for the witness commitment",
                    WITNESS_COMMITMENT_PUBLIC_INPUT_LIMBS
                ))
            })?;

        for (offset, expected_value) in expected.iter().enumerate() {
            let slot = start + offset;
            let found = self.public_inputs.values[slot];
            if found != *expected_value {
                return Err(VerificationError::WitnessCommitmentPublicInputMismatch {
                    slot,
                    expected: *expected_value,
                    found,
                });
            }
        }

        Ok(())
    }

    /// Return the deterministic, length-framed statement encoding.
    pub fn canonical_encode(&self) -> Result<Vec<u8>, VerificationError> {
        self.validate()?;

        let mut encoded = Vec::with_capacity(
            STATEMENT_ENCODING_DOMAIN.len()
                + 2
                + 2
                + 4
                + self.circuit_id.len()
                + 32
                + 4
                + self.public_inputs.values.len() * BN254_FIELD_ELEMENT_BYTES
                + 32
                + 1
                + 8
                + 32
                + 1
                + 8,
        );
        encoded.extend_from_slice(STATEMENT_ENCODING_DOMAIN);
        encoded.extend_from_slice(&self.schema_version.to_be_bytes());
        encoded.push(self.curve.tag());
        encoded.push(FIELD_ENCODING_BN254_BIG_ENDIAN_32);
        append_length_prefixed(&mut encoded, self.circuit_id.as_bytes());
        encoded.extend_from_slice(self.verification_key_id.as_bytes());
        append_u32(&mut encoded, self.public_inputs.values.len());
        for value in &self.public_inputs.values {
            encoded.extend_from_slice(value.as_bytes());
        }
        encoded.extend_from_slice(&self.witness_commitment);
        encoded.push(self.block_context.network.tag());
        encoded.extend_from_slice(&self.block_context.block_height.to_be_bytes());
        encoded.extend_from_slice(&self.block_context.block_hash);
        match self.block_context.max_valid_height {
            Some(max_valid_height) => {
                encoded.push(1);
                encoded.extend_from_slice(&max_valid_height.to_be_bytes());
            }
            None => encoded.push(0),
        }

        Ok(encoded)
    }

    /// Hash the canonical statement with a distinct statement-hash domain.
    pub fn statement_hash(&self) -> Result<[u8; 32], VerificationError> {
        let encoded = self.canonical_encode()?;
        Ok(hash_domain_separated(STATEMENT_HASH_DOMAIN, &encoded))
    }
}

/// Opaque compressed proof bytes accepted by this boundary.
///
/// The contract requires exactly 128 bytes: 32 bytes for compressed G1 A,
/// 64 bytes for compressed G2 B, and 32 bytes for compressed G1 C. The
/// boundary checks width and presence only; subgroup, curve-point, and pairing
/// checks belong to a future backend and are intentionally not claimed here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Groth16Proof {
    /// Canonical 128-byte compressed proof encoding.
    bytes: Vec<u8>,
}

impl Groth16Proof {
    /// Construct a proof from the exact fixed-width compressed encoding.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, VerificationError> {
        if bytes.len() != GROTH16_COMPRESSED_PROOF_BYTES {
            return Err(VerificationError::InvalidProof(
                InvalidProofReason::MalformedEncoding(format!(
                    "compressed BN254 proof must be exactly {GROTH16_COMPRESSED_PROOF_BYTES} bytes"
                )),
            ));
        }
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(VerificationError::InvalidProof(
                InvalidProofReason::MalformedEncoding(
                    "compressed proof must not be all zero bytes".to_owned(),
                ),
            ));
        }

        Ok(Self { bytes })
    }

    /// Return the canonical compressed proof bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Hash the proof bytes for deterministic fixture matching.
    pub fn digest(&self) -> [u8; 32] {
        hash_domain_separated(PROOF_DIGEST_DOMAIN, &self.bytes)
    }

    fn validate(&self) -> Result<(), VerificationError> {
        Self::from_bytes(self.bytes.clone()).map(|_| ())
    }
}

impl<'de> Deserialize<'de> for Groth16Proof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        Self::from_bytes(bytes).map_err(D::Error::custom)
    }
}

/// Runtime request passed from an adapter to an injected verifier backend.
///
/// It contains public inputs, a witness commitment, a statement hash, and the
/// proof bytes. It never carries raw witness values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Groth16VerificationRequest {
    /// Canonical statement bound to the proof.
    pub statement: Groth16Statement,
    /// Opaque compressed proof bytes for the selected curve.
    pub proof: Groth16Proof,
    /// The caller-supplied expected hash. It must equal the hash of
    /// `statement` before any backend call.
    pub statement_hash: [u8; 32],
    /// Current chain height used only for block-context freshness checks; it
    /// is not part of the statement hash.
    pub current_block_height: u64,
}

impl Groth16VerificationRequest {
    /// Build and fully validate a request using a freshly computed statement hash.
    pub fn new(
        statement: Groth16Statement,
        proof: Groth16Proof,
        current_block_height: u64,
    ) -> Result<Self, VerificationError> {
        let statement_hash = statement.statement_hash()?;
        let request = Self {
            statement,
            proof,
            statement_hash,
            current_block_height,
        };
        request.validate()?;
        Ok(request)
    }

    /// Validate every boundary invariant before invoking a backend.
    pub fn validate(&self) -> Result<(), VerificationError> {
        self.statement.validate()?;
        self.proof.validate()?;
        self.statement
            .block_context
            .validate_at(self.current_block_height)?;

        let computed_hash = self.statement.statement_hash()?;
        if self.statement_hash != computed_hash {
            return Err(VerificationError::StatementHashMismatch {
                expected: computed_hash,
                supplied: self.statement_hash,
            });
        }

        Ok(())
    }
}

/// Result returned by a backend after the canonical request has been
/// accepted. `valid` only means that the backend accepted the request; the
/// mock backend below is explicitly not cryptographic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the injected backend accepted the request.
    pub valid: bool,
    /// Verification key identifier used by the backend.
    pub vk_id: VerificationKeyId,
    /// Ordered public inputs accepted by the backend.
    pub public_inputs: PublicInput,
    /// Canonical statement hash that was verified.
    pub statement_hash: [u8; 32],
    /// Backend transcript or acceptance digest.
    pub transcript: [u8; 32],
    /// Current chain height at which the request was accepted.
    pub verified_at_height: u64,
}

/// Stable reasons for a proof being rejected as invalid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum InvalidProofReason {
    #[error("malformed proof encoding: {0}")]
    MalformedEncoding(String),
    #[error("backend returned an invalid proof result")]
    BackendRejected,
    #[error("proof digest does not match the registered deterministic fixture")]
    FixtureMismatch,
}

/// Errors raised by the contract boundary or a test backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum VerificationError {
    #[error("unsupported Groth16 schema version {0}")]
    UnsupportedSchemaVersion(u16),
    #[error("unsupported Groth16 curve: {0}")]
    UnsupportedCurve(String),
    #[error("invalid circuit identifier: {0}")]
    InvalidCircuitId(String),
    #[error("invalid field element at {index:?}: {reason}")]
    InvalidFieldElement {
        index: Option<usize>,
        reason: String,
    },
    #[error("invalid public inputs: {0}")]
    InvalidPublicInputs(String),
    #[error("invalid witness: {0}")]
    InvalidWitness(String),
    #[error("invalid witness commitment: {0}")]
    InvalidWitnessCommitment(String),
    #[error("witness commitment public-input mismatch at slot {slot}: expected {expected:?}, found {found:?}")]
    WitnessCommitmentPublicInputMismatch {
        slot: usize,
        expected: FieldElement,
        found: FieldElement,
    },
    #[error("invalid proof: {0}")]
    InvalidProof(InvalidProofReason),
    #[error("invalid verification key: {0}")]
    InvalidVerificationKey(String),
    #[error("verification key id mismatch: supplied {supplied:?}, derived {derived:?}")]
    VerificationKeyIdMismatch {
        supplied: VerificationKeyId,
        derived: VerificationKeyId,
    },
    #[error("verification key is not registered: {0:?}")]
    VerificationKeyNotFound(VerificationKeyId),
    #[error(
        "verification-key association mismatch for {verification_key_id:?}: registered ({registered_circuit_id}, schema {registered_schema_version}, curve {registered_curve:?}), requested ({requested_circuit_id}, schema {requested_schema_version}, curve {requested_curve:?})"
    )]
    VerificationKeyAssociationMismatch {
        verification_key_id: VerificationKeyId,
        registered_circuit_id: String,
        registered_schema_version: u16,
        registered_curve: Groth16Curve,
        requested_circuit_id: String,
        requested_schema_version: u16,
        requested_curve: Groth16Curve,
    },
    #[error("statement hash mismatch: expected {expected:?}, supplied {supplied:?}")]
    StatementHashMismatch {
        expected: [u8; 32],
        supplied: [u8; 32],
    },
    #[error("proof expired at height {proof_max_height}; current height is {current_height}")]
    ProofExpired {
        current_height: u64,
        proof_max_height: u64,
    },
    #[error("proof is anchored at future height {proof_block_height}; current height is {current_height}")]
    ProofFromFuture {
        current_height: u64,
        proof_block_height: u64,
    },
    #[error("circuit mismatch: expected {expected}, found {found}")]
    CircuitMismatch { expected: String, found: String },
    #[error("invalid Bitcoin block context: {0}")]
    InvalidBlockContext(String),
    #[error("malformed BitVM envelope: {0}")]
    MalformedEnvelope(String),
    #[error("raw witness material is not accepted at the runtime verifier boundary")]
    RawWitnessProvided,
    #[error("test fixture is not registered for statement {0:?}")]
    FixtureNotRegistered([u8; 32]),
    #[error("backend verifier is not configured")]
    VerifierUnavailable,
    #[error("verifier backend state is unavailable: {0}")]
    BackendState(String),
}

/// Backend-neutral verifier trait. Implementations must validate or otherwise
/// honor [`Groth16VerificationRequest::validate`] and the explicit
/// circuit-to-verification-key association before doing cryptographic work.
#[async_trait]
pub trait Groth16Verifier: Send + Sync {
    /// Verify a canonical proof request. No raw witness is available here.
    async fn verify(
        &self,
        request: &Groth16VerificationRequest,
    ) -> Result<VerificationResult, VerificationError>;

    /// Register exact key bytes together with the circuit/schema/curve they
    /// serve. The key identifier must match the exact bytes.
    async fn register_verification_key(
        &self,
        circuit_id: String,
        schema_version: u16,
        curve: Groth16Curve,
        vk_id: VerificationKeyId,
        vk_bytes: Vec<u8>,
    ) -> Result<(), VerificationError>;

    /// Reject unknown or mismatched circuit/schema/curve-to-key associations
    /// before proof verification is attempted.
    async fn validate_verification_key_association(
        &self,
        statement: &Groth16Statement,
    ) -> Result<(), VerificationError>;
}

/// Deterministic, test-only verifier.
///
/// This verifier does not perform Groth16 pairings. A fixture must register
/// both its exact key bytes and its exact proof digest; verification then
/// enforces the full boundary contract and accepts only that deterministic
/// fixture. It is intentionally unsuitable for production cryptographic
/// verification.
#[cfg(any(test, feature = "mock-integrations"))]
#[derive(Debug, Default)]
pub struct MockGroth16Verifier {
    key_records: RwLock<HashMap<VerificationKeyId, VerificationKeyRecord>>,
    fixture_proofs: RwLock<HashMap<[u8; 32], [u8; 32]>>,
}

#[cfg(any(test, feature = "mock-integrations"))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct VerificationKeyRecord {
    circuit_id: String,
    schema_version: u16,
    curve: Groth16Curve,
    bytes: Vec<u8>,
}

#[cfg(any(test, feature = "mock-integrations"))]
impl MockGroth16Verifier {
    /// Create an empty deterministic fixture verifier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a complete deterministic fixture for tests.
    pub async fn register_fixture(
        &self,
        request: &Groth16VerificationRequest,
        vk_bytes: Vec<u8>,
    ) -> Result<(), VerificationError> {
        request.validate()?;
        self.register_verification_key(
            request.statement.circuit_id.clone(),
            request.statement.schema_version,
            request.statement.curve,
            request.statement.verification_key_id,
            vk_bytes,
        )
        .await?;
        let mut fixtures = self
            .fixture_proofs
            .write()
            .map_err(|_| VerificationError::BackendState("fixture lock poisoned".to_owned()))?;
        fixtures.insert(request.statement_hash, request.proof.digest());
        Ok(())
    }
}

#[cfg(any(test, feature = "mock-integrations"))]
#[async_trait]
impl Groth16Verifier for MockGroth16Verifier {
    async fn verify(
        &self,
        request: &Groth16VerificationRequest,
    ) -> Result<VerificationResult, VerificationError> {
        request.validate()?;
        self.validate_verification_key_association(&request.statement)
            .await?;

        let fixtures = self
            .fixture_proofs
            .read()
            .map_err(|_| VerificationError::BackendState("fixture lock poisoned".to_owned()))?;
        let expected_proof_digest = fixtures.get(&request.statement_hash).ok_or(
            VerificationError::FixtureNotRegistered(request.statement_hash),
        )?;
        let actual_proof_digest = request.proof.digest();
        if *expected_proof_digest != actual_proof_digest {
            return Err(VerificationError::InvalidProof(
                InvalidProofReason::FixtureMismatch,
            ));
        }

        let mut transcript_payload = Vec::with_capacity(64);
        transcript_payload.extend_from_slice(&request.statement_hash);
        transcript_payload.extend_from_slice(&actual_proof_digest);

        Ok(VerificationResult {
            valid: true,
            vk_id: request.statement.verification_key_id,
            public_inputs: request.statement.public_inputs.clone(),
            statement_hash: request.statement_hash,
            transcript: hash_domain_separated(TRANSCRIPT_DOMAIN, &transcript_payload),
            verified_at_height: request.current_block_height,
        })
    }

    async fn register_verification_key(
        &self,
        circuit_id: String,
        schema_version: u16,
        curve: Groth16Curve,
        vk_id: VerificationKeyId,
        vk_bytes: Vec<u8>,
    ) -> Result<(), VerificationError> {
        validate_circuit_id(&circuit_id)?;
        if schema_version != GROTH16_SCHEMA_VERSION {
            return Err(VerificationError::UnsupportedSchemaVersion(schema_version));
        }
        if curve != Groth16Curve::Bn254 {
            return Err(VerificationError::UnsupportedCurve(format!(
                "unsupported curve tag for schema version {schema_version}"
            )));
        }

        let derived = VerificationKeyId::from_key_bytes(&vk_bytes)?;
        if derived != vk_id {
            return Err(VerificationError::VerificationKeyIdMismatch {
                supplied: vk_id,
                derived,
            });
        }

        let record = VerificationKeyRecord {
            circuit_id,
            schema_version,
            curve,
            bytes: vk_bytes,
        };
        let mut keys = self.key_records.write().map_err(|_| {
            VerificationError::BackendState("verification-key lock poisoned".to_owned())
        })?;
        if let Some(existing) = keys.get(&vk_id) {
            if existing != &record {
                return Err(VerificationError::VerificationKeyAssociationMismatch {
                    verification_key_id: vk_id,
                    registered_circuit_id: existing.circuit_id.clone(),
                    registered_schema_version: existing.schema_version,
                    registered_curve: existing.curve,
                    requested_circuit_id: record.circuit_id,
                    requested_schema_version: record.schema_version,
                    requested_curve: record.curve,
                });
            }
        } else {
            keys.insert(vk_id, record);
        }
        Ok(())
    }

    async fn validate_verification_key_association(
        &self,
        statement: &Groth16Statement,
    ) -> Result<(), VerificationError> {
        let keys = self.key_records.read().map_err(|_| {
            VerificationError::BackendState("verification-key lock poisoned".to_owned())
        })?;
        let record = keys.get(&statement.verification_key_id).ok_or(
            VerificationError::VerificationKeyNotFound(statement.verification_key_id),
        )?;
        let derived = VerificationKeyId::from_key_bytes(&record.bytes)?;
        if derived != statement.verification_key_id {
            return Err(VerificationError::VerificationKeyIdMismatch {
                supplied: statement.verification_key_id,
                derived,
            });
        }
        if record.circuit_id != statement.circuit_id
            || record.schema_version != statement.schema_version
            || record.curve != statement.curve
        {
            return Err(VerificationError::VerificationKeyAssociationMismatch {
                verification_key_id: statement.verification_key_id,
                registered_circuit_id: record.circuit_id.clone(),
                registered_schema_version: record.schema_version,
                registered_curve: record.curve,
                requested_circuit_id: statement.circuit_id.clone(),
                requested_schema_version: statement.schema_version,
                requested_curve: statement.curve,
            });
        }
        Ok(())
    }
}

/// Legacy conversion hook retained for callers that have a separate BitVM
/// parser. The concrete production handoff lives in `bitvm_adapter.rs` and
/// creates a complete [`Groth16VerificationRequest`].
pub trait BitVmGroth16Adapter {
    fn from_bitvm_proof(bitvm_proof_bytes: &[u8]) -> Result<Groth16Proof, VerificationError>;

    fn extract_public_inputs(transcript: &[u8]) -> Result<PublicInput, VerificationError>;
}

/// Compute a witness commitment from prover-side field elements.
///
/// This helper is useful for deterministic test-vector reproduction. A
/// production runtime must call it on the prover side and submit only the
/// resulting 32-byte commitment.
pub fn compute_witness_commitment(witness: &[FieldElement]) -> Result<[u8; 32], VerificationError> {
    if witness.is_empty() {
        return Err(VerificationError::InvalidWitness(
            "at least one private witness field element is required".to_owned(),
        ));
    }
    if witness.len() > MAX_FIELD_ELEMENTS {
        return Err(VerificationError::InvalidWitness(format!(
            "{} witness elements exceeds the limit of {MAX_FIELD_ELEMENTS}",
            witness.len()
        )));
    }

    let mut encoded = Vec::with_capacity(4 + witness.len() * BN254_FIELD_ELEMENT_BYTES);
    append_u32(&mut encoded, witness.len());
    for (index, value) in witness.iter().enumerate() {
        value.validate(Some(index))?;
        encoded.extend_from_slice(value.as_bytes());
    }

    Ok(hash_domain_separated(WITNESS_COMMITMENT_DOMAIN, &encoded))
}

fn hash_domain_separated(domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut framed = Vec::with_capacity(domain.len() + 4 + payload.len());
    framed.extend_from_slice(domain);
    append_length_prefixed(&mut framed, payload);
    sha256::Hash::hash(&framed).to_byte_array()
}

fn append_length_prefixed(output: &mut Vec<u8>, bytes: &[u8]) {
    append_u32(output, bytes.len());
    output.extend_from_slice(bytes);
}

fn append_u32(output: &mut Vec<u8>, value: usize) {
    let value = u32::try_from(value).expect("contract limits keep lengths within u32");
    output.extend_from_slice(&value.to_be_bytes());
}

impl fmt::Display for VerificationKeyId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(value: u8) -> FieldElement {
        FieldElement::from_bytes({
            let mut bytes = [0u8; 32];
            bytes[31] = value;
            bytes
        })
        .unwrap()
    }

    fn statement() -> Groth16Statement {
        let key_id = VerificationKeyId::from_key_bytes(b"test-vk").unwrap();
        let witness_commitment = compute_witness_commitment(&[field(3), field(4)]).unwrap();
        let commitment_inputs = witness_commitment_public_inputs(witness_commitment).unwrap();
        let mut public_inputs = vec![field(1), field(2)];
        public_inputs.extend_from_slice(&commitment_inputs);
        Groth16Statement {
            schema_version: GROTH16_SCHEMA_VERSION,
            curve: Groth16Curve::Bn254,
            circuit_id: "test-circuit-v1".to_owned(),
            verification_key_id: key_id,
            public_inputs: PublicInput::new(public_inputs).unwrap(),
            witness_commitment,
            block_context: BitcoinBlockContext {
                network: BitcoinNetwork::Regtest,
                block_height: 100,
                block_hash: [7u8; 32],
                max_valid_height: Some(120),
            },
        }
    }

    fn proof() -> Groth16Proof {
        Groth16Proof::from_bytes((1u8..=GROTH16_COMPRESSED_PROOF_BYTES as u8).collect()).unwrap()
    }

    #[test]
    fn canonical_statement_hash_is_order_sensitive() {
        let first = statement();
        let mut second = first.clone();
        second.public_inputs.values.swap(0, 1);

        assert_ne!(
            first.statement_hash().unwrap(),
            second.statement_hash().unwrap()
        );
    }

    #[tokio::test]
    async fn mock_requires_registered_key_and_fixture() {
        let verifier = MockGroth16Verifier::new();
        let statement = statement();
        let request = Groth16VerificationRequest::new(statement, proof(), 105).unwrap();

        assert!(matches!(
            verifier.verify(&request).await,
            Err(VerificationError::VerificationKeyNotFound(_))
        ));

        verifier
            .register_fixture(&request, b"test-vk".to_vec())
            .await
            .unwrap();
        let result = verifier.verify(&request).await.unwrap();
        assert!(result.valid);
        assert_eq!(result.statement_hash, request.statement_hash);
    }

    #[test]
    fn field_modulus_and_proof_width_are_enforced() {
        assert!(FieldElement::from_bytes(BN254_SCALAR_MODULUS).is_err());
        assert!(Groth16Proof::from_bytes(vec![1u8; GROTH16_COMPRESSED_PROOF_BYTES - 1]).is_err());
        assert!(Groth16Proof::from_bytes(vec![0u8; GROTH16_COMPRESSED_PROOF_BYTES]).is_err());
    }
}
