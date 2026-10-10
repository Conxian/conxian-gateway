//! PAPSS (Pan-African Payment and Settlement System) Settlement Adapter
//!
//! Afreximbank's pan-African cross-border settlement rail (AfCFTA). PAPSS
//! settles intra-African trade in local currencies (ZAR, NGN, KES, GHS, EGP,
//! …) over ISO 20022 messaging, reducing USD/EUR dependency for African
//! corridors. This adapter verifies PAPSS settlement attestations
//! (Afreximbank + participating central-bank confirmations) via a
//! deterministic payload hash + threshold Schnorr attestations, mirroring the
//! mBridge adapter's verification model.

use conxian_core::{ConxianError, ConxianResult};
use secp256k1::{schnorr, Message, Secp256k1, XOnlyPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

/// PAPSS settlement attestation payload (ISO 20022 + African local currency).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PapssAttestationPayload {
    /// PAPSS message identifier (ISO 20022 MsgId).
    #[serde(alias = "PAPSS_MsgId", alias = "MsgId")]
    pub papss_id: String,
    /// Sending participant (central bank / commercial bank identifier).
    #[serde(alias = "PAPSS_Sender")]
    pub sender: String,
    /// Receiving participant identifier.
    #[serde(alias = "PAPSS_Receiver")]
    pub receiver: String,
    /// Settlement amount in minor units (e.g. ZAR cents).
    #[serde(alias = "PAPSS_Amount", alias = "SettlementAmount")]
    pub amount: u64,
    /// Settlement currency (ZAR, NGN, KES, GHS, EGP, …).
    #[serde(alias = "PAPSS_Currency", alias = "SettlementCurrency")]
    pub currency: String,
    /// Deterministic proof hash of the settlement instruction.
    pub proof_hash: String,
    pub timestamp: u64,
    /// Afreximbank / central-bank threshold attestations:
    /// vector of (x-only pubkey hex, signature hex).
    pub settlement_attestations: Vec<(String, String)>,
    /// Quorum threshold `k` required for settlement finality.
    pub quorum_threshold: usize,
}

/// Verification result for a PAPSS settlement attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PapssVerificationResult {
    pub papss_id: String,
    pub is_valid: bool,
    pub verified_attestations: usize,
    pub quorum_threshold: usize,
    pub state_root_hash: String,
}

/// PAPSS Settlement Verification Adapter.
pub struct PapssAdapter;

impl PapssAdapter {
    /// Compute the deterministic settlement payload hash.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_payload_hash(
        papss_id: &str,
        sender: &str,
        receiver: &str,
        amount: u64,
        currency: &str,
        timestamp: u64,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(papss_id.as_bytes());
        hasher.update(sender.as_bytes());
        hasher.update(receiver.as_bytes());
        hasher.update(amount.to_be_bytes());
        hasher.update(currency.as_bytes());
        hasher.update(timestamp.to_be_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verifies a PAPSS settlement attestation (threshold central-bank signatures).
    pub fn verify_papss_settlement_attestation(
        payload: &PapssAttestationPayload,
    ) -> ConxianResult<PapssVerificationResult> {
        info!(
            papss_id = %payload.papss_id,
            attestations_count = payload.settlement_attestations.len(),
            "Verifying PAPSS settlement attestation"
        );

        if payload.papss_id.trim().is_empty() {
            return Err(ConxianError::Compliance("Missing PAPSS message ID".into()));
        }
        if payload.amount == 0 {
            return Err(ConxianError::Compliance(
                "PAPSS settlement amount must be greater than zero".into(),
            ));
        }

        let computed_hash = Self::compute_payload_hash(
            &payload.papss_id,
            &payload.sender,
            &payload.receiver,
            payload.amount,
            &payload.currency,
            payload.timestamp,
        );

        if !payload.proof_hash.is_empty()
            && payload.proof_hash.to_lowercase() != computed_hash.to_lowercase()
        {
            return Err(ConxianError::Security(
                "PAPSS payload hash mismatch".into(),
            ));
        }

        let msg_hash = Sha256::digest(computed_hash.as_bytes());
        let message = Message::from_digest(msg_hash.into());
        let secp = Secp256k1::verification_only();

        let mut valid_signatures = 0;

        for (pubkey_hex, sig_hex) in &payload.settlement_attestations {
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
                "PAPSS settlement quorum not met: {}/{} valid attestations",
                valid_signatures, payload.quorum_threshold
            )));
        }

        Ok(PapssVerificationResult {
            papss_id: payload.papss_id.clone(),
            is_valid: true,
            verified_attestations: valid_signatures,
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
    fn test_papss_verification_success() {
        let secp = Secp256k1::new();
        let mut rng = secp256k1::rand::thread_rng();

        let keypair1 = Keypair::new(&secp, &mut rng);
        let (pubkey1, _) = keypair1.x_only_public_key();
        let keypair2 = Keypair::new(&secp, &mut rng);
        let (pubkey2, _) = keypair2.x_only_public_key();

        let papss_id = "PAPSS-AFRICA-001";
        let sender = "NGBK001";
        let receiver = "GHBK002";
        let amount = 250000;
        let currency = "NGN";
        let timestamp = 1750000000;

        let payload_hash = PapssAdapter::compute_payload_hash(
            papss_id, sender, receiver, amount, currency, timestamp,
        );

        let msg_hash = Sha256::digest(payload_hash.as_bytes());
        let message = Message::from_digest(msg_hash.into());

        let sig1 = secp.sign_schnorr(&message, &keypair1);
        let sig2 = secp.sign_schnorr(&message, &keypair2);

        let payload = PapssAttestationPayload {
            papss_id: papss_id.into(),
            sender: sender.into(),
            receiver: receiver.into(),
            amount,
            currency: currency.into(),
            proof_hash: payload_hash,
            timestamp,
            settlement_attestations: vec![
                (hex::encode(pubkey1.serialize()), hex::encode(sig1.as_ref())),
                (hex::encode(pubkey2.serialize()), hex::encode(sig2.as_ref())),
            ],
            quorum_threshold: 2,
        };

        let result = PapssAdapter::verify_papss_settlement_attestation(&payload).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.verified_attestations, 2);
        assert_eq!(result.quorum_threshold, 2);
    }

    #[test]
    fn test_papss_verification_quorum_failure() {
        let secp = Secp256k1::new();
        let mut rng = secp256k1::rand::thread_rng();

        let keypair1 = Keypair::new(&secp, &mut rng);
        let (pubkey1, _) = keypair1.x_only_public_key();

        let papss_id = "PAPSS-AFRICA-002";
        let payload_hash = PapssAdapter::compute_payload_hash(
            papss_id, "NGBK001", "GHBK002", 500, "NGN", 1750000000,
        );

        let msg_hash = Sha256::digest(payload_hash.as_bytes());
        let message = Message::from_digest(msg_hash.into());
        let sig1 = secp.sign_schnorr(&message, &keypair1);

        let payload = PapssAttestationPayload {
            papss_id: papss_id.into(),
            sender: "NGBK001".into(),
            receiver: "GHBK002".into(),
            amount: 500,
            currency: "NGN".into(),
            proof_hash: payload_hash,
            timestamp: 1750000000,
            settlement_attestations: vec![(
                hex::encode(pubkey1.serialize()),
                hex::encode(sig1.as_ref()),
            )],
            quorum_threshold: 2,
        };

        let err = PapssAdapter::verify_papss_settlement_attestation(&payload).unwrap_err();
        assert!(err.to_string().contains("quorum not met"));
    }
}
