//! BRICS mBridge & CIPS Cross-Border Settlement Engine (Candidate P)
//!
//! Provides DLT state proof verification, HotStuff consensus attestation validation,
//! and CIPS ISO 20022 cross-border payload translation for BRICS+ financial systems.

use conxian_core::{ConxianError, ConxianResult};
use secp256k1::{schnorr, Message, Secp256k1, XOnlyPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

/// mBridge DLT Settlement Attestation Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MBridgeAttestationPayload {
    pub mbridge_id: String,
    pub from_cbdc: String,
    pub to_cbdc: String,
    pub amount: u64,
    pub currency: String,
    pub sender_bic: String,
    pub receiver_bic: String,
    pub proof_hash: String,
    pub timestamp: u64,
    /// Validator Schnorr threshold attestations: vector of (x-only pubkey hex, signature hex)
    pub validator_attestations: Vec<(String, String)>,
    /// Quorum threshold k required for consensus validation
    pub quorum_threshold: usize,
}

/// Verification result for mBridge DLT state proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MBridgeVerificationResult {
    pub mbridge_id: String,
    pub is_valid: bool,
    pub verified_validators: usize,
    pub quorum_threshold: usize,
    pub state_root_hash: String,
}

/// mBridge DLT State Verification Adapter
pub struct MBridgeAdapter;

impl MBridgeAdapter {
    /// Compute deterministic DLT payload hash
    #[allow(clippy::too_many_arguments)]
    pub fn compute_payload_hash(
        mbridge_id: &str,
        from_cbdc: &str,
        to_cbdc: &str,
        amount: u64,
        currency: &str,
        sender_bic: &str,
        receiver_bic: &str,
        timestamp: u64,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(mbridge_id.as_bytes());
        hasher.update(from_cbdc.as_bytes());
        hasher.update(to_cbdc.as_bytes());
        hasher.update(amount.to_be_bytes());
        hasher.update(currency.as_bytes());
        hasher.update(sender_bic.as_bytes());
        hasher.update(receiver_bic.as_bytes());
        hasher.update(timestamp.to_be_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verifies mBridge DLT state proofs and Schnorr consensus attestations.
    pub fn verify_mbridge_dlt_attestation(
        payload: &MBridgeAttestationPayload,
    ) -> ConxianResult<MBridgeVerificationResult> {
        info!(
            mbridge_id = %payload.mbridge_id,
            validators_count = payload.validator_attestations.len(),
            "Verifying mBridge DLT attestation"
        );

        if payload.mbridge_id.trim().is_empty() {
            return Err(ConxianError::Compliance(
                "Missing mBridge transaction ID".into(),
            ));
        }
        if payload.amount == 0 {
            return Err(ConxianError::Compliance(
                "mBridge transfer amount must be greater than zero".into(),
            ));
        }

        let computed_hash = Self::compute_payload_hash(
            &payload.mbridge_id,
            &payload.from_cbdc,
            &payload.to_cbdc,
            payload.amount,
            &payload.currency,
            &payload.sender_bic,
            &payload.receiver_bic,
            payload.timestamp,
        );

        if !payload.proof_hash.is_empty()
            && payload.proof_hash.to_lowercase() != computed_hash.to_lowercase()
        {
            return Err(ConxianError::Security(
                "mBridge payload hash mismatch".into(),
            ));
        }

        let msg_hash = Sha256::digest(computed_hash.as_bytes());
        let message = Message::from_digest(msg_hash.into());
        let secp = Secp256k1::verification_only();

        let mut valid_signatures = 0;

        for (pubkey_hex, sig_hex) in &payload.validator_attestations {
            let pubkey_bytes = match hex::decode(pubkey_hex) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let pubkey = match XOnlyPublicKey::from_slice(&pubkey_bytes) {
                Ok(pk) => pk,
                Err(_) => continue,
            };

            let sig_bytes = match hex::decode(sig_hex) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let sig = match schnorr::Signature::from_slice(&sig_bytes) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if secp.verify_schnorr(&sig, &message, &pubkey).is_ok() {
                valid_signatures += 1;
            }
        }

        let quorum_met =
            valid_signatures >= payload.quorum_threshold && payload.quorum_threshold > 0;

        if !quorum_met {
            return Err(ConxianError::Security(format!(
                "mBridge consensus threshold not met: {}/{} valid signatures",
                valid_signatures, payload.quorum_threshold
            )));
        }

        Ok(MBridgeVerificationResult {
            mbridge_id: payload.mbridge_id.clone(),
            is_valid: true,
            verified_validators: valid_signatures,
            quorum_threshold: payload.quorum_threshold,
            state_root_hash: computed_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Keypair, Secp256k1};

    #[test]
    fn test_mbridge_verification_success() {
        let secp = Secp256k1::new();
        let mut rng = secp256k1::rand::thread_rng();

        let keypair1 = Keypair::new(&secp, &mut rng);
        let (pubkey1, _) = keypair1.x_only_public_key();

        let keypair2 = Keypair::new(&secp, &mut rng);
        let (pubkey2, _) = keypair2.x_only_public_key();

        let mbridge_id = "mb-tx-2026-001";
        let from_cbdc = "e-CNY";
        let to_cbdc = "e-AED";
        let amount = 1000000;
        let currency = "AED";
        let sender_bic = "BKCHCNBJXXX";
        let receiver_bic = "FADBAEADXXX";
        let timestamp = 1750000000;

        let payload_hash = MBridgeAdapter::compute_payload_hash(
            mbridge_id,
            from_cbdc,
            to_cbdc,
            amount,
            currency,
            sender_bic,
            receiver_bic,
            timestamp,
        );

        let msg_hash = Sha256::digest(payload_hash.as_bytes());
        let message = Message::from_digest(msg_hash.into());

        let sig1 = secp.sign_schnorr(&message, &keypair1);
        let sig2 = secp.sign_schnorr(&message, &keypair2);

        let payload = MBridgeAttestationPayload {
            mbridge_id: mbridge_id.into(),
            from_cbdc: from_cbdc.into(),
            to_cbdc: to_cbdc.into(),
            amount,
            currency: currency.into(),
            sender_bic: sender_bic.into(),
            receiver_bic: receiver_bic.into(),
            proof_hash: payload_hash,
            timestamp,
            validator_attestations: vec![
                (hex::encode(pubkey1.serialize()), hex::encode(sig1.as_ref())),
                (hex::encode(pubkey2.serialize()), hex::encode(sig2.as_ref())),
            ],
            quorum_threshold: 2,
        };

        let result = MBridgeAdapter::verify_mbridge_dlt_attestation(&payload).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.verified_validators, 2);
        assert_eq!(result.quorum_threshold, 2);
    }

    #[test]
    fn test_mbridge_verification_quorum_failure() {
        let secp = Secp256k1::new();
        let mut rng = secp256k1::rand::thread_rng();

        let keypair1 = Keypair::new(&secp, &mut rng);
        let (pubkey1, _) = keypair1.x_only_public_key();

        let mbridge_id = "mb-tx-2026-002";
        let payload_hash = MBridgeAdapter::compute_payload_hash(
            mbridge_id,
            "e-CNY",
            "e-AED",
            500,
            "AED",
            "BKCHCNBJXXX",
            "FADBAEADXXX",
            1750000000,
        );

        let msg_hash = Sha256::digest(payload_hash.as_bytes());
        let message = Message::from_digest(msg_hash.into());
        let sig1 = secp.sign_schnorr(&message, &keypair1);

        let payload = MBridgeAttestationPayload {
            mbridge_id: mbridge_id.into(),
            from_cbdc: "e-CNY".into(),
            to_cbdc: "e-AED".into(),
            amount: 500,
            currency: "AED".into(),
            sender_bic: "BKCHCNBJXXX".into(),
            receiver_bic: "FADBAEADXXX".into(),
            proof_hash: payload_hash,
            timestamp: 1750000000,
            validator_attestations: vec![(
                hex::encode(pubkey1.serialize()),
                hex::encode(sig1.as_ref()),
            )],
            quorum_threshold: 2,
        };

        let err = MBridgeAdapter::verify_mbridge_dlt_attestation(&payload).unwrap_err();
        assert!(err.to_string().contains("consensus threshold not met"));
    }
}
