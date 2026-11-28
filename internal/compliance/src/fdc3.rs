//! FDC3 3.0 `fdc3.payment` bridge for ISO 20022 `pacs.008` ingress.
//!
//! This is a bounded translation layer. It does not validate an ISO 20022 XSD,
//! clear a payment, or assert settlement finality. It exists so a Conxian
//! Desktop Connector can accept a standard FDC3 payment context and route it
//! through the Gateway's existing ISO 20022 profile without inventing a
//! `conxian.settlement` wire format.

use crate::iso20022::{Iso20022MessageType, Iso20022RoutingEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const FDC3_PAYMENT_CONTEXT_TYPE: &str = "fdc3.payment";
pub const FDC3_TRANSACTION_RESULT_CONTEXT_TYPE: &str = "fdc3.transactionResult";
const MAX_FIELD_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fdc3Party {
    #[serde(default)]
    pub name: Option<String>,
    pub account: String,
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fdc3Payment {
    #[serde(rename = "type", default = "default_payment_context_type")]
    pub context_type: String,
    pub id: HashMap<String, String>,
    #[serde(default)]
    pub name: Option<String>,
    pub amount: f64,
    pub currency: String,
    pub debtor: Fdc3Party,
    pub creditor: Fdc3Party,
    #[serde(default, rename = "networkRouting")]
    pub network_routing: Option<HashMap<String, String>>,
}

fn default_payment_context_type() -> String {
    FDC3_PAYMENT_CONTEXT_TYPE.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fdc3TransactionStatus {
    Created,
    Deleted,
    Updated,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fdc3TransactionResult {
    #[serde(rename = "type", default = "default_transaction_result_context_type")]
    pub context_type: String,
    pub status: Fdc3TransactionStatus,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default)]
    pub message: Option<String>,
}

fn default_transaction_result_context_type() -> String {
    FDC3_TRANSACTION_RESULT_CONTEXT_TYPE.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fdc3Error {
    UnsupportedMessageType,
    MissingAccount(&'static str),
    MissingIdentifier,
    InvalidField(String),
    Serialization(String),
}

impl std::fmt::Display for Fdc3Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedMessageType => {
                write!(f, "FDC3 payments require an ISO 20022 pacs.008 message")
            }
            Self::MissingAccount(party) => write!(f, "{party}.account is required by fdc3.payment"),
            Self::MissingIdentifier => {
                write!(f, "fdc3.payment requires at least one payment identifier")
            }
            Self::InvalidField(reason) => write!(f, "invalid FDC3 payment field: {reason}"),
            Self::Serialization(reason) => write!(f, "FDC3 serialization failed: {reason}"),
        }
    }
}

impl std::error::Error for Fdc3Error {}

impl Fdc3Payment {
    /// Converts an ISO 20022 routing event into a standard `fdc3.payment`.
    pub fn from_iso20022(event: &Iso20022RoutingEvent) -> Result<Self, Fdc3Error> {
        if event.message_type != Iso20022MessageType::Pacs008 {
            return Err(Fdc3Error::UnsupportedMessageType);
        }
        let debtor_account = event
            .debtor_account
            .clone()
            .ok_or(Fdc3Error::MissingAccount("debtor"))?;
        let creditor_account = event
            .creditor_account
            .clone()
            .ok_or(Fdc3Error::MissingAccount("creditor"))?;

        let mut id = HashMap::new();
        if let Some(uetr) = event.uetr.as_ref() {
            id.insert("UETR".to_string(), uetr.clone());
        } else if let Some(end_to_end_id) = event.end_to_end_id.as_ref() {
            id.insert("endToEndId".to_string(), end_to_end_id.clone());
        } else if let Some(instruction_id) = event.instruction_id.as_ref() {
            id.insert("instructionId".to_string(), instruction_id.clone());
        } else {
            id.insert("messageId".to_string(), event.message_id.clone());
        }

        let mut network_routing = HashMap::new();
        network_routing.insert("rail".to_string(), "ISO20022".to_string());
        network_routing.insert("messageType".to_string(), "pacs.008".to_string());
        if let Some(settlement_date) = event.settlement_date.as_ref() {
            network_routing.insert("settlementDate".to_string(), settlement_date.clone());
        }

        let payment = Self {
            context_type: FDC3_PAYMENT_CONTEXT_TYPE.to_string(),
            id,
            name: Some("ISO 20022 pacs.008 Settlement".to_string()),
            amount: amount_major(event.amount_minor, event.amount_scale)?,
            currency: event.currency.clone(),
            debtor: Fdc3Party {
                name: Some(event.debtor.clone()),
                account: debtor_account,
                agent: event.debtor_agent.clone(),
            },
            creditor: Fdc3Party {
                name: Some(event.creditor.clone()),
                account: creditor_account,
                agent: event.creditor_agent.clone(),
            },
            network_routing: Some(network_routing),
        };
        payment.validate()?;
        Ok(payment)
    }

    /// Validates the fields that Conxian needs before routing a payment.
    pub fn validate(&self) -> Result<(), Fdc3Error> {
        if self.context_type != FDC3_PAYMENT_CONTEXT_TYPE {
            return Err(Fdc3Error::InvalidField(format!(
                "type must be {FDC3_PAYMENT_CONTEXT_TYPE}"
            )));
        }
        if !self.amount.is_finite() || self.amount <= 0.0 {
            return Err(Fdc3Error::InvalidField(
                "amount must be a finite positive number".to_string(),
            ));
        }
        if self.currency.len() != 3 || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(Fdc3Error::InvalidField(
                "currency must be a three-letter uppercase ISO 4217 code".to_string(),
            ));
        }
        if !self
            .id
            .iter()
            .any(|(key, value)| !key.trim().is_empty() && !value.trim().is_empty())
        {
            return Err(Fdc3Error::MissingIdentifier);
        }
        validate_optional_text("payment name", self.name.as_deref())?;
        validate_id_map("payment id", &self.id)?;
        validate_party("debtor", &self.debtor)?;
        validate_party("creditor", &self.creditor)?;
        if let Some(routing) = self.network_routing.as_ref() {
            validate_id_map("networkRouting", routing)?;
        }
        Ok(())
    }

    /// Emits the bounded JSON profile accepted by `iso20022::translate`.
    pub fn to_iso20022_json(&self) -> Result<String, Fdc3Error> {
        self.validate()?;

        let message_id = self
            .id
            .get("UETR")
            .or_else(|| self.id.get("endToEndId"))
            .or_else(|| self.id.get("instructionId"))
            .or_else(|| self.id.get("messageId"))
            .or_else(|| self.id.values().find(|value| !value.trim().is_empty()))
            .ok_or(Fdc3Error::MissingIdentifier)?;

        let mut profile = serde_json::Map::new();
        profile.insert("message_type".to_string(), serde_json::json!("pacs008"));
        profile.insert("message_id".to_string(), serde_json::json!(message_id));
        if let Some(instruction_id) = self.id.get("instructionId") {
            profile.insert(
                "instruction_id".to_string(),
                serde_json::json!(instruction_id),
            );
        }
        if let Some(end_to_end_id) = self.id.get("endToEndId") {
            profile.insert(
                "end_to_end_id".to_string(),
                serde_json::json!(end_to_end_id),
            );
        }
        if let Some(uetr) = self.id.get("UETR") {
            profile.insert("uetr".to_string(), serde_json::json!(uetr));
        }
        profile.insert(
            "amount".to_string(),
            serde_json::json!(format_amount(self.amount)),
        );
        profile.insert(
            "currency".to_string(),
            serde_json::json!(self.currency.clone()),
        );
        profile.insert(
            "debtor".to_string(),
            serde_json::json!(self
                .debtor
                .name
                .clone()
                .unwrap_or_else(|| self.debtor.account.clone())),
        );
        profile.insert(
            "creditor".to_string(),
            serde_json::json!(self
                .creditor
                .name
                .clone()
                .unwrap_or_else(|| self.creditor.account.clone())),
        );
        profile.insert(
            "debtor_account".to_string(),
            serde_json::json!(self.debtor.account.clone()),
        );
        profile.insert(
            "creditor_account".to_string(),
            serde_json::json!(self.creditor.account.clone()),
        );
        if let Some(agent) = self.debtor.agent.as_ref() {
            profile.insert("debtor_agent".to_string(), serde_json::json!(agent));
        }
        if let Some(agent) = self.creditor.agent.as_ref() {
            profile.insert("creditor_agent".to_string(), serde_json::json!(agent));
        }
        if let Some(settlement_date) = self
            .network_routing
            .as_ref()
            .and_then(|routing| routing.get("settlementDate"))
        {
            profile.insert(
                "settlement_date".to_string(),
                serde_json::json!(settlement_date),
            );
        }

        serde_json::to_string(&profile).map_err(|error| Fdc3Error::Serialization(error.to_string()))
    }

    pub fn to_json(&self) -> Result<String, Fdc3Error> {
        serde_json::to_string(self).map_err(|error| Fdc3Error::Serialization(error.to_string()))
    }

    pub fn from_json(value: &str) -> Result<Self, Fdc3Error> {
        serde_json::from_str(value).map_err(|error| Fdc3Error::Serialization(error.to_string()))
    }
}

impl Iso20022RoutingEvent {
    /// Converts this routing event into a standard FDC3 payment context.
    pub fn to_fdc3_payment(&self) -> Result<Fdc3Payment, Fdc3Error> {
        Fdc3Payment::from_iso20022(self)
    }

    /// Wraps the routing event in an FDC3 `fdc3.transactionResult`.
    pub fn to_fdc3_transaction_result(
        &self,
        status: Fdc3TransactionStatus,
        message: impl Into<String>,
    ) -> Result<Fdc3TransactionResult, Fdc3Error> {
        let context = serde_json::to_value(self)
            .map_err(|error| Fdc3Error::Serialization(error.to_string()))?;
        let result = Fdc3TransactionResult {
            context_type: FDC3_TRANSACTION_RESULT_CONTEXT_TYPE.to_string(),
            status,
            context: Some(context),
            message: Some(message.into()),
        };
        result.validate()?;
        Ok(result)
    }
}

impl Fdc3TransactionResult {
    pub fn validate(&self) -> Result<(), Fdc3Error> {
        if self.context_type != FDC3_TRANSACTION_RESULT_CONTEXT_TYPE {
            return Err(Fdc3Error::InvalidField(format!(
                "type must be {FDC3_TRANSACTION_RESULT_CONTEXT_TYPE}"
            )));
        }
        validate_optional_text("transaction result message", self.message.as_deref())?;
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, Fdc3Error> {
        serde_json::to_string(self).map_err(|error| Fdc3Error::Serialization(error.to_string()))
    }
}

fn amount_major(amount_minor: u64, scale: u32) -> Result<f64, Fdc3Error> {
    let scale = i32::try_from(scale)
        .map_err(|_| Fdc3Error::InvalidField("amount scale is not representable".to_string()))?;
    let amount = amount_minor as f64 / 10_f64.powi(scale);
    if !amount.is_finite() || amount <= 0.0 {
        return Err(Fdc3Error::InvalidField(
            "amount is not representable as a positive FDC3 number".to_string(),
        ));
    }
    Ok(amount)
}

fn format_amount(amount: f64) -> String {
    let mut value = format!("{amount}");
    if value.contains('e') || value.contains('E') {
        value = format!("{amount:.18}");
        while value.ends_with('0') {
            value.pop();
        }
        if value.ends_with('.') {
            value.pop();
        }
    }
    value
}

fn validate_party(label: &str, party: &Fdc3Party) -> Result<(), Fdc3Error> {
    if party.account.trim().is_empty() {
        return Err(Fdc3Error::InvalidField(format!(
            "{label}.account cannot be empty or whitespace"
        )));
    }
    validate_optional_text(&format!("{label}.name"), party.name.as_deref())?;
    validate_optional_text(&format!("{label}.account"), Some(party.account.as_str()))?;
    validate_optional_text(&format!("{label}.agent"), party.agent.as_deref())?;
    Ok(())
}

fn validate_id_map(label: &str, identifiers: &HashMap<String, String>) -> Result<(), Fdc3Error> {
    for (key, value) in identifiers {
        if key.trim().is_empty() {
            return Err(Fdc3Error::InvalidField(format!(
                "{label} contains an empty key"
            )));
        }
        validate_optional_text(&format!("{label}.{key}"), Some(key.as_str()))?;
        validate_optional_text(&format!("{label}.{key} value"), Some(value.as_str()))?;
    }
    Ok(())
}

fn validate_optional_text(label: &str, value: Option<&str>) -> Result<(), Fdc3Error> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() > MAX_FIELD_BYTES || value.chars().any(char::is_control) {
        return Err(Fdc3Error::InvalidField(format!(
            "{label} is invalid or exceeds {MAX_FIELD_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso20022::translate;

    const PACS008_WITH_ACCOUNTS: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>MSG-42</MsgId><IntrBkSttlmDt>2026-10-13</IntrBkSttlmDt></GrpHdr><CdtTrfTxInf><PmtId><InstrId>INSTR-7</InstrId><EndToEndId>E2E-9</EndToEndId><UETR>123e4567-e89b-12d3-a456-426614174000</UETR></PmtId><IntrBkSttlmAmt Ccy="ZAR">123.45</IntrBkSttlmAmt><Dbtr><Nm>Sender Bank</Nm><DbtrAcct><Id><IBAN>ZA12345678901234567890</IBAN></Id></DbtrAcct><DbtrAgt><FinInstnId><BICFI>BUKBGB22</BICFI></FinInstnId></DbtrAgt></Dbtr><Cdtr><Nm>Receiver Bank</Nm><CdtrAcct><Id><IBAN>ZA09876543210987654321</IBAN></Id></CdtrAcct><CdtrAgt><FinInstnId><BICFI>CHASUS33</BICFI></FinInstnId></CdtrAgt></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;

    #[test]
    fn maps_pacs008_with_accounts_to_fdc3_payment() {
        let event = translate(PACS008_WITH_ACCOUNTS.as_bytes()).expect("valid profile");
        assert_eq!(
            event.debtor_account.as_deref(),
            Some("ZA12345678901234567890")
        );
        assert_eq!(
            event.creditor_account.as_deref(),
            Some("ZA09876543210987654321")
        );
        assert_eq!(
            event.uetr.as_deref(),
            Some("123e4567-e89b-12d3-a456-426614174000")
        );

        let payment = event.to_fdc3_payment().expect("fdc3 payment");
        assert_eq!(payment.context_type, FDC3_PAYMENT_CONTEXT_TYPE);
        assert_eq!(
            payment.id.get("UETR").map(String::as_str),
            Some("123e4567-e89b-12d3-a456-426614174000")
        );
        assert_eq!(payment.amount, 123.45);
        assert_eq!(payment.currency, "ZAR");
        assert_eq!(payment.debtor.account, "ZA12345678901234567890");
        assert_eq!(payment.creditor.agent.as_deref(), Some("CHASUS33"));
        assert!(payment.validate().is_ok());
    }

    #[test]
    fn fails_closed_without_iso20022_accounts() {
        let xml = br#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>MSG-42</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="ZAR">10.00</IntrBkSttlmAmt><Dbtr><Nm>Alice</Nm></Dbtr><Cdtr><Nm>Bob</Nm></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;
        let event = translate(xml).expect("routing event remains valid");
        assert!(event.to_fdc3_payment().is_err());
    }

    #[test]
    fn round_trips_fdc3_payment_through_iso20022_profile() {
        let event = translate(PACS008_WITH_ACCOUNTS.as_bytes()).expect("valid profile");
        let payment = event.to_fdc3_payment().expect("fdc3 payment");
        let json = payment.to_iso20022_json().expect("ISO profile");
        let round_tripped = translate(json.as_bytes()).expect("round-trip profile");
        let payment_again = round_tripped.to_fdc3_payment().expect("round-trip payment");
        assert_eq!(payment_again.id, payment.id);
        assert_eq!(payment_again.amount, payment.amount);
        assert_eq!(payment_again.currency, payment.currency);
        assert_eq!(payment_again.debtor.account, payment.debtor.account);
        assert_eq!(payment_again.creditor.account, payment.creditor.account);
    }

    #[test]
    fn builds_fdc3_transaction_result_from_routing_event() {
        let event = translate(PACS008_WITH_ACCOUNTS.as_bytes()).expect("valid profile");
        let result = event
            .to_fdc3_transaction_result(Fdc3TransactionStatus::Created, "accepted")
            .expect("transaction result");
        assert_eq!(result.context_type, FDC3_TRANSACTION_RESULT_CONTEXT_TYPE);
        assert!(result.context.is_some());
        assert!(result.validate().is_ok());
    }
}
