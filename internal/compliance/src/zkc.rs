use bitcoin::hashes::{sha256, Hash};
pub use conxian_core::{Attestation, ConxianError, ConxianResult, SchnorrAttestation};
use secp256k1::schnorr::Signature as SchnorrSignature;
use secp256k1::XOnlyPublicKey;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};

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
            Err(e) => Err(ConxianError::Compliance(format!(
                "Signature verification failed: {}",
                e
            ))),
        }
    }

    /// Research enhancement: Verify Schnorr signature for Taproot-compatible attestations.
    pub fn verify_schnorr(&self, attestation: &SchnorrAttestation) -> ConxianResult<bool> {
        // Parse X-only public key
        let pubkey_bytes = hex::decode(&attestation.x_only_public_key).map_err(|e| {
            ConxianError::Compliance(format!("Invalid x-only public key hex: {}", e))
        })?;
        let pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| ConxianError::Compliance(format!("Invalid x-only public key: {}", e)))?;

        // Parse Schnorr signature
        let sig_bytes = hex::decode(&attestation.signature).map_err(|e| {
            ConxianError::Compliance(format!("Invalid Schnorr signature hex: {}", e))
        })?;
        let sig = SchnorrSignature::from_slice(&sig_bytes)
            .map_err(|e| ConxianError::Compliance(format!("Invalid Schnorr signature: {}", e)))?;

        // Hash the payload
        let message_hash = sha256::Hash::hash(attestation.payload.as_bytes());
        let message = Message::from_digest(message_hash.to_byte_array());

        // Verify signature
        match self.secp.verify_schnorr(&sig, &message, &pubkey) {
            Ok(_) => Ok(true),
            Err(e) => Err(ConxianError::Compliance(format!(
                "Schnorr signature verification failed: {}",
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::thread_rng;
    use secp256k1::Keypair;

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
    fn test_zkc_verify_schnorr_valid() {
        let secp = Secp256k1::new();
        let mut rng = thread_rng();
        let kp = Keypair::new(&secp, &mut rng);
        let (pk, _) = kp.x_only_public_key();

        let payload = "valid-schnorr-payload";
        let message_hash = sha256::Hash::hash(payload.as_bytes());
        let message = Message::from_digest(message_hash.to_byte_array());
        let sig = secp.sign_schnorr(&message, &kp);

        let verifier = ZkcVerifier::new();
        let attestation = SchnorrAttestation {
            device_id: "conxius-schnorr-123".to_string(),
            signature: hex::encode(sig.as_ref()),
            payload: payload.to_string(),
            x_only_public_key: hex::encode(pk.serialize()),
        };
        assert!(verifier.verify_schnorr(&attestation).unwrap());
    }
}
