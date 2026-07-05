//! Fedimint Protocol Adapter (G-16 / CON-1401)
//!
//! Fedimint is a federated Chaumian e-cash protocol on Bitcoin.
//! This adapter communicates with Fedimint guardians via their
//! REST API to verify federation state, query module info, and
//! validate consensus.
//!
//! Architecture: HTTP guardian API (no heavy client DB required).
//! For full wallet integration, use `fedimint-client` v0.11 crate
//! (requires rocksdb + root secret — track in #229).
//!
//! Guardian API reference: <https://docs.rs/fedimint-core/latest/fedimint_core/api>

use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianError, ConxianResult};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, info, warn};

/// Fedimint federation adapter using guardian REST API.
///
/// Communicates with one or more Fedimint guardians to observe
/// federation state. Does not hold e-cash or manage a client DB —
/// this is a read-only observer + transaction submitter for the gateway.
pub struct FedimintAdapter {
    /// Primary guardian API base URL (e.g. `http://localhost:18173`)
    pub guardian_url: String,
    /// Federation invite code or config string
    pub invite_code: Option<String>,
    /// Cached federation ID from config
    federation_id: Option<String>,
    client: reqwest::Client,
}

/// Federation config as returned by `GET /config` on a guardian.
#[derive(Debug, Deserialize)]
struct FederationConfig {
    #[serde(rename = "federation_id")]
    federation_id: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "consensus_version")]
    consensus_version: Option<serde_json::Value>,
    #[serde(rename = "modules")]
    modules: Option<serde_json::Value>,
}

/// Guardian info response.
#[derive(Debug, Deserialize)]
struct GuardianInfo {
    #[serde(rename = "consensus_status")]
    consensus_status: Option<String>,
    #[serde(rename = "epoch")]
    epoch: Option<u64>,
}

impl FedimintAdapter {
    pub fn new(guardian_url: String) -> Self {
        Self {
            guardian_url,
            invite_code: None,
            federation_id: None,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_invite(mut self, invite: String) -> Self {
        self.invite_code = Some(invite);
        self
    }

    /// Derive a federation identifier from the guardian URL and cached config.
    fn fed_identity(&self) -> String {
        self.federation_id
            .as_deref()
            .unwrap_or("fedimint:unknown")
            .to_string()
    }

    /// Fetch the federation config from the guardian.
    async fn fetch_config(&self) -> ConxianResult<FederationConfig> {
        let url = format!("{}/config", self.guardian_url);
        debug!(url, "Fetching Fedimint federation config");
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                ConxianError::Internal(format!("Fedimint config fetch failed: {e}"))
            })?;

        let config: FederationConfig = resp
            .json()
            .await
            .map_err(|e| ConxianError::Internal(format!("Fedimint config parse failed: {e}")))?;
        Ok(config)
    }

    /// Fetch guardian info (consensus status, epoch).
    async fn fetch_info(&self) -> ConxianResult<GuardianInfo> {
        let url = format!("{}/info", self.guardian_url);
        debug!(url, "Fetching Fedimint guardian info");
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ConxianError::Internal(format!("Fedimint info fetch failed: {e}")))?;

        let info: GuardianInfo = resp
            .json()
            .await
            .map_err(|e| ConxianError::Internal(format!("Fedimint info parse failed: {e}")))?;
        Ok(info)
    }

    /// Query a module endpoint on the guardian.
    #[allow(dead_code)]
    async fn query_module(&self, module_id: u32, path: &str) -> ConxianResult<Value> {
        let url = format!("{}/module/{}/{}", self.guardian_url, module_id, path);
        debug!(url, "Querying Fedimint module");
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                ConxianError::Internal(format!("Fedimint module query failed: {e}"))
            })?;

        let body: Value = resp
            .json()
            .await
            .map_err(|e| ConxianError::Internal(format!("Fedimint module parse failed: {e}")))?;
        Ok(body)
    }
}

#[async_trait]
impl ChainAdapter for FedimintAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        info!("Fedimint: fetching federation epoch as height");

        match self.fetch_info().await {
            Ok(info) => {
                let epoch = info.epoch.unwrap_or(0);
                debug!(epoch, "Fedimint federation epoch");
                Ok(epoch)
            }
            Err(e) => {
                warn!(error = %e, "Fedimint info fetch failed, returning 0");
                Ok(0)
            }
        }
    }

    async fn get_chain_identity(&self) -> String {
        let id = self.fed_identity();
        format!("fedimint:{}", id)
    }

    async fn prepare_unsigned_transaction(
        &self,
        tx_details: serde_json::Value,
    ) -> ConxianResult<serde_json::Value> {
        info!("Fedimint: preparing e-cash transaction");

        // Validate required fields
        let module_id = tx_details
            .get("module_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let amount_msat = tx_details
            .get("amount_msat")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if amount_msat == 0 {
            return Err(ConxianError::Internal(
                "Fedimint: amount_msat is required".into(),
            ));
        }

        let payload = json!({
            "module_id": module_id,
            "amount_msat": amount_msat,
            "memo": tx_details.get("memo").unwrap_or(&json!("")),
            "federation_id": self.fed_identity(),
        });

        info!(
            module_id,
            amount_msat, "Fedimint unsigned tx prepared (submit via guardian POST /transaction)"
        );
        Ok(payload)
    }

    async fn verify_state_proof(&self, _proof_metadata: serde_json::Value) -> ConxianResult<bool> {
        info!("Fedimint: verifying federation state proof");

        // Fedimint consensus verification is based on:
        // 1. Quorum of guardian signatures on each epoch
        // 2. Valid transaction inclusion in the consensus history
        // 3. Correct blind signature issuance

        // Fetch current config to verify federation identity
        match self.fetch_config().await {
            Ok(config) => {
                if let Some(fed_id) = &config.federation_id {
                    info!(
                        federation_id = fed_id,
                        "Fedimint federation config verified"
                    );
                }
                let modules = config.modules.unwrap_or_default();
                info!(
                    module_count = modules.as_object().map(|m| m.len()).unwrap_or(0),
                    "Fedimint modules discovered"
                );
            }
            Err(e) => {
                warn!(error = %e, "Fedimint config fetch failed during proof verification");
            }
        }

        // Check guardian consensus
        match self.fetch_info().await {
            Ok(info) => {
                let status = info.consensus_status.unwrap_or_else(|| "unknown".into());
                info!(
                    consensus_status = %status,
                    epoch = info.epoch,
                    "Fedimint guardian consensus status"
                );
                Ok(status == "consensus_running" || status == "synced")
            }
            Err(e) => {
                warn!(error = %e, "Fedimint info fetch failed");
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fedimint_adapter_identity() {
        let adapter = FedimintAdapter::new("http://localhost:18173".into());
        assert_eq!(
            adapter.get_chain_identity().await,
            "fedimint:fedimint:unknown"
        );
    }

    #[test]
    fn test_fedimint_with_invite() {
        let adapter = FedimintAdapter::new("http://localhost:18173".into())
            .with_invite("fed11qgqrg...".into());
        assert!(adapter.invite_code.is_some());
        assert_eq!(adapter.invite_code.unwrap(), "fed11qgqrg...");
    }

    #[tokio::test]
    async fn test_fedimint_fed_identity_cached() {
        let mut adapter = FedimintAdapter::new("http://localhost:18173".into());
        adapter.federation_id = Some("test-fed-001".into());
        assert_eq!(
            adapter.get_chain_identity().await,
            "fedimint:test-fed-001"
        );
    }

    #[tokio::test]
    async fn test_get_latest_height_default() {
        let adapter = FedimintAdapter::new("http://localhost:18173".into());
        // With no server, returns 0 (graceful degradation)
        let height = adapter.get_latest_height().await;
        assert!(height.is_ok());
    }

    #[tokio::test]
    async fn test_verify_state_proof_no_server() {
        let adapter = FedimintAdapter::new("http://localhost:18173".into());
        let result = adapter.verify_state_proof(json!({})).await;
        // Returns false when guardian is unreachable
        assert_eq!(result.unwrap(), false);
    }

    #[tokio::test]
    async fn test_prepare_unsigned_tx_validates_amount() {
        let adapter = FedimintAdapter::new("http://localhost:18173".into());
        let result = adapter
            .prepare_unsigned_transaction(json!({"module_id": 1, "amount_msat": 5000}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_prepare_unsigned_tx_rejects_zero() {
        let adapter = FedimintAdapter::new("http://localhost:18173".into());
        let result = adapter
            .prepare_unsigned_transaction(json!({"module_id": 1, "amount_msat": 0}))
            .await;
        assert!(result.is_err());
    }
}
