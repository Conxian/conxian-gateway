use conxian_core::{ConxianError, ConxianResult};
use serde::{Deserialize, Serialize};
use tracing::info;

/// CON-1267: Nostr Wallet Connect (NWC) transport skeleton.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NwcConnection {
    pub pubkey: String,
    pub relay: String,
    pub secret: String,
    pub lud16: Option<String>,
}

impl NwcConnection {
    /// Reconstruct the nostr+walletconnect:// URI from connection details
    pub fn to_uri_string(&self) -> String {
        let mut uri = format!(
            "nostr+walletconnect://{}?relay={}&secret={}",
            self.pubkey,
            urlencoding::encode(&self.relay),
            self.secret
        );
        if let Some(ref lud16) = self.lud16 {
            uri.push_str(&format!("&lud16={}", urlencoding::encode(lud16)));
        }
        uri
    }

    pub fn parse_uri(uri: &str) -> ConxianResult<Self> {
        info!("Parsing NWC connection URI: {}", uri);

        if !uri.starts_with("nostr+walletconnect://") {
            return Err(ConxianError::Compliance(
                "Invalid NWC URI scheme".to_string(),
            ));
        }

        let parts: Vec<&str> = uri
            .strip_prefix("nostr+walletconnect://")
            .unwrap()
            .split('?')
            .collect();
        if parts.is_empty() {
            return Err(ConxianError::Compliance(
                "Invalid NWC URI format".to_string(),
            ));
        }

        let pubkey = parts[0].to_string();
        let mut relay = String::new();
        let mut secret = String::new();
        let mut lud16 = None;

        if parts.len() > 1 {
            for param in parts[1].split('&') {
                let kv: Vec<&str> = param.split('=').collect();
                if kv.len() == 2 {
                    let key = kv[0];
                    let val = urlencoding::decode(kv[1])
                        .map_err(|e| {
                            ConxianError::Compliance(format!("Invalid URL encoding: {}", e))
                        })?
                        .into_owned();
                    match key {
                        "relay" => relay = val,
                        "secret" => secret = val,
                        "lud16" => lud16 = Some(val),
                        _ => {}
                    }
                }
            }
        }

        if pubkey.is_empty() || relay.is_empty() || secret.is_empty() {
            return Err(ConxianError::Compliance(
                "Missing mandatory NWC URI parameters".to_string(),
            ));
        }

        Ok(Self {
            pubkey,
            relay,
            secret,
            lud16,
        })
    }

    pub fn construct_payment_request(&self, invoice: &str) -> ConxianResult<serde_json::Value> {
        info!("Constructing NWC payment request for invoice: {}", invoice);

        // Industry Enhancement: Real NIP-47 request construction (Kind 23194)
        Ok(serde_json::json!({
            "method": "pay_invoice",
            "params": {
                "invoice": invoice
            }
        }))
    }

    pub fn construct_make_invoice_request(
        &self,
        amount_msat: u64,
        description: Option<String>,
        expiry: Option<u64>,
    ) -> ConxianResult<serde_json::Value> {
        info!(
            "Constructing NWC make_invoice request for {} msat",
            amount_msat
        );

        Ok(serde_json::json!({
            "method": "make_invoice",
            "params": {
                "amount": amount_msat,
                "description": description,
                "expiry": expiry
            }
        }))
    }

    pub fn construct_lookup_invoice_request(
        &self,
        payment_hash: Option<String>,
        invoice: Option<String>,
    ) -> ConxianResult<serde_json::Value> {
        info!("Constructing NWC lookup_invoice request");

        Ok(serde_json::json!({
            "method": "lookup_invoice",
            "params": {
                "payment_hash": payment_hash,
                "invoice": invoice
            }
        }))
    }

    pub fn construct_get_balance_request(&self) -> ConxianResult<serde_json::Value> {
        info!("Constructing NWC get_balance request");

        Ok(serde_json::json!({
            "method": "get_balance",
            "params": {}
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nwc_uri() {
        let uri = "nostr+walletconnect://addr123?relay=wss%3A%2F%2Frelay.com&secret=sec123";
        let conn = NwcConnection::parse_uri(uri).unwrap();
        assert_eq!(conn.pubkey, "addr123");
        assert_eq!(conn.relay, "wss://relay.com");
        assert_eq!(conn.secret, "sec123");
    }

    #[test]
    fn test_construct_nwc_requests() {
        let conn = NwcConnection {
            pubkey: "pk".into(),
            relay: "relay".into(),
            secret: "sec".into(),
            lud16: None,
        };

        let pay = conn.construct_payment_request("inv123").unwrap();
        assert_eq!(pay["method"], "pay_invoice");
        assert_eq!(pay["params"]["invoice"], "inv123");

        let make = conn
            .construct_make_invoice_request(1000, Some("test".into()), None)
            .unwrap();
        assert_eq!(make["method"], "make_invoice");
        assert_eq!(make["params"]["amount"], 1000);

        let lookup = conn
            .construct_lookup_invoice_request(Some("hash".into()), None)
            .unwrap();
        assert_eq!(lookup["method"], "lookup_invoice");
        assert_eq!(lookup["params"]["payment_hash"], "hash");

        let balance = conn.construct_get_balance_request().unwrap();
        assert_eq!(balance["method"], "get_balance");
    }
}
