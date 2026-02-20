use conxian_core::{ConxianError, ConxianResult};
use serde::{Deserialize, Serialize};
use secp256k1::{Message, Secp256k1, PublicKey, ecdsa::Signature};
use bitcoin::hashes::{sha256, Hash};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Attestation {
    pub device_id: String,
    pub signature: String, // Hex encoded
    pub payload: String,
    pub public_key: String, // Hex encoded
}

pub struct ZkcVerifier {
    secp: Secp256k1<secp256k1::All>,
}

impl Default for ZkcVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkcVerifier {
    pub fn new() -> Self {
        Self {
            secp: Secp256k1::new(),
        }
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

        // Parse public key
        let pubkey_bytes = hex::decode(&attestation.public_key)
            .map_err(|e| ConxianError::Compliance(format!("Invalid public key hex: {}", e)))?;
        let pubkey = PublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| ConxianError::Compliance(format!("Invalid public key: {}", e)))?;

        // Parse signature
        let sig_bytes = hex::decode(&attestation.signature)
            .map_err(|e| ConxianError::Compliance(format!("Invalid signature hex: {}", e)))?;

        let sig = Signature::from_der(&sig_bytes)
            .or_else(|_| Signature::from_compact(&sig_bytes))
            .map_err(|e| ConxianError::Compliance(format!("Invalid signature format: {}", e)))?;

        // Hash the payload
        let message_hash = sha256::Hash::hash(attestation.payload.as_bytes());
        let message = Message::from_digest(message_hash.to_byte_array());

        // Verify signature
        match self.secp.verify_ecdsa(&message, &sig, &pubkey) {
            Ok(_) => Ok(true),
            Err(e) => Err(ConxianError::Compliance(format!("Signature verification failed: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::SecretKey;
    use rand::thread_rng;

    #[test]
    fn test_zkc_verify_valid() {
        let secp = Secp256k1::new();
        let (sk, pk) = secp.generate_keypair(&mut thread_rng());

        let payload = "valid-payload";
        let message_hash = sha256::Hash::hash(payload.as_bytes());
        let message = Message::from_digest(message_hash.to_byte_array());
        let sig = secp.sign_ecdsa(&message, &sk);

        let verifier = ZkcVerifier::new();
        let attestation = Attestation {
            device_id: "conxius-123".to_string(),
            signature: hex::encode(sig.serialize_der()),
            payload: payload.to_string(),
            public_key: hex::encode(pk.serialize()),
        };
        assert!(verifier.verify(&attestation).unwrap());
    }

    #[test]
    fn test_zkc_verify_invalid_signature() {
        let secp = Secp256k1::new();
        let (_sk, pk) = secp.generate_keypair(&mut thread_rng());
        let (sk2, _pk2) = secp.generate_keypair(&mut thread_rng());

        let payload = "valid-payload";
        let message_hash = sha256::Hash::hash(payload.as_bytes());
        let message = Message::from_digest(message_hash.to_byte_array());
        let sig = secp.sign_ecdsa(&message, &sk2); // Signed with wrong key

        let verifier = ZkcVerifier::new();
        let attestation = Attestation {
            device_id: "conxius-123".to_string(),
            signature: hex::encode(sig.serialize_der()),
            payload: payload.to_string(),
            public_key: hex::encode(pk.serialize()),
        };
        let result = verifier.verify(&attestation);
        assert!(result.is_err());
    }
}
