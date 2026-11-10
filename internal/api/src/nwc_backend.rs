//! Nostr Wallet Connect (NIP-47) Lightning settlement backend.
//!
//! Selected by [`crate::new_lightning_adapter_from_env`] when `NWC_URI` holds a
//! valid `nostr+walletconnect://` URI. When the variable is absent or malformed
//! the adapter keeps the fail-closed production stub, so settlement stays closed
//! until a wallet is configured.

use crate::lightning::{
    LightningBackend, LightningBackendError, LightningSettlementRequest,
    LightningSettlementResponse,
};
use crate::nostr::NwcConnection;
use async_trait::async_trait;
use nwc::prelude::*;
use std::time::Duration;
use tracing::{debug, warn};

pub struct NwcLightningBackend {
    connection: NwcConnection,
    timeout: Duration,
}

impl NwcLightningBackend {
    pub fn new(connection: NwcConnection) -> Self {
        Self {
            connection,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn client(&self) -> Result<NostrWalletConnect, LightningBackendError> {
        let uri = NostrWalletConnectUri::parse(self.connection.to_uri_string())
            .map_err(|_| LightningBackendError::Unavailable)?;
        Ok(NostrWalletConnect::new(uri))
    }
}

#[async_trait]
impl LightningBackend for NwcLightningBackend {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        let client = self.client()?;
        let pay_request = PayInvoiceRequest {
            id: None,
            invoice: request.challenge.clone(),
            amount: None,
        };

        let result = tokio::time::timeout(
            self.timeout,
            client.pay_invoice(pay_request).timeout(self.timeout),
        )
        .await;

        match result {
            Ok(Ok(response)) => {
                debug!(
                    preimage = %response.preimage,
                    fees_paid = ?response.fees_paid,
                    "NWC payment settled"
                );
                Ok(LightningSettlementResponse {
                    settled_amount: request.amount,
                    preimage: response.preimage.clone(),
                    proof: format!("nwc:{}", response.preimage),
                })
            }
            Ok(Err(e)) => {
                warn!(error = %e, "NWC payment rejected");
                Err(LightningBackendError::Rejected {
                    detail: e.to_string(),
                })
            }
            Err(_) => {
                warn!("NWC payment timed out");
                Err(LightningBackendError::Unavailable)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Values from the `nwc` crate's own documented example URI.
    const VALID_PUBKEY: &str = "b889ff5b1513b641e2a139f661a661364979c5beee91842f8f0ef42ab558e9d4";
    const VALID_SECRET: &str = "71a8c14c1407c113601079c4302dab36460f0ccd0ad506f1f2dc73b5100e4f3c";

    fn connection(relay: &str, secret: &str) -> NwcConnection {
        NwcConnection {
            pubkey: VALID_PUBKEY.into(),
            relay: relay.into(),
            secret: secret.into(),
            lud16: None,
        }
    }

    #[test]
    fn valid_uri_builds_client() {
        let backend = NwcLightningBackend::new(connection("wss://relay.damus.io", VALID_SECRET));
        assert!(backend.client().is_ok());
    }

    #[test]
    fn malformed_secret_fails_closed() {
        // `deadbeef` is not a 32-byte hex secret, so URI parsing must reject it.
        let backend = NwcLightningBackend::new(connection("wss://relay.damus.io", "deadbeef"));
        assert!(backend.client().is_err());
    }

    #[tokio::test]
    async fn unreachable_relay_fails_closed() {
        let backend = NwcLightningBackend::new(connection("wss://127.0.0.1:1", VALID_SECRET))
            .with_timeout(Duration::from_millis(150));
        let req = LightningSettlementRequest {
            challenge: "lnbc1...".into(),
            amount: 1000,
            asset: "BTC".into(),
            expiry: u64::MAX,
            proof_refs: vec![],
        };
        assert!(backend.settle_payment(req).await.is_err());
    }
}
