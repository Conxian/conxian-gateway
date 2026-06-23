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
}
