use compliance::zkc::ATTESTATION_SIGNING_DOMAIN;
use compliance::ZkcVerifier;
use conxian_core::{Attestation, AttestationRequest, ConxianResult, ZkmlProof};
use secp256k1::{Message, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

fn assert_denied(res: ConxianResult<bool>) {
    assert!(
        matches!(res, Ok(false)),
        "expected settlement attestation denial as Ok(false), got: {res:?}"
    );
}

fn make_signed_attestation(device_id: &str, payload_hash: &str) -> AttestationRequest {
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);

    let mut hasher = Sha256::new();
    hasher.update(ATTESTATION_SIGNING_DOMAIN);
    hasher.update(device_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(payload_hash.as_bytes());
    let digest = hasher.finalize();

    let message = Message::from_digest_slice(&digest).unwrap();
    let signature = secp.sign_ecdsa(&message, &secret_key);
    let signature_der = signature.serialize_der();

    AttestationRequest::Ecdsa(Attestation {
        device_id: device_id.to_string(),
        signature: hex::encode(signature_der),
        payload: payload_hash.to_string(),
        public_key: hex::encode(public_key.serialize()),
    })
}

#[test]
fn settlement_attestation_rejects_non_tee_device() {
    let verifier = ZkcVerifier::new();

    let attestation = AttestationRequest::Ecdsa(Attestation {
        device_id: "conxius-mobile-123".to_string(),
        signature: "00".to_string(),
        payload: "payload-hash".to_string(),
        public_key: "00".to_string(),
    });

    let res = verifier.verify_settlement_trigger_attestation(&attestation, "payload-hash");
    assert_denied(res);
}

#[test]
fn settlement_attestation_rejects_unsupported_attestation_types() {
    let verifier = ZkcVerifier::new();

    let attestation = AttestationRequest::Zkml(ZkmlProof {
        device_id: "conxius-tee-123".to_string(),
        image_id: "".to_string(),
        receipt: "".to_string(),
        receipt_hash: "".to_string(),
        public_inputs: "".to_string(),
        journal: "".to_string(),
    });

    let res = verifier.verify_settlement_trigger_attestation(&attestation, "payload-hash");
    assert_denied(res);
}

#[test]
fn settlement_attestation_denies_on_verification_errors() {
    let verifier = ZkcVerifier::new();

    let attestation = AttestationRequest::Ecdsa(Attestation {
        device_id: "conxius-tee-123".to_string(),
        signature: "".to_string(),
        payload: "payload-hash".to_string(),
        public_key: "".to_string(),
    });

    let res = verifier.verify_settlement_trigger_attestation(&attestation, "payload-hash");
    assert_denied(res);
}

#[test]
fn settlement_attestation_rejects_mock_device_id() {
    let verifier = ZkcVerifier::new();

    let accepted = make_signed_attestation("conxius-tee-test-123", "payload-hash");
    let res = verifier.verify_settlement_trigger_attestation(&accepted, "payload-hash");
    assert!(matches!(res, Ok(true)), "expected Ok(true), got: {res:?}");

    let rejected = make_signed_attestation("conxius-tee-mock-123", "payload-hash");
    let res = verifier.verify_settlement_trigger_attestation(&rejected, "payload-hash");
    assert_denied(res);
}
