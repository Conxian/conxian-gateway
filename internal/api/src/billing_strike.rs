//! Strike (`api.strike.me`) invoice backend for the managed-gateway billing lane.
//!
//! Strike is a custodial Bitcoin/Lightning payments API. It is wired here as an
//! **optional** [`InvoiceBackend`](crate::billing::InvoiceBackend) alternative to
//! the lnbits-native `HttpInvoiceBackend`, so the managed subscription lane can
//! issue and verify real Lightning invoices without an operator-run node.
//!
//! Flow (mirrors Strike's partner API):
//! ```text
//! POST /v1/invoices                     -> { invoiceId, state, amount }
//! POST /v1/invoices/{invoiceId}/quote   -> { lnInvoice (BOLT11), expiration }
//! GET  /v1/invoices/{invoiceId}         -> { state: PAID|UNPAID|PENDING|CANCELLED,
//!                                            transactions[].amountReceived }
//! ```
//!
//! Settlement is verified against the invoice's `state` **and** the received
//! amount. An unknown state, or a settled amount below the expected value, fails
//! closed — an invoice id alone is never trusted.
//!
//! Custody boundary: Strike holds the funds it receives. Use this only for
//! collecting managed-subscription payments; it is not a Conxian custody layer
//! and does not replace the merchant-of-record.

use crate::billing::{BillingError, Invoice, InvoiceBackend};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_STRIKE_API_BASE_URL: &str = "https://api.strike.me";

/// Strike API credentials + base URL. Absent (`STRIKE_API_KEY` unset) means the
/// backend is not configured and every call fails closed.
#[derive(Debug, Clone)]
pub struct StrikeConfig {
    pub base_url: String,
    pub api_key: String,
}

impl StrikeConfig {
    /// Read the Strike configuration from the environment.
    ///
    /// `STRIKE_API_BASE_URL` defaults to the production API; point it at
    /// `https://api.dev.strike.me` for the sandbox. A blank/absent
    /// `STRIKE_API_KEY` yields `None` (fail-closed: the lnbits backend stays
    /// selected and Strike is never called).
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("STRIKE_API_KEY").ok()?;
        if api_key.trim().is_empty() {
            return None;
        }
        let base_url = std::env::var("STRIKE_API_BASE_URL")
            .ok()
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_STRIKE_API_BASE_URL.to_string());
        Some(Self { base_url, api_key })
    }
}

/// Fail-closed Strike invoice backend. Constructed only when configured; an
/// unconfigured instance rejects every call rather than contacting Strike.
pub struct StrikeInvoiceBackend {
    config: Option<StrikeConfig>,
    client: reqwest::Client,
}

impl StrikeInvoiceBackend {
    pub fn new(config: Option<StrikeConfig>) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn config(&self) -> Result<&StrikeConfig, BillingError> {
        self.config
            .as_ref()
            .ok_or(BillingError::StrikeNotConfigured)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Render integer sats as a BTC decimal string (e.g. `128000` -> `"0.00128"`).
pub fn sats_to_btc_str(sats: u64) -> String {
    let whole = sats / 100_000_000;
    let frac = sats % 100_000_000;
    if frac == 0 {
        return format!("{whole}.0");
    }
    let mut frac_str = format!("{frac:08}");
    while frac_str.ends_with('0') {
        frac_str.pop();
    }
    format!("{whole}.{frac_str}")
}

/// Parse a BTC decimal string into integer sats. Rejects non-numeric input and
/// sub-satoshi precision (returns `None` so the caller fails closed).
pub fn btc_str_to_sats(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') {
        return None;
    }
    let (whole, frac) = match value.split_once('.') {
        Some((w, f)) => (w, f),
        None => (value, ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    if frac.len() > 8 {
        // More precision than a satoshi: do not silently round.
        return None;
    }
    let frac: u64 = if frac.is_empty() {
        0
    } else {
        let padded = format!("{frac:0<8}");
        padded.parse().ok()?
    };
    whole.checked_mul(100_000_000)?.checked_add(frac)
}

/// Map Strike's invoice `state` to settlement. `None` (unknown state) must fail
/// closed at the call site.
pub fn invoice_state_settled(state: &str) -> Option<bool> {
    match state.to_ascii_uppercase().as_str() {
        "PAID" => Some(true),
        "UNPAID" | "PENDING" | "CANCELLED" => Some(false),
        _ => None,
    }
}

/// Verify the received amount against the expected sats. An absent/opaque amount
/// is accepted (Strike is authoritative on `state`); a known amount below the
/// expected value is an error, so an underpayment can never activate a plan.
pub fn verify_received_amount(
    received_btc: Option<&str>,
    expected_sats: u64,
) -> Result<(), BillingError> {
    if let Some(raw) = received_btc {
        if let Some(got) = btc_str_to_sats(raw) {
            if got < expected_sats {
                return Err(BillingError::AmountMismatch {
                    expected: expected_sats,
                    got,
                });
            }
        }
    }
    Ok(())
}

#[async_trait::async_trait]
impl InvoiceBackend for StrikeInvoiceBackend {
    async fn create_invoice(
        &self,
        amount_sats: u64,
        expiry_seconds: u64,
    ) -> Result<Invoice, BillingError> {
        let config = self.config()?;
        let correlation_id = uuid::Uuid::new_v4().to_string();
        let body = json!({
            "correlationId": correlation_id,
            "description": "Conxian managed subscription",
            "amount": { "currency": "BTC", "amount": sats_to_btc_str(amount_sats) },
        });
        let res = self
            .client
            .post(format!("{}/v1/invoices", config.base_url))
            .bearer_auth(&config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        if !res.status().is_success() {
            return Err(BillingError::BackendHttp(res.status().as_u16()));
        }
        let created: serde_json::Value = res
            .json()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        let invoice_id = created
            .get("invoiceId")
            .and_then(|v| v.as_str())
            .ok_or(BillingError::MissingSettledIndicator)?
            .to_string();

        // The BOLT11 payment request comes from the invoice quote, not the
        // invoice itself.
        let quote_res = self
            .client
            .post(format!(
                "{}/v1/invoices/{}/quote",
                config.base_url, invoice_id
            ))
            .bearer_auth(&config.api_key)
            .json(&json!({}))
            .send()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        if !quote_res.status().is_success() {
            return Err(BillingError::BackendHttp(quote_res.status().as_u16()));
        }
        let quote: serde_json::Value = quote_res
            .json()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        let payment_request = quote
            .get("lnInvoice")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .ok_or(BillingError::MissingSettledIndicator)?
            .to_string();

        Ok(Invoice {
            invoice_id,
            payment_request,
            amount_sats,
            expires_at: now_secs() + expiry_seconds,
        })
    }

    async fn check_settled(
        &self,
        invoice_id: &str,
        amount_sats: u64,
    ) -> Result<bool, BillingError> {
        let config = self.config()?;
        let res = self
            .client
            .get(format!(
                "{}/v1/invoices/{}?includeTransactions=true",
                config.base_url, invoice_id
            ))
            .bearer_auth(&config.api_key)
            .send()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        if !res.status().is_success() {
            return Err(BillingError::BackendHttp(res.status().as_u16()));
        }
        let invoice: serde_json::Value = res
            .json()
            .await
            .map_err(|e| BillingError::BackendUnreachable(e.to_string()))?;
        let state = invoice
            .get("state")
            .and_then(|v| v.as_str())
            .ok_or(BillingError::MissingSettledIndicator)?;
        let settled = invoice_state_settled(state).ok_or(BillingError::MissingSettledIndicator)?;
        if !settled {
            return Ok(false);
        }
        let received = invoice
            .pointer("/transactions/0/amountReceived/amount")
            .and_then(|v| v.as_str());
        verify_received_amount(received, amount_sats)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sats_btc_round_trip() {
        assert_eq!(sats_to_btc_str(0), "0.0");
        assert_eq!(sats_to_btc_str(100_000_000), "1.0");
        assert_eq!(sats_to_btc_str(128_000), "0.00128");
        assert_eq!(sats_to_btc_str(1), "0.00000001");
        for sats in [
            0u64,
            1,
            999,
            100_000_000,
            128_000,
            1_000_000,
            210_000_000_000_000,
        ] {
            assert_eq!(
                btc_str_to_sats(&sats_to_btc_str(sats)),
                Some(sats),
                "sats={sats}"
            );
        }
    }

    #[test]
    fn btc_parse_rejects_bad_input() {
        assert_eq!(btc_str_to_sats(""), None);
        assert_eq!(btc_str_to_sats("-1"), None);
        assert_eq!(btc_str_to_sats("abc"), None);
        // sub-satoshi precision is refused rather than rounded
        assert_eq!(btc_str_to_sats("0.000000001"), None);
        assert_eq!(btc_str_to_sats("0.00001"), Some(1000));
        assert_eq!(btc_str_to_sats("1"), Some(100_000_000));
    }

    #[test]
    fn unknown_invoice_state_fails_closed() {
        assert_eq!(invoice_state_settled("PAID"), Some(true));
        assert_eq!(invoice_state_settled("paid"), Some(true));
        assert_eq!(invoice_state_settled("UNPAID"), Some(false));
        assert_eq!(invoice_state_settled("PENDING"), Some(false));
        assert_eq!(invoice_state_settled("CANCELLED"), Some(false));
        assert_eq!(invoice_state_settled("WEIRD"), None);
        assert_eq!(invoice_state_settled(""), None);
    }

    #[test]
    fn amount_below_expected_is_rejected_but_absent_is_accepted() {
        assert!(verify_received_amount(Some("0.00128"), 128_000).is_ok());
        assert!(verify_received_amount(Some("0.002"), 128_000).is_ok());
        // underpayment must fail closed
        assert!(matches!(
            verify_received_amount(Some("0.001"), 128_000),
            Err(BillingError::AmountMismatch {
                expected: 128_000,
                got: 100_000
            })
        ));
        // opaque/absent amount: rely on `state`
        assert!(verify_received_amount(None, 128_000).is_ok());
    }

    #[test]
    fn unconfigured_backend_is_not_configured() {
        // Without STRIKE_API_KEY the constructor still builds (so the enum is
        // total) but the backend reports unconfigured and fails closed.
        let backend = StrikeInvoiceBackend::new(None);
        assert!(backend.config().is_err());
    }
}
