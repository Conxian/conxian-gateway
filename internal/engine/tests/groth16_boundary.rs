use async_trait::async_trait;
use conxian_core::ConxianError;
use conxian_engine::bitcoin::{
    compute_witness_commitment, parse_bitvm_groth16_envelope, witness_commitment_public_inputs,
    BitcoinBlockContext, BitcoinNetwork, FieldElement, Groth16Curve, Groth16Proof,
    Groth16Statement, Groth16VerificationRequest, Groth16Verifier, InvalidProofReason,
    MockGroth16Verifier, PublicInput, VerificationError, VerificationKeyId, VerificationResult,
    BN254_SCALAR_MODULUS, GROTH16_COMPRESSED_PROOF_BYTES, GROTH16_SCHEMA_VERSION,
};
use conxian_engine::BitVmAdapter;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

const FIXTURE_JSON: &str = include_str!("fixtures/groth16/bitvm_fixture.json");
const EXPECTED_FIXTURE_VERIFICATION_KEY_ID: &str =
    "1577f847a088d7ba6df804f6f5fc2b31a3b1b42e896a5faaa50c2df7585f5727";
const EXPECTED_FIXTURE_WITNESS_COMMITMENT: &str =
    "011d59399ff1bdd26b5928ae8d0ea549017a4441a37b76e8af0392de98b0ebad";
const EXPECTED_FIXTURE_STATEMENT_HASH: &str =
    "cb583f8fc4f52243c60340851d9fe4bfe865bfbe0ae41f1a316cefd893649758";

#[derive(Debug, Deserialize)]
struct Fixture {
    schema_version: u16,
    curve: String,
    circuit_id: String,
    verification_key_bytes: String,
    verification_key_id: String,
    public_inputs: Vec<String>,
    witness_values: Vec<String>,
    witness_commitment: String,
    block_context: FixtureBlockContext,
    proof: String,
    statement_hash: String,
    current_block_height: u64,
    expected_valid: bool,
}

#[derive(Debug, Deserialize)]
struct FixtureBlockContext {
    network: String,
    block_height: u64,
    block_hash: String,
    max_valid_height: Option<u64>,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE_JSON).expect("fixture JSON must be valid")
}

fn decode_32(value: &str) -> [u8; 32] {
    hex::decode(value)
        .expect("fixture hex must decode")
        .try_into()
        .expect("fixture value must be 32 bytes")
}

fn field(value: &str) -> FieldElement {
    FieldElement::from_bytes(decode_32(value)).expect("fixture field must be canonical")
}

fn fixture_statement(data: &Fixture) -> Groth16Statement {
    let key_bytes = hex::decode(&data.verification_key_bytes).expect("fixture key hex");
    let key_id = VerificationKeyId::from_key_bytes(&key_bytes).expect("fixture key id");
    assert_eq!(
        data.verification_key_id,
        EXPECTED_FIXTURE_VERIFICATION_KEY_ID
    );
    assert_eq!(hex::encode(key_id.0), data.verification_key_id);

    let witness: Vec<_> = data
        .witness_values
        .iter()
        .map(|value| field(value))
        .collect();
    let witness_commitment = compute_witness_commitment(&witness).expect("fixture commitment");
    assert_eq!(data.witness_commitment, EXPECTED_FIXTURE_WITNESS_COMMITMENT);
    assert_eq!(hex::encode(witness_commitment), data.witness_commitment);

    let public_inputs = PublicInput::new(
        data.public_inputs
            .iter()
            .map(|value| field(value))
            .collect(),
    )
    .expect("fixture public inputs");
    let commitment_inputs = witness_commitment_public_inputs(witness_commitment)
        .expect("fixture commitment public inputs");
    assert_eq!(public_inputs.values.len(), 5);
    assert_eq!(&public_inputs.values[3..], commitment_inputs.as_slice());

    Groth16Statement {
        schema_version: data.schema_version,
        curve: data.curve.parse::<Groth16Curve>().expect("fixture curve"),
        circuit_id: data.circuit_id.clone(),
        verification_key_id: key_id,
        public_inputs,
        witness_commitment,
        block_context: BitcoinBlockContext {
            network: data
                .block_context
                .network
                .parse::<BitcoinNetwork>()
                .expect("fixture network"),
            block_height: data.block_context.block_height,
            block_hash: decode_32(&data.block_context.block_hash),
            max_valid_height: data.block_context.max_valid_height,
        },
    }
}

fn fixture_request(data: &Fixture, current_height: u64) -> Groth16VerificationRequest {
    let statement = fixture_statement(data);
    let proof = Groth16Proof::from_bytes(hex::decode(&data.proof).expect("fixture proof hex"))
        .expect("fixture proof encoding");
    let request = Groth16VerificationRequest::new(statement, proof, current_height)
        .expect("fixture request must validate");
    assert_eq!(data.statement_hash, EXPECTED_FIXTURE_STATEMENT_HASH);
    assert_eq!(hex::encode(request.statement_hash), data.statement_hash);
    request
}

fn fixture_envelope(data: &Fixture) -> Value {
    json!({
        "schema_version": data.schema_version,
        "curve": data.curve,
        "circuit_id": data.circuit_id,
        "verification_key_id": data.verification_key_id,
        "public_inputs": data.public_inputs,
        "witness_commitment": data.witness_commitment,
        "block_context": {
            "network": data.block_context.network,
            "block_height": data.block_context.block_height,
            "block_hash": data.block_context.block_hash,
            "max_valid_height": data.block_context.max_valid_height,
        },
        "proof": data.proof,
        "statement_hash": data.statement_hash,
    })
}

#[derive(Default)]
struct CountingVerifier {
    verify_calls: AtomicUsize,
}

#[async_trait]
impl Groth16Verifier for CountingVerifier {
    async fn verify(
        &self,
        _request: &Groth16VerificationRequest,
    ) -> Result<VerificationResult, VerificationError> {
        self.verify_calls.fetch_add(1, Ordering::SeqCst);
        Err(VerificationError::InvalidProof(
            InvalidProofReason::BackendRejected,
        ))
    }

    async fn register_verification_key(
        &self,
        _circuit_id: String,
        _schema_version: u16,
        _curve: Groth16Curve,
        _vk_id: VerificationKeyId,
        _vk_bytes: Vec<u8>,
    ) -> Result<(), VerificationError> {
        Ok(())
    }

    async fn validate_verification_key_association(
        &self,
        _statement: &Groth16Statement,
    ) -> Result<(), VerificationError> {
        Ok(())
    }
}

#[derive(Default)]
struct InvalidResultVerifier {
    verify_calls: AtomicUsize,
}

#[async_trait]
impl Groth16Verifier for InvalidResultVerifier {
    async fn verify(
        &self,
        request: &Groth16VerificationRequest,
    ) -> Result<VerificationResult, VerificationError> {
        self.verify_calls.fetch_add(1, Ordering::SeqCst);
        Ok(VerificationResult {
            valid: false,
            vk_id: request.statement.verification_key_id,
            public_inputs: request.statement.public_inputs.clone(),
            statement_hash: request.statement_hash,
            transcript: [0u8; 32],
            verified_at_height: request.current_block_height,
        })
    }

    async fn register_verification_key(
        &self,
        _circuit_id: String,
        _schema_version: u16,
        _curve: Groth16Curve,
        _vk_id: VerificationKeyId,
        _vk_bytes: Vec<u8>,
    ) -> Result<(), VerificationError> {
        Ok(())
    }

    async fn validate_verification_key_association(
        &self,
        _statement: &Groth16Statement,
    ) -> Result<(), VerificationError> {
        Ok(())
    }
}

#[tokio::test]
async fn fixture_reproduces_commitment_hash_and_validates_end_to_end() {
    let data = fixture();
    assert!(data.expected_valid);
    assert_eq!(data.schema_version, GROTH16_SCHEMA_VERSION);

    let request = fixture_request(&data, data.current_block_height);
    let key_bytes = hex::decode(&data.verification_key_bytes).unwrap();
    let verifier = Arc::new(MockGroth16Verifier::new());
    verifier
        .register_fixture(&request, key_bytes)
        .await
        .unwrap();

    let result = verifier.verify(&request).await.unwrap();
    assert!(result.valid);
    assert_eq!(result.statement_hash, request.statement_hash);
    assert_eq!(result.public_inputs.values.len(), 5);

    let adapter = BitVmAdapter::with_verifier("regtest".to_owned(), Arc::clone(&verifier));
    let handoff = adapter
        .verify_groth16_envelope(fixture_envelope(&data), data.current_block_height)
        .await
        .unwrap();
    assert!(handoff.valid);
    assert_eq!(handoff.statement_hash, request.statement_hash);
}

#[tokio::test]
async fn borrowed_bitvm_handoff_validates_before_delegation() {
    let data = fixture();
    let request = fixture_request(&data, data.current_block_height);
    let verifier = MockGroth16Verifier::new();
    verifier
        .register_fixture(&request, hex::decode(&data.verification_key_bytes).unwrap())
        .await
        .unwrap();

    let adapter = BitVmAdapter::new("regtest".to_owned());
    let result = adapter
        .verify_groth16_envelope_with(
            &verifier,
            fixture_envelope(&data),
            data.current_block_height,
        )
        .await
        .unwrap();
    assert!(result.valid);
}

#[tokio::test]
async fn malformed_envelope_is_rejected_before_verifier_invocation() {
    let data = fixture();
    let verifier = CountingVerifier::default();
    let adapter = BitVmAdapter::new("regtest".to_owned());
    let mut envelope = fixture_envelope(&data);
    envelope["statement_hash"] = json!("00".repeat(32));

    assert!(adapter
        .verify_groth16_envelope_with(&verifier, envelope, data.current_block_height)
        .await
        .is_err());
    assert_eq!(verifier.verify_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn witness_commitment_public_input_mismatch_is_rejected_before_backend() {
    let data = fixture();
    let verifier = CountingVerifier::default();
    let adapter = BitVmAdapter::new("regtest".to_owned());
    let mut envelope = fixture_envelope(&data);
    envelope["public_inputs"][4] =
        json!("00000000000000000000000000000000017a4441a37b76e8af0392de98b0ebae");

    assert!(matches!(
        parse_bitvm_groth16_envelope(envelope.clone(), data.current_block_height),
        Err(VerificationError::WitnessCommitmentPublicInputMismatch { slot: 4, .. })
    ));
    let error = adapter
        .verify_groth16_envelope_with(&verifier, envelope, data.current_block_height)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ConxianError::Security(message)
            if message.contains("witness commitment public-input mismatch")
    ));
    assert_eq!(verifier.verify_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn backend_invalid_result_is_mapped_to_fail_closed_invalid_proof() {
    let data = fixture();
    let verifier = InvalidResultVerifier::default();
    let adapter = BitVmAdapter::new("regtest".to_owned());

    let error = adapter
        .verify_groth16_envelope_with(
            &verifier,
            fixture_envelope(&data),
            data.current_block_height,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ConxianError::Security(message)
            if message.contains("invalid proof: backend returned an invalid proof result")
    ));
    assert_eq!(verifier.verify_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn public_input_reorder_mutation_and_statement_hash_tampering_are_rejected() {
    let data = fixture();
    let request = fixture_request(&data, data.current_block_height);
    let verifier = MockGroth16Verifier::new();
    verifier
        .register_fixture(&request, hex::decode(&data.verification_key_bytes).unwrap())
        .await
        .unwrap();

    let mut stale_hash = request.clone();
    stale_hash.statement.public_inputs.values.swap(0, 1);
    assert!(matches!(
        verifier.verify(&stale_hash).await,
        Err(VerificationError::StatementHashMismatch { .. })
    ));

    let mut reordered = request.statement.clone();
    reordered.public_inputs.values.swap(0, 1);
    let reordered_request = Groth16VerificationRequest::new(
        reordered,
        request.proof.clone(),
        request.current_block_height,
    )
    .unwrap();
    assert!(matches!(
        verifier.verify(&reordered_request).await,
        Err(VerificationError::FixtureNotRegistered(_))
    ));

    let mut envelope = fixture_envelope(&data);
    envelope["statement_hash"] = json!("00".repeat(32));
    assert!(matches!(
        parse_bitvm_groth16_envelope(envelope, data.current_block_height),
        Err(VerificationError::StatementHashMismatch { .. })
    ));
}

#[tokio::test]
async fn witness_commitment_proof_vk_and_circuit_mutations_are_rejected() {
    let data = fixture();
    let request = fixture_request(&data, data.current_block_height);
    let verifier = MockGroth16Verifier::new();
    verifier
        .register_fixture(&request, hex::decode(&data.verification_key_bytes).unwrap())
        .await
        .unwrap();

    let mut stale_commitment = request.clone();
    stale_commitment.statement.witness_commitment[0] ^= 1;
    assert!(matches!(
        verifier.verify(&stale_commitment).await,
        Err(VerificationError::WitnessCommitmentPublicInputMismatch { .. })
    ));

    let mut proof_bytes = request.proof.as_bytes().to_vec();
    proof_bytes[17] ^= 1;
    let proof_mutation = Groth16VerificationRequest {
        proof: Groth16Proof::from_bytes(proof_bytes).unwrap(),
        ..request.clone()
    };
    assert!(matches!(
        verifier.verify(&proof_mutation).await,
        Err(VerificationError::InvalidProof(_))
    ));

    let wrong_key_statement = Groth16Statement {
        verification_key_id: VerificationKeyId::from_key_bytes(b"wrong-vk").unwrap(),
        ..request.statement.clone()
    };
    let wrong_key_request = Groth16VerificationRequest::new(
        wrong_key_statement,
        request.proof.clone(),
        request.current_block_height,
    )
    .unwrap();
    assert!(matches!(
        verifier.verify(&wrong_key_request).await,
        Err(VerificationError::VerificationKeyNotFound(_))
    ));

    let wrong_circuit_statement = Groth16Statement {
        circuit_id: "different-circuit-v1".to_owned(),
        ..request.statement.clone()
    };
    let wrong_circuit_request = Groth16VerificationRequest::new(
        wrong_circuit_statement,
        request.proof.clone(),
        request.current_block_height,
    )
    .unwrap();
    assert!(matches!(
        verifier.verify(&wrong_circuit_request).await,
        Err(VerificationError::VerificationKeyAssociationMismatch { .. })
    ));

    assert!(matches!(
        verifier
            .register_verification_key(
                data.circuit_id.clone(),
                data.schema_version,
                Groth16Curve::Bn254,
                VerificationKeyId([9u8; 32]),
                b"wrong-vk".to_vec(),
            )
            .await,
        Err(VerificationError::VerificationKeyIdMismatch { .. })
    ));
}

#[test]
fn malformed_field_and_proof_encodings_are_rejected_before_handoff() {
    let data = fixture();
    let mut malformed_field = fixture_envelope(&data);
    malformed_field["public_inputs"][0] = json!(hex::encode(BN254_SCALAR_MODULUS));
    assert!(matches!(
        parse_bitvm_groth16_envelope(malformed_field, data.current_block_height),
        Err(VerificationError::InvalidFieldElement { index: Some(0), .. })
    ));

    let mut malformed_proof = fixture_envelope(&data);
    malformed_proof["proof"] = json!(hex::encode(vec![1u8; GROTH16_COMPRESSED_PROOF_BYTES - 1]));
    assert!(matches!(
        parse_bitvm_groth16_envelope(malformed_proof, data.current_block_height),
        Err(VerificationError::MalformedEnvelope(_))
    ));
}

#[test]
fn statement_hash_binds_identity_inputs_commitment_and_block_context() {
    let data = fixture();
    let statement = fixture_statement(&data);
    let baseline = statement.statement_hash().unwrap();

    let mut circuit = statement.clone();
    circuit.circuit_id = "different-circuit-v1".to_owned();
    assert_ne!(circuit.statement_hash().unwrap(), baseline);

    let mut key = statement.clone();
    key.verification_key_id = VerificationKeyId::from_key_bytes(b"different-vk").unwrap();
    assert_ne!(key.statement_hash().unwrap(), baseline);

    let mut public_input = statement.clone();
    public_input.public_inputs.values[0] =
        field("000000000000000000000000000000000000000000000000000000000000000b");
    assert_ne!(public_input.statement_hash().unwrap(), baseline);

    let mut commitment = statement.clone();
    commitment.witness_commitment[0] ^= 1;
    let commitment_inputs =
        witness_commitment_public_inputs(commitment.witness_commitment).unwrap();
    let commitment_start = commitment.public_inputs.values.len() - commitment_inputs.len();
    commitment.public_inputs.values[commitment_start..].copy_from_slice(&commitment_inputs);
    assert_ne!(commitment.statement_hash().unwrap(), baseline);

    let mut network = statement.clone();
    network.block_context.network = BitcoinNetwork::Mainnet;
    assert_ne!(network.statement_hash().unwrap(), baseline);

    let mut height = statement.clone();
    height.block_context.block_height += 1;
    assert_ne!(height.statement_hash().unwrap(), baseline);

    let mut block_hash = statement.clone();
    block_hash.block_context.block_hash[0] ^= 1;
    assert_ne!(block_hash.statement_hash().unwrap(), baseline);

    let mut expiry = statement;
    expiry.block_context.max_valid_height = Some(
        expiry
            .block_context
            .max_valid_height
            .expect("fixture expiry")
            + 1,
    );
    assert_ne!(expiry.statement_hash().unwrap(), baseline);
}

#[test]
fn public_field_and_proof_deserialization_reject_noncanonical_values() {
    let modulus_json = serde_json::to_string(&BN254_SCALAR_MODULUS).unwrap();
    assert!(serde_json::from_str::<FieldElement>(&modulus_json).is_err());

    let short_proof_json =
        serde_json::to_string(&vec![1u8; GROTH16_COMPRESSED_PROOF_BYTES - 1]).unwrap();
    assert!(serde_json::from_str::<Groth16Proof>(&short_proof_json).is_err());
}

#[test]
fn invalid_and_expired_block_context_and_raw_witness_are_rejected() {
    let data = fixture();

    assert!(matches!(
        Groth16VerificationRequest::new(
            fixture_statement(&data),
            Groth16Proof::from_bytes(hex::decode(&data.proof).unwrap()).unwrap(),
            data.block_context.block_height - 1,
        ),
        Err(VerificationError::ProofFromFuture { .. })
    ));
    assert!(matches!(
        Groth16VerificationRequest::new(
            fixture_statement(&data),
            Groth16Proof::from_bytes(hex::decode(&data.proof).unwrap()).unwrap(),
            data.block_context.max_valid_height.unwrap() + 1,
        ),
        Err(VerificationError::ProofExpired { .. })
    ));

    let mut raw_witness_envelope = fixture_envelope(&data);
    raw_witness_envelope["witness"] = json!(["not-accepted-at-runtime"]);
    assert!(matches!(
        parse_bitvm_groth16_envelope(raw_witness_envelope, data.current_block_height),
        Err(VerificationError::RawWitnessProvided)
    ));
}
