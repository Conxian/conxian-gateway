use compliance::ZkcVerifier;
use conxian_core::{Attestation, AttestationRequest, ConxianResult, ZkmlProof};

fn assert_denied(res: ConxianResult<bool>) {
    assert!(
        matches!(res, Ok(false)),
        "expected settlement attestation denial as Ok(false), got: {res:?}"
    );
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

    let attestation = AttestationRequest::Ecdsa(Attestation {
        device_id: "conxius-tee-mock-123".to_string(),
        signature: "".to_string(),
        payload: "payload-hash".to_string(),
        public_key: "".to_string(),
    });

    let res = verifier.verify_settlement_trigger_attestation(&attestation, "payload-hash");
    assert_denied(res);
}
