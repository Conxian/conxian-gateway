use crate::lightning::{
    LightningBackend, LightningBackendError, LightningSettlementRequest,
    LightningSettlementResponse,
};
use crate::nostr::NwcConnection;
use async_trait::async_trait;
use nwc::prelude::*;
use std::time::Duration;
use tracing::{debug, info, warn};

pub struct NwcLightningBackend {
    connection: NwcConnection,
    timeout: Duration,
}

impl NwcLightningBackend {
    pub async fn new(connection: NwcConnection) -> Result<Self, NwcInitError> {
        let uri: NostrWalletConnectUri = connection
            .to_uri_string()
            .parse()
            .map_err(|e| NwcInitError::UriParse(e.to_string()))?;
        let nwc_client = NWC::new(uri);
        let balance = nwc_client
            .get_balance()
            .await
            .map_err(|e| NwcInitError::Connection(e.to_string()))?;
        info!(msats = balance.balance, "NWC backend initialized");

        Ok(Self {
            connection,
            timeout: Duration::from_secs(30),
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[derive(Debug)]
pub enum NwcInitError {
    UriParse(String),
    Connection(String),
    Unsupported(String),
}

impl std::fmt::Display for NwcInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UriParse(e) => write!(f, "NWC URI parse error: {e}"),
            Self::Connection(e) => write!(f, "NWC connection error: {e}"),
            Self::Unsupported(e) => write!(f, "NWC unsupported: {e}"),
        }
    }
}

#[async_trait]
impl LightningBackend for NwcLightningBackend {
    async fn settle_payment(
        &self,
        request: LightningSettlementRequest,
    ) -> Result<LightningSettlementResponse, LightningBackendError> {
        let uri: NostrWalletConnectUri = self
            .connection
            .to_uri_string()
            .parse()
            .map_err(|e| LightningBackendError::Unavailable)?;

        let nwc_client = NWC::new(uri);

        let pay_result = tokio::time::timeout(self.timeout, async {
            let invoice = request.challenge.clone();
            nwc_client
                .pay_invoice(invoice)
                .await
                .map_err(|e| format!("NWC pay_invoice failed: {e}"))
        })
        .await;

        match pay_result {
            Ok(Ok(response)) => {
                debug!(
                    preimage = %response.preimage,
                    fees_paid = response.fees_paid,
                    "NWC payment settled"
                );
                Ok(LightningSettlementResponse {
                    settled_amount: request.amount,
                    preimage: response.preimage,
                    proof: format!("nwc:{}", response.preimage),
                })
            }
            Ok(Err(e)) => {
                warn!(error = %e, "NWC payment rejected");
                Err(LightningBackendError::Rejected { detail: e })
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

    #[test]
    fn test_nwc_backend_init_error_display() {
        let err = NwcInitError::UriParse("bad".into());
        assert!(err.to_string().contains("bad"));

        let err = NwcInitError::Connection("down".into());
        assert!(err.to_string().contains("down"));
    }

    #[tokio::test]
    async fn test_nwc_backend_with_fake_uri_rejects() {
        let conn = NwcConnection {
            pubkey: "0000000000000000000000000000000000000000000000000000000000000000".into(),
            relay: "wss://nonexistent.example".into(),
            secret: "deadbeef".into(),
            lud16: None,
        };
        let backend = NwcLightningBackend {
            connection: conn,
            timeout: Duration::from_millis(100),
        };
        let req = LightningSettlementRequest {
            challenge: "lnbc1...".into(),
            amount: 1000,
            asset: "BTC".into(),
            expiry: u64::MAX,
            proof_refs: vec![],
        };
        let result = backend.settle_payment(req).await;
        assert!(result.is_err());
    }
}
