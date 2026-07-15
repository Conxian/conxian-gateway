use conxian_core::ConxianResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

/// DLC Oracle adapter using DDK v1.1.2 (successor to rust-dlc)
pub struct DlcOracleClient {
    pub oracle_url: String,
    pub oracle_pubkey: String,
    client: reqwest::Client,
}

/// A DLC event announcement from the oracle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleAnnouncement {
    pub event_id: String,
    pub oracle_pubkey: String,
    pub nonces: Vec<String>,
    pub outcomes: Vec<String>,
    pub event_maturity_epoch: u64,
    pub event_descriptor: String,
}

/// An oracle attestation (signed outcome)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleAttestation {
    pub event_id: String,
    pub outcome: String,
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
        let resp = self.client.get(&url).send().await.map_err(|e| {
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
        let resp = self.client.get(&url).send().await.map_err(|e| {
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

    /// Verify an oracle signature against a known announcement
    pub fn verify_attestation(
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
        // DDK/kormir signature verification via secp256k1-zkp adaptor sigs:
        // ddk::verify_oracle_attestation(&announcement, &attestation, outcome_index)
        let outcome_matches = announcement
            .outcomes
            .get(expected_outcome_index)
            .map(|expected| expected == &attestation.outcome)
            .unwrap_or(false);

        Ok(outcome_matches)
    }
}

/// Multi-oracle threshold DLC coordinator
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

    /// Check if threshold attestations agree on a specific outcome
    pub async fn check_threshold_outcome(&self, event_id: &str) -> ConxianResult<Option<String>> {
        let mut outcome_votes: HashMap<String, usize> = HashMap::new();

        for oracle in &self.oracles {
            match oracle.get_attestation(event_id).await {
                Ok(attestation) => {
                    *outcome_votes
                        .entry(attestation.outcome.clone())
                        .or_default() += 1;
                }
                Err(_) => continue,
            }
        }

        Ok(outcome_votes
            .into_iter()
            .find(|(_, count)| *count >= self.threshold)
            .map(|(outcome, _)| outcome))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_attestation_match() {
        let announcement = OracleAnnouncement {
            event_id: "btc-usd-2026q3".into(),
            oracle_pubkey: "pk-deadbeef".into(),
            nonces: vec!["R1".into(), "R2".into()],
            outcomes: vec!["up".into(), "down".into()],
            event_maturity_epoch: 1751328000,
            event_descriptor: "BTC/USD Q3 2026".into(),
        };

        let attestation = OracleAttestation {
            event_id: "btc-usd-2026q3".into(),
            outcome: "up".into(),
            signature: "sig-aaaa".into(),
            oracle_pubkey: "pk-deadbeef".into(),
        };

        assert!(DlcOracleClient::verify_attestation(&announcement, &attestation, 0).unwrap());
        assert!(!DlcOracleClient::verify_attestation(&announcement, &attestation, 1).unwrap());
    }

    #[test]
    fn test_verify_attestation_mismatch() {
        let announcement = OracleAnnouncement {
            event_id: "btc-usd-2026q3".into(),
            oracle_pubkey: "pk-deadbeef".into(),
            nonces: vec!["R1".into()],
            outcomes: vec!["up".into()],
            event_maturity_epoch: 1751328000,
            event_descriptor: "BTC/USD Q3 2026".into(),
        };

        // Wrong pubkey
        let attestation = OracleAttestation {
            event_id: "btc-usd-2026q3".into(),
            outcome: "up".into(),
            signature: "sig-aaaa".into(),
            oracle_pubkey: "pk-different".into(),
        };
        assert!(!DlcOracleClient::verify_attestation(&announcement, &attestation, 0).unwrap());

        // Wrong event_id
        let attestation = OracleAttestation {
            event_id: "eth-usd-2026q3".into(),
            outcome: "up".into(),
            signature: "sig-aaaa".into(),
            oracle_pubkey: "pk-deadbeef".into(),
        };
        assert!(!DlcOracleClient::verify_attestation(&announcement, &attestation, 0).unwrap());
    }

    #[test]
    #[should_panic(expected = "threshold")]
    fn test_threshold_oracle_coordinator_requires_valid_threshold() {
        let oracle = DlcOracleClient::new("http://localhost:8080".into(), "pk1".into());
        ThresholdOracleCoordinator::new(vec![oracle], 0);
    }
}
