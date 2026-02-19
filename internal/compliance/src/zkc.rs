use conxian_core::{ConxianError, ConxianResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Attestation {
    pub device_id: String,
    pub signature: String,
    pub payload: String,
}

pub struct ZkcVerifier;

impl Default for ZkcVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkcVerifier {
    pub fn new() -> Self {
        Self
    }

    pub fn verify(&self, attestation: &Attestation) -> ConxianResult<bool> {
        // Validation: device_id must follow the expected format
        if !attestation.device_id.starts_with("conxius-") {
            return Err(ConxianError::Compliance(
                "Invalid device ID: must start with 'conxius-'".to_string(),
            ));
        }

        // Validation: signature must not be empty
        if attestation.signature.is_empty() {
            return Err(ConxianError::Compliance(
                "Attestation signature cannot be empty".to_string(),
            ));
        }

        // Validation: payload must not be empty
        if attestation.payload.is_empty() {
            return Err(ConxianError::Compliance(
                "Attestation payload cannot be empty".to_string(),
            ));
        }

        // In a real implementation, this would verify the signature
        // against the Secure Enclave's public key (Conxius Wallet).
        // For now, we simulate success if all fields are present and validly formatted.
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zkc_verify_valid() {
        let verifier = ZkcVerifier::new();
        let attestation = Attestation {
            device_id: "conxius-123".to_string(),
            signature: "valid-signature".to_string(),
            payload: "valid-payload".to_string(),
        };
        assert!(verifier.verify(&attestation).unwrap());
    }

    #[test]
    fn test_zkc_verify_invalid_device() {
        let verifier = ZkcVerifier::new();
        let attestation = Attestation {
            device_id: "other-123".to_string(),
            signature: "sig".to_string(),
            payload: "payload".to_string(),
        };
        let result = verifier.verify(&attestation);
        assert!(result.is_err());
        match result {
            Err(ConxianError::Compliance(msg)) => assert!(msg.contains("Invalid device ID")),
            _ => panic!("Expected Compliance error"),
        }
    }

    #[test]
    fn test_zkc_verify_empty_signature() {
        let verifier = ZkcVerifier::new();
        let attestation = Attestation {
            device_id: "conxius-123".to_string(),
            signature: "".to_string(),
            payload: "payload".to_string(),
        };
        let result = verifier.verify(&attestation);
        assert!(result.is_err());
        match result {
            Err(ConxianError::Compliance(msg)) => assert!(msg.contains("signature cannot be empty")),
            _ => panic!("Expected Compliance error"),
        }
    }

    #[test]
    fn test_zkc_verify_empty_payload() {
        let verifier = ZkcVerifier::new();
        let attestation = Attestation {
            device_id: "conxius-123".to_string(),
            signature: "sig".to_string(),
            payload: "".to_string(),
        };
        let result = verifier.verify(&attestation);
        assert!(result.is_err());
        match result {
            Err(ConxianError::Compliance(msg)) => assert!(msg.contains("payload cannot be empty")),
            _ => panic!("Expected Compliance error"),
        }
    }
}
