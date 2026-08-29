use conxian_core::ConxianResult;
use secp256k1::{schnorr, Secp256k1, VerifyOnly, XOnlyPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::{info, warn};

/// HTTP oracle scaffold for DLC-shaped event and attestation payloads.
///
/// Cryptographic Schnorr (BIP340) verification is active via
/// `verify_schnorr_attestation` and multi-oracle threshold quorum
/// coordination (`check_threshold_outcome`).
pub struct DlcOracleClient {
    pub oracle_url: String,
    pub oracle_pubkey: String,
    client: reqwest::Client,
}

/// A DLC event announcement with BIP340 nonces for Schnorr adaptor construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleAnnouncement {
    pub event_id: String,
    pub oracle_pubkey: String,
    /// BIP340 nonce points (R values) for each outcome — used for adaptor sig
    /// construction in Stage 4. Basic Schnorr verification (Stage 2) does not
    /// consume these; the full signature already embeds R || s.
    pub nonces: Vec<String>,
    pub outcomes: Vec<String>,
    pub event_maturity_epoch: u64,
    pub event_descriptor: String,
}

/// An oracle attestation carrying a 64-byte hex-encoded BIP340 Schnorr
/// signature over `SHA256(event_id || outcome)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleAttestation {
    pub event_id: String,
    pub outcome: String,
    /// 64-byte hex BIP340 Schnorr signature (R || s).
    pub signature: String,
    pub oracle_pubkey: String,
}

impl DlcOracleClient {
    pub fn new(oracle_url: String, oracle_pubkey: String) -> Self {
        Self {
            oracle_url,
            oracle_pubkey,
            client: reqwest::Client::new(),
        }
    }

    /// Fetch all active event announcements from the oracle
    pub async fn list_announcements(&self) -> ConxianResult<Vec<OracleAnnouncement>> {
        let url = format!("{}/v1/announcements", self.oracle_url);
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                conxian_core::ConxianError::Internal(format!("Oracle HTTP error: {e}"))
            })?;

        let announcements: Vec<OracleAnnouncement> = resp.json().await.map_err(|e| {
            conxian_core::ConxianError::Internal(format!("Oracle parse error: {e}"))
        })?;

        info!(count = announcements.len(), "DLC announcements fetched");
        Ok(announcements)
    }

    /// Get attestation for a specific event
    pub async fn get_attestation(&self, event_id: &str) -> ConxianResult<OracleAttestation> {
        let url = format!("{}/v1/attestation/{}", self.oracle_url, event_id);
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                conxian_core::ConxianError::Internal(format!("Oracle HTTP error: {e}"))
            })?;

        if resp.status().is_client_error() {
            return Err(conxian_core::ConxianError::Internal(format!(
                "Oracle event {event_id} not attested yet"
            )));
        }

        let attestation: OracleAttestation = resp.json().await.map_err(|e| {
            conxian_core::ConxianError::Internal(format!("Oracle parse error: {e}"))
        })?;

        info!(
            event_id = %event_id,
            outcome = %attestation.outcome,
            "DLC attestation received"
        );
        Ok(attestation)
    }

    /// Verify payload consistency AND BIP340 Schnorr cryptographic attestation.
    ///
    /// Returns `Ok(true)` only when all of:
    /// 1. event_id matches between announcement and attestation,
    /// 2. oracle_pubkey matches,
    /// 3. expected_outcome_index points to the attested outcome,
    /// 4. the 64-byte hex Schnorr signature verifies over
    ///    `SHA256(event_id || outcome)` against the oracle's x-only pubkey.
    ///
    /// Returns `Ok(false)` for any consistency or cryptographic mismatch.
    /// Returns `Err` for hex-decoding or secp256k1 internal failures.
    pub fn verify_attestation(
        secp: &Secp256k1<VerifyOnly>,
        announcement: &OracleAnnouncement,
        attestation: &OracleAttestation,
        expected_outcome_index: usize,
    ) -> ConxianResult<bool> {
        if announcement.event_id != attestation.event_id {
            return Ok(false);
        }
        if announcement.oracle_pubkey != attestation.oracle_pubkey {
            return Ok(false);
        }
        let outcome_matches = announcement
            .outcomes
            .get(expected_outcome_index)
            .map(|expected| expected == &attestation.outcome)
            .unwrap_or(false);
        if !outcome_matches {
            return Ok(false);
        }
        Self::verify_schnorr_attestation(secp, announcement, attestation)
    }

    /// BIP340 Schnorr signature verification for an oracle attestation.
    ///
    /// The message is `SHA256(event_id || outcome)`. The signature is a
    /// 64-byte hex-encoded BIP340 Schnorr sig. The pubkey is a 32-byte
    /// hex-encoded x-only public key.
    ///
    /// Both the announcement and attestation must agree on `event_id` and
    /// `oracle_pubkey` before cryptographic verification is attempted.
    pub fn verify_schnorr_attestation(
        secp: &Secp256k1<VerifyOnly>,
        announcement: &OracleAnnouncement,
        attestation: &OracleAttestation,
    ) -> ConxianResult<bool> {
        if announcement.event_id != attestation.event_id {
            return Ok(false);
        }
        if announcement.oracle_pubkey != attestation.oracle_pubkey {
            return Ok(false);
        }

        let pubkey_bytes = hex::decode(&announcement.oracle_pubkey).map_err(|e| {
            conxian_core::ConxianError::Internal(format!("DLC oracle pubkey hex: {e}"))
        })?;
        let pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes).map_err(|e| {
            conxian_core::ConxianError::Internal(format!("DLC oracle pubkey invalid: {e}"))
        })?;
        let sig_bytes = hex::decode(&attestation.signature).map_err(|e| {
            conxian_core::ConxianError::Internal(format!("DLC attestation sig hex: {e}"))
        })?;
        let sig = schnorr::Signature::from_slice(&sig_bytes).map_err(|e| {
            conxian_core::ConxianError::Internal(format!("DLC attestation sig invalid: {e}"))
        })?;

        let mut hasher = Sha256::new();
        hasher.update(attestation.event_id.as_bytes());
        hasher.update(attestation.outcome.as_bytes());
        let msg_hash: [u8; 32] = hasher.finalize().into();
        let msg = secp256k1::Message::from_digest(msg_hash);

        Ok(secp.verify_schnorr(&sig, &msg, &pubkey).is_ok())
    }
}

/// Multi-oracle outcome-agreement scaffold.
pub struct ThresholdOracleCoordinator {
    pub oracles: Vec<DlcOracleClient>,
    pub threshold: usize,
}

impl ThresholdOracleCoordinator {
    pub fn new(oracles: Vec<DlcOracleClient>, threshold: usize) -> Self {
        assert!(threshold <= oracles.len() && threshold > 0);
        Self { oracles, threshold }
    }

    /// Fetch announcements from all oracles and deduplicate by event_id
    pub async fn collect_announcements(
        &self,
    ) -> ConxianResult<HashMap<String, Vec<OracleAnnouncement>>> {
        let mut events: HashMap<String, Vec<OracleAnnouncement>> = HashMap::new();
        for oracle in &self.oracles {
            match oracle.list_announcements().await {
                Ok(announcements) => {
                    for ann in announcements {
                        events.entry(ann.event_id.clone()).or_default().push(ann);
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Oracle announcement fetch failed");
                }
            }
        }
        info!(
            event_count = events.len(),
            "Multi-oracle announcements collected"
        );
        Ok(events)
    }

    /// Check whether fetched attestation payloads agree on an outcome AND pass
    /// cryptographic BIP340 Schnorr signature verification.
    ///
    /// Only attestations whose 64-byte Schnorr signature verifies over
    /// `SHA256(event_id || outcome)` against the oracle's x-only pubkey are
    /// counted toward the quorum threshold `k`.
    pub async fn check_threshold_outcome(
        &self,
        secp: &Secp256k1<VerifyOnly>,
        event_id: &str,
    ) -> ConxianResult<Option<String>> {
        let mut outcome_votes: HashMap<String, usize> = HashMap::new();

        for oracle in &self.oracles {
            match oracle.get_attestation(event_id).await {
                Ok(attestation) => {
                    let dummy_ann = OracleAnnouncement {
                        event_id: event_id.to_string(),
                        oracle_pubkey: oracle.oracle_pubkey.clone(),
                        nonces: vec![],
                        outcomes: vec![attestation.outcome.clone()],
                        event_maturity_epoch: 0,
                        event_descriptor: String::new(),
                    };

                    match DlcOracleClient::verify_schnorr_attestation(
                        secp,
                        &dummy_ann,
                        &attestation,
                    ) {
                        Ok(true) => {
                            *outcome_votes
                                .entry(attestation.outcome.clone())
                                .or_default() += 1;
                        }
                        Ok(false) => {
                            warn!(
                                oracle_pubkey = %oracle.oracle_pubkey,
                                event_id = %event_id,
                                "Multi-oracle attestation signature verification failed"
                            );
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                oracle_pubkey = %oracle.oracle_pubkey,
                                event_id = %event_id,
                                "Multi-oracle attestation signature error"
                            );
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(outcome_votes
            .into_iter()
            .find(|(_, count)| *count >= self.threshold)
            .map(|(outcome, _)| outcome))
    }

    /// Cryptographically verify a slice of collected attestations against a threshold.
    ///
    /// Returns `true` if at least `self.threshold` attestations carry valid Schnorr
    /// signatures agreeing on `expected_outcome`.
    pub fn verify_threshold_attestations(
        &self,
        secp: &Secp256k1<VerifyOnly>,
        event_id: &str,
        expected_outcome: &str,
        attestations: &[OracleAttestation],
    ) -> ConxianResult<bool> {
        let mut valid_votes = 0;

        for attestation in attestations {
            if attestation.event_id != event_id || attestation.outcome != expected_outcome {
                continue;
            }

            let dummy_ann = OracleAnnouncement {
                event_id: event_id.to_string(),
                oracle_pubkey: attestation.oracle_pubkey.clone(),
                nonces: vec![],
                outcomes: vec![expected_outcome.to_string()],
                event_maturity_epoch: 0,
                event_descriptor: String::new(),
            };

            if let Ok(true) =
                DlcOracleClient::verify_schnorr_attestation(secp, &dummy_ann, attestation)
            {
                valid_votes += 1;
            }
        }

        Ok(valid_votes >= self.threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{rand, Keypair};

    fn test_secp() -> Secp256k1<VerifyOnly> {
        Secp256k1::verification_only()
    }

    fn signing_secp() -> Secp256k1<secp256k1::All> {
        Secp256k1::new()
    }

    /// Build a valid BIP340-signed attestation for test inputs.
    fn sign_attestation(event_id: &str, outcome: &str, kp: &Keypair) -> OracleAttestation {
        let mut hasher = Sha256::new();
        hasher.update(event_id.as_bytes());
        hasher.update(outcome.as_bytes());
        let msg_hash: [u8; 32] = hasher.finalize().into();
        let msg = secp256k1::Message::from_digest(msg_hash);
        let sig = signing_secp().sign_schnorr(&msg, kp);
        OracleAttestation {
            event_id: event_id.into(),
            outcome: outcome.into(),
            signature: hex::encode(sig.serialize()),
            oracle_pubkey: hex::encode(kp.x_only_public_key().0.serialize()),
        }
    }

    fn announcement_for(
        event_id: &str,
        pubkey_hex: &str,
        outcomes: Vec<&str>,
    ) -> OracleAnnouncement {
        OracleAnnouncement {
            event_id: event_id.into(),
            oracle_pubkey: pubkey_hex.into(),
            nonces: vec!["aa".repeat(32)],
            outcomes: outcomes.into_iter().map(|s| s.to_string()).collect(),
            event_maturity_epoch: 1751328000,
            event_descriptor: "BTC/USD Q3 2026".into(),
        }
    }

    #[test]
    fn schnorr_attestation_verifies_valid_signature() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();
        let (sk, _) = ssecp.generate_keypair(&mut rng);
        let kp = Keypair::from_secret_key(&ssecp, &sk);
        let pubkey_hex = hex::encode(kp.x_only_public_key().0.serialize());

        let ann = announcement_for("btc-usd-2026q3", &pubkey_hex, vec!["up", "down"]);
        let att = sign_attestation("btc-usd-2026q3", "up", &kp);

        assert!(DlcOracleClient::verify_schnorr_attestation(&secp, &ann, &att).unwrap());
    }

    #[test]
    fn schnorr_rejects_wrong_outcome() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();
        let (sk, _) = ssecp.generate_keypair(&mut rng);
        let kp = Keypair::from_secret_key(&ssecp, &sk);
        let pubkey_hex = hex::encode(kp.x_only_public_key().0.serialize());

        // Sign for "up", but attestation claims "down" — sig over different message
        let ann = announcement_for("btc-usd-2026q3", &pubkey_hex, vec!["up", "down"]);
        let decoy_att = OracleAttestation {
            event_id: "btc-usd-2026q3".into(),
            outcome: "down".into(),
            signature: hex::encode(
                signing_secp()
                    .sign_schnorr(
                        &secp256k1::Message::from_digest({
                            let mut h = Sha256::new();
                            h.update(b"btc-usd-2026q3");
                            h.update(b"up"); // signed "up", but outcome claims "down"
                            h.finalize().into()
                        }),
                        &kp,
                    )
                    .serialize(),
            ),
            oracle_pubkey: pubkey_hex.clone(),
        };

        assert!(!DlcOracleClient::verify_schnorr_attestation(&secp, &ann, &decoy_att).unwrap());
    }

    #[test]
    fn schnorr_rejects_wrong_pubkey() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();
        let (sk, _) = ssecp.generate_keypair(&mut rng);
        let kp = Keypair::from_secret_key(&ssecp, &sk);
        let pubkey_hex = hex::encode(kp.x_only_public_key().0.serialize());

        // Sign with wrong_keypair, claim pubkey_hex
        let (wrong_sk, _) = ssecp.generate_keypair(&mut rng);
        let wrong_kp = Keypair::from_secret_key(&ssecp, &wrong_sk);
        let ann = announcement_for("btc-usd-2026q3", &pubkey_hex, vec!["up"]);
        let att = sign_attestation("btc-usd-2026q3", "up", &wrong_kp);

        assert!(!DlcOracleClient::verify_schnorr_attestation(&secp, &ann, &att).unwrap());
    }

    #[test]
    fn schnorr_rejects_wrong_event_id() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();
        let (sk, _) = ssecp.generate_keypair(&mut rng);
        let kp = Keypair::from_secret_key(&ssecp, &sk);
        let pubkey_hex = hex::encode(kp.x_only_public_key().0.serialize());

        // Announcement for btc, attestation for eth — diff message hash
        let ann = announcement_for("btc-usd-2026q3", &pubkey_hex, vec!["up"]);
        let att = sign_attestation("eth-usd-2026q3", "up", &kp);

        assert!(!DlcOracleClient::verify_schnorr_attestation(&secp, &ann, &att).unwrap());
    }

    #[test]
    fn full_verify_attestation_with_schnorr() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();
        let (sk, _) = ssecp.generate_keypair(&mut rng);
        let kp = Keypair::from_secret_key(&ssecp, &sk);
        let pubkey_hex = hex::encode(kp.x_only_public_key().0.serialize());

        let ann = announcement_for("btc-usd-2026q3", &pubkey_hex, vec!["up", "down"]);
        let att = sign_attestation("btc-usd-2026q3", "up", &kp);

        assert!(DlcOracleClient::verify_attestation(&secp, &ann, &att, 0).unwrap());
        assert!(!DlcOracleClient::verify_attestation(&secp, &ann, &att, 1).unwrap());
    }

    #[test]
    fn full_verify_rejects_mismatched_pubkey() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();
        let (sk, _) = ssecp.generate_keypair(&mut rng);
        let kp = Keypair::from_secret_key(&ssecp, &sk);
        let pubkey_hex = hex::encode(kp.x_only_public_key().0.serialize());

        let ann = announcement_for("btc-usd-2026q3", &pubkey_hex, vec!["up"]);
        let att = sign_attestation("btc-usd-2026q3", "up", &kp);

        let mut wrong_ann = ann.clone();
        wrong_ann.oracle_pubkey = "ff".repeat(32); // wrong pubkey
        assert!(!DlcOracleClient::verify_attestation(&secp, &wrong_ann, &att, 0).unwrap());
    }

    #[test]
    fn full_verify_rejects_wrong_event_id() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();
        let (sk, _) = ssecp.generate_keypair(&mut rng);
        let kp = Keypair::from_secret_key(&ssecp, &sk);
        let pubkey_hex = hex::encode(kp.x_only_public_key().0.serialize());

        let ann = announcement_for("btc-usd-2026q3", &pubkey_hex, vec!["up"]);
        let att = sign_attestation("btc-usd-2026q3", "up", &kp);

        let mut wrong_att = att.clone();
        wrong_att.event_id = "eth-usd-2026q3".into();
        assert!(!DlcOracleClient::verify_attestation(&secp, &ann, &wrong_att, 0).unwrap());
    }

    #[test]
    fn verify_attestation_errs_on_bad_hex_pubkey() {
        let secp = test_secp();
        let ann = announcement_for("btc-usd-2026q3", "not-hex", vec!["up"]);
        let att = OracleAttestation {
            event_id: "btc-usd-2026q3".into(),
            outcome: "up".into(),
            signature: "ff".repeat(64),
            oracle_pubkey: "not-hex".into(),
        };
        assert!(DlcOracleClient::verify_attestation(&secp, &ann, &att, 0).is_err());
    }

    #[test]
    #[should_panic(expected = "threshold")]
    fn test_threshold_oracle_coordinator_requires_valid_threshold() {
        let oracle = DlcOracleClient::new("http://localhost:8080".into(), "pk1".into());
        ThresholdOracleCoordinator::new(vec![oracle], 0);
    }

    #[test]
    fn threshold_oracle_coordinator_verifies_valid_multisig_attestations() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();

        let (sk1, _) = ssecp.generate_keypair(&mut rng);
        let kp1 = Keypair::from_secret_key(&ssecp, &sk1);
        let pk1_hex = hex::encode(kp1.x_only_public_key().0.serialize());

        let (sk2, _) = ssecp.generate_keypair(&mut rng);
        let kp2 = Keypair::from_secret_key(&ssecp, &sk2);
        let pk2_hex = hex::encode(kp2.x_only_public_key().0.serialize());

        let (sk3, _) = ssecp.generate_keypair(&mut rng);
        let kp3 = Keypair::from_secret_key(&ssecp, &sk3);
        let pk3_hex = hex::encode(kp3.x_only_public_key().0.serialize());

        let o1 = DlcOracleClient::new("http://o1".into(), pk1_hex.clone());
        let o2 = DlcOracleClient::new("http://o2".into(), pk2_hex.clone());
        let o3 = DlcOracleClient::new("http://o3".into(), pk3_hex.clone());

        let coordinator = ThresholdOracleCoordinator::new(vec![o1, o2, o3], 2);

        let att1 = sign_attestation("btc-usd-2026q3", "up", &kp1);
        let att2 = sign_attestation("btc-usd-2026q3", "up", &kp2);

        assert!(coordinator
            .verify_threshold_attestations(
                &secp,
                "btc-usd-2026q3",
                "up",
                &[att1.clone(), att2.clone()]
            )
            .unwrap());
    }

    #[test]
    fn threshold_oracle_coordinator_rejects_forged_attestations() {
        let secp = test_secp();
        let ssecp = signing_secp();
        let mut rng = rand::thread_rng();

        let (sk1, _) = ssecp.generate_keypair(&mut rng);
        let kp1 = Keypair::from_secret_key(&ssecp, &sk1);
        let pk1_hex = hex::encode(kp1.x_only_public_key().0.serialize());

        let (sk2, _) = ssecp.generate_keypair(&mut rng);
        let kp2 = Keypair::from_secret_key(&ssecp, &sk2);
        let pk2_hex = hex::encode(kp2.x_only_public_key().0.serialize());

        let o1 = DlcOracleClient::new("http://o1".into(), pk1_hex.clone());
        let o2 = DlcOracleClient::new("http://o2".into(), pk2_hex.clone());

        let coordinator = ThresholdOracleCoordinator::new(vec![o1, o2], 2);

        let att1 = sign_attestation("btc-usd-2026q3", "up", &kp1);

        let (rogue_sk, _) = ssecp.generate_keypair(&mut rng);
        let rogue_kp = Keypair::from_secret_key(&ssecp, &rogue_sk);
        let mut forged_att2 = sign_attestation("btc-usd-2026q3", "up", &rogue_kp);
        forged_att2.oracle_pubkey = pk2_hex;

        assert!(!coordinator
            .verify_threshold_attestations(&secp, "btc-usd-2026q3", "up", &[att1, forged_att2])
            .unwrap());
    }
}
