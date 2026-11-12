use conxian_core::{ConxianError, ConxianResult};
use serde::{Deserialize, Serialize};
use tracing::info;

/// CON-1267: Nostr Wallet Connect (NWC) transport skeleton.
#[derive(Serialize, Deserialize, Clone)]
pub struct NwcConnection {
    pub pubkey: String,
    /// Primary relay (kept for backward compatibility; equals `relays[0]`).
    pub relay: String,
    pub secret: String,
    pub lud16: Option<String>,
    /// All relays from the NWC URI, in order. The `nwc` client connects to every
    /// one, so a single relay outage no longer closes the payment rail (the #1
    /// NWC failure mode). Defaults to `[relay]` when deserialising an older record.
    #[serde(default)]
    pub relays: Vec<String>,
}

impl std::fmt::Debug for NwcConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never expose the wallet-connect secret in logs or error output.
        f.debug_struct("NwcConnection")
            .field("pubkey", &self.pubkey)
            .field("relay", &self.relay)
            .field("relays", &self.relays)
            .field("secret", &"<redacted>")
            .field("lud16", &self.lud16)
            .finish()
    }
}

impl NwcConnection {
    /// The effective relay set: `relays` if non-empty, else the single `relay`.
    /// De-duplicates while preserving order, and drops blanks.
    pub fn relay_list(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let source = if self.relays.is_empty() {
            std::slice::from_ref(&self.relay)
        } else {
            &self.relays[..]
        };
        for relay in source {
            let trimmed = relay.trim();
            if !trimmed.is_empty() && !out.iter().any(|r| r == trimmed) {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    /// Reconstruct the nostr+walletconnect:// URI from connection details.
    /// Emits one `relay=` parameter per relay so the client connects to all of
    /// them (NIP-47 permits a relay list).
    pub fn to_uri_string(&self) -> String {
        let relays = self.relay_list();
        let relay_params: String = relays
            .iter()
            .map(|r| format!("&relay={}", urlencoding::encode(r)))
            .collect();
        let mut uri = format!(
            "nostr+walletconnect://{}?secret={}{}",
            self.pubkey, self.secret, relay_params
        );
        if let Some(ref lud16) = self.lud16 {
            uri.push_str(&format!("&lud16={}", urlencoding::encode(lud16)));
        }
        uri
    }

    pub fn parse_uri(uri: &str) -> ConxianResult<Self> {
        // Do not log the URI: it carries the wallet-connect `secret` value.
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
        let mut relays: Vec<String> = Vec::new();
        let mut secret = String::new();
        let mut lud16 = None;

        if parts.len() > 1 {
            for param in parts[1].split('&') {
                let kv: Vec<&str> = param.splitn(2, '=').collect();
                if kv.len() == 2 {
                    let key = kv[0];
                    let val = urlencoding::decode(kv[1])
                        .map_err(|e| {
                            ConxianError::Compliance(format!("Invalid URL encoding: {}", e))
                        })?
                        .into_owned();
                    match key {
                        // A URI may carry several `relay=` params; collect them all.
                        "relay" => {
                            let trimmed = val.trim();
                            if !trimmed.is_empty() && !relays.iter().any(|r| r == trimmed) {
                                relays.push(trimmed.to_string());
                            }
                        }
                        "secret" => secret = val,
                        "lud16" => lud16 = Some(val),
                        _ => {}
                    }
                }
            }
        }

        if pubkey.is_empty() || relays.is_empty() || secret.is_empty() {
            return Err(ConxianError::Compliance(
                "Missing mandatory NWC URI parameters".to_string(),
            ));
        }

        let relay = relays[0].clone();
        Ok(Self {
            pubkey,
            relay,
            secret,
            lud16,
            relays,
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
    fn debug_redacts_secret() {
        let conn = NwcConnection {
            pubkey: "pk".into(),
            relay: "wss://relay.example".into(),
            secret: "super-secret-value".into(),
            lud16: None,
            relays: vec![],
        };
        let rendered = format!("{conn:?}");
        assert!(!rendered.contains("super-secret-value"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn test_parse_nwc_uri() {
        let uri = "nostr+walletconnect://addr123?relay=wss%3A%2F%2Frelay.com&secret=sec123";
        let conn = NwcConnection::parse_uri(uri).unwrap();
        assert_eq!(conn.pubkey, "addr123");
        assert_eq!(conn.relay, "wss://relay.com");
        assert_eq!(conn.secret, "sec123");
        assert_eq!(conn.relays, vec!["wss://relay.com".to_string()]);
        assert_eq!(conn.relay_list(), vec!["wss://relay.com".to_string()]);
    }

    #[test]
    fn test_parse_multi_relay_uri() {
        // NIP-47 permits several relay= params; all must be collected, in order.
        let uri = "nostr+walletconnect://addr123?relay=wss%3A%2F%2Frelay-a.com&relay=wss%3A%2F%2Frelay-b.com&secret=sec123&relay=wss%3A%2F%2Frelay-c.com";
        let conn = NwcConnection::parse_uri(uri).unwrap();
        assert_eq!(
            conn.relay_list(),
            vec![
                "wss://relay-a.com".to_string(),
                "wss://relay-b.com".to_string(),
                "wss://relay-c.com".to_string(),
            ]
        );
        // Primary relay stays the first for backward compatibility.
        assert_eq!(conn.relay, "wss://relay-a.com");
    }

    #[test]
    fn test_parse_deduplicates_relays() {
        let uri =
            "nostr+walletconnect://addr?relay=wss%3A%2F%2Fr.com&relay=wss%3A%2F%2Fr.com&secret=s";
        let conn = NwcConnection::parse_uri(uri).unwrap();
        assert_eq!(conn.relay_list(), vec!["wss://r.com".to_string()]);
    }

    #[test]
    fn test_to_uri_string_emits_all_relays() {
        let conn = NwcConnection {
            pubkey: "pk".into(),
            relay: "wss://a".into(),
            secret: "sec".into(),
            lud16: Some("lud@x.com".into()),
            relays: vec!["wss://a".into(), "wss://b".into()],
        };
        let uri = conn.to_uri_string();
        assert!(uri.starts_with("nostr+walletconnect://pk?secret=sec"));
        assert!(uri.contains("&relay=wss%3A%2F%2Fa"));
        assert!(uri.contains("&relay=wss%3A%2F%2Fb"));
        assert!(uri.contains("&lud16=lud%40x.com"));
        // Round-trips: parsing the rebuilt URI recovers both relays.
        let parsed = NwcConnection::parse_uri(&uri).unwrap();
        assert_eq!(
            parsed.relay_list(),
            vec!["wss://a".to_string(), "wss://b".to_string()]
        );
    }

    #[test]
    fn test_relay_list_falls_back_to_primary_when_empty() {
        // An older deserialised record (no `relays`) still yields its primary.
        let conn = NwcConnection {
            pubkey: "pk".into(),
            relay: "wss://only".into(),
            secret: "s".into(),
            lud16: None,
            relays: vec![],
        };
        assert_eq!(conn.relay_list(), vec!["wss://only".to_string()]);
    }

    #[test]
    fn test_parse_rejects_missing_relay() {
        let uri = "nostr+walletconnect://addr123?secret=sec123";
        assert!(NwcConnection::parse_uri(uri).is_err());
    }

    #[test]
    fn test_construct_nwc_requests() {
        let conn = NwcConnection {
            pubkey: "pk".into(),
            relay: "relay".into(),
            secret: "sec".into(),
            lud16: None,
            relays: vec![],
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
