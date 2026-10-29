//! Bounded ISO 20022 ingress translation.
//!
//! This module translates a deliberately small, documented profile into a
//! tokenless routing event. It is not an XSD validator and it does not assert
//! clearing, settlement, or Bitcoin finality.

use conxian_core::{ConxianError, ConxianResult};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ISO20022_ROUTING_SCHEMA: &str = "conxian.iso20022.routing.v1";
pub const MAX_ISO20022_PAYLOAD_BYTES: usize = 1024 * 1024;
const MAX_FIELD_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Iso20022MessageType {
    Pacs008,
    Pacs009,
    Pain001,
    Camt053,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Iso20022Format {
    Xml,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Iso20022RoutingEvent {
    pub schema: &'static str,
    pub message_type: Iso20022MessageType,
    pub message_id: String,
    pub instruction_id: Option<String>,
    pub end_to_end_id: Option<String>,
    pub amount_minor: u64,
    pub amount_scale: u32,
    pub currency: String,
    pub debtor: String,
    pub creditor: String,
    pub settlement_date: Option<String>,
    pub payload_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonProfile {
    message_type: Iso20022MessageType,
    message_id: String,
    instruction_id: Option<String>,
    end_to_end_id: Option<String>,
    amount: String,
    currency: String,
    debtor: String,
    creditor: String,
    settlement_date: Option<String>,
}

#[derive(Default)]
struct XmlProfile {
    message_type: Option<Iso20022MessageType>,
    message_id: Option<String>,
    instruction_id: Option<String>,
    end_to_end_id: Option<String>,
    amount: Option<String>,
    currency: Option<String>,
    debtor: Option<String>,
    creditor: Option<String>,
    settlement_date: Option<String>,
}

pub fn detect_format(payload: &[u8]) -> ConxianResult<Iso20022Format> {
    match payload
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
    {
        Some(b'<') => Ok(Iso20022Format::Xml),
        Some(b'{') => Ok(Iso20022Format::Json),
        _ => Err(compliance(
            "ISO 20022 payload must be an XML document or JSON object",
        )),
    }
}

pub fn translate(payload: &[u8]) -> ConxianResult<Iso20022RoutingEvent> {
    if payload.is_empty() {
        return Err(compliance("ISO 20022 payload cannot be empty"));
    }
    if payload.len() > MAX_ISO20022_PAYLOAD_BYTES {
        return Err(compliance("ISO 20022 payload exceeds the 1 MiB limit"));
    }

    let digest = hex::encode(Sha256::digest(payload));
    match detect_format(payload)? {
        Iso20022Format::Xml => translate_xml(payload, digest),
        Iso20022Format::Json => translate_json(payload, digest),
    }
}

fn translate_json(payload: &[u8], digest: String) -> ConxianResult<Iso20022RoutingEvent> {
    let profile: JsonProfile = serde_json::from_slice(payload)
        .map_err(|error| compliance(format!("invalid ISO 20022 JSON profile: {error}")))?;
    build_event(
        profile.message_type,
        profile.message_id,
        profile.instruction_id,
        profile.end_to_end_id,
        profile.amount,
        profile.currency,
        profile.debtor,
        profile.creditor,
        profile.settlement_date,
        digest,
    )
}

fn translate_xml(payload: &[u8], digest: String) -> ConxianResult<Iso20022RoutingEvent> {
    let xml = std::str::from_utf8(payload)
        .map_err(|_| compliance("ISO 20022 XML must be valid UTF-8"))?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut text_stack: Vec<String> = Vec::new();
    let mut amount_currency_stack: Vec<Option<String>> = Vec::new();
    let mut root_seen = false;
    let mut transaction_count = 0_u8;
    let mut profile = XmlProfile::default();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => {
                let name = local_name(element.name().as_ref())?;
                if path.is_empty() {
                    if name != "Document" {
                        return Err(compliance("ISO 20022 XML root must be Document"));
                    }
                    root_seen = true;
                    for attribute in element.attributes() {
                        let attribute = attribute.map_err(|error| {
                            compliance(format!("invalid XML attribute: {error}"))
                        })?;
                        if attribute.key.as_ref() == "xmlns" {
                            profile.message_type =
                                message_type_from_namespace(attribute.value.as_ref());
                        }
                    }
                }
                if name == "CdtTrfTxInf" {
                    transaction_count = transaction_count.saturating_add(1);
                    if transaction_count > 1 {
                        return Err(compliance(
                            "multiple ISO 20022 credit transfer transactions are not supported",
                        ));
                    }
                }
                let mut amount_currency = None;
                if matches!(name.as_str(), "IntrBkSttlmAmt" | "InstdAmt") {
                    for attribute in element.attributes() {
                        let attribute = attribute.map_err(|error| {
                            compliance(format!("invalid amount attribute: {error}"))
                        })?;
                        if attribute.key.as_ref() == "Ccy" {
                            amount_currency = Some(field(attribute.value.as_ref(), "currency")?);
                        }
                    }
                }
                path.push(name);
                text_stack.push(String::new());
                amount_currency_stack.push(amount_currency);
            }
            Ok(Event::Empty(element)) => {
                if path.is_empty() && element.name().as_ref() != "Document" {
                    return Err(compliance("ISO 20022 XML root must be Document"));
                }
            }
            Ok(Event::Text(text)) => {
                if let Some(current) = text_stack.last_mut() {
                    current.push_str(text.as_ref());
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(current) = text_stack.last_mut() {
                    current.push_str(text.as_ref());
                }
            }
            Ok(Event::End(_)) => {
                let value = text_stack.pop().unwrap_or_default();
                let amount_currency = amount_currency_stack.pop().flatten();
                capture_xml_value(&path, &value, amount_currency, &mut profile)?;
                path.pop();
            }
            Ok(Event::DocType(_)) => {
                return Err(compliance("DOCTYPE is not accepted in ISO 20022 ingress"));
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(compliance(format!("ISO 20022 XML syntax error: {error}")));
            }
            _ => {}
        }
        buffer.clear();
    }

    if !root_seen {
        return Err(compliance("ISO 20022 XML is missing the Document root"));
    }
    let message_type = profile
        .message_type
        .ok_or_else(|| compliance("unsupported or missing ISO 20022 namespace"))?;
    build_event(
        message_type,
        required(profile.message_id, "MsgId")?,
        profile.instruction_id,
        profile.end_to_end_id,
        required(profile.amount, "settlement amount")?,
        required(profile.currency, "settlement currency")?,
        required(profile.debtor, "debtor")?,
        required(profile.creditor, "creditor")?,
        profile.settlement_date,
        digest,
    )
}

fn capture_xml_value(
    path: &[String],
    raw: &str,
    amount_currency: Option<String>,
    profile: &mut XmlProfile,
) -> ConxianResult<()> {
    let Some(name) = path.last().map(String::as_str) else {
        return Ok(());
    };
    if !matches!(
        name,
        "MsgId"
            | "InstrId"
            | "EndToEndId"
            | "IntrBkSttlmAmt"
            | "InstdAmt"
            | "IntrBkSttlmDt"
            | "ReqdExctnDt"
            | "Nm"
    ) {
        return Ok(());
    }
    let value = field(raw, name)?;
    match name {
        "MsgId" if profile.message_id.is_none() => profile.message_id = Some(value),
        "InstrId" if profile.instruction_id.is_none() => profile.instruction_id = Some(value),
        "EndToEndId" if profile.end_to_end_id.is_none() => profile.end_to_end_id = Some(value),
        "IntrBkSttlmAmt" | "InstdAmt" if profile.amount.is_none() => {
            profile.amount = Some(value);
            profile.currency = amount_currency;
        }
        "IntrBkSttlmDt" | "ReqdExctnDt" if profile.settlement_date.is_none() => {
            profile.settlement_date = Some(value)
        }
        "Nm" if profile.debtor.is_none() && path.iter().any(|part| part == "Dbtr") => {
            profile.debtor = Some(value)
        }
        "Nm" if profile.creditor.is_none() && path.iter().any(|part| part == "Cdtr") => {
            profile.creditor = Some(value)
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_event(
    message_type: Iso20022MessageType,
    message_id: String,
    instruction_id: Option<String>,
    end_to_end_id: Option<String>,
    amount: String,
    currency: String,
    debtor: String,
    creditor: String,
    settlement_date: Option<String>,
    payload_sha256: String,
) -> ConxianResult<Iso20022RoutingEvent> {
    let (amount_minor, amount_scale) = decimal_to_minor_units(&amount)?;
    let currency = field(&currency.to_ascii_uppercase(), "currency")?;
    if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(compliance("currency must be a three-letter uppercase code"));
    }
    if message_id == "NOTPROVIDED" || message_id.is_empty() {
        return Err(compliance("MsgId must identify the source message"));
    }

    Ok(Iso20022RoutingEvent {
        schema: ISO20022_ROUTING_SCHEMA,
        message_type,
        message_id,
        instruction_id,
        end_to_end_id,
        amount_minor,
        amount_scale,
        currency,
        debtor,
        creditor,
        settlement_date,
        payload_sha256,
    })
}

fn decimal_to_minor_units(value: &str) -> ConxianResult<(u64, u32)> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') || value.starts_with('+') {
        return Err(compliance("amount must be an unsigned decimal"));
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fractional = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fractional
            .is_some_and(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(compliance("amount must be an unsigned decimal"));
    }
    let fractional = fractional.unwrap_or_default();
    if fractional.len() > 18 {
        return Err(compliance("amount scale exceeds 18 decimal places"));
    }
    let scale = u32::try_from(fractional.len())
        .map_err(|_| compliance("amount scale is not representable"))?;
    let factor = 10_u64
        .checked_pow(scale)
        .ok_or_else(|| compliance("amount scale overflow"))?;
    let whole = whole
        .parse::<u64>()
        .map_err(|_| compliance("amount exceeds u64 range"))?;
    let fraction = if fractional.is_empty() {
        0
    } else {
        fractional
            .parse::<u64>()
            .map_err(|_| compliance("amount fraction exceeds u64 range"))?
    };
    let minor = whole
        .checked_mul(factor)
        .and_then(|amount| amount.checked_add(fraction))
        .ok_or_else(|| compliance("amount exceeds u64 range"))?;
    Ok((minor, scale))
}

fn message_type_from_namespace(namespace: &str) -> Option<Iso20022MessageType> {
    if namespace.contains(":pacs.008.") {
        Some(Iso20022MessageType::Pacs008)
    } else if namespace.contains(":pacs.009.") {
        Some(Iso20022MessageType::Pacs009)
    } else if namespace.contains(":pain.001.") {
        Some(Iso20022MessageType::Pain001)
    } else if namespace.contains(":camt.053.") {
        Some(Iso20022MessageType::Camt053)
    } else {
        None
    }
}

fn local_name(name: &str) -> ConxianResult<String> {
    let name = name.rsplit(':').next().unwrap_or(name);
    field(name, "element name")
}

fn field(value: &str, name: &str) -> ConxianResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(compliance(format!("{name} cannot be empty")));
    }
    if value.len() > MAX_FIELD_BYTES || value.chars().any(char::is_control) {
        return Err(compliance(format!("{name} is invalid or too long")));
    }
    Ok(value.to_owned())
}

fn required(value: Option<String>, name: &str) -> ConxianResult<String> {
    value.ok_or_else(|| compliance(format!("ISO 20022 payload is missing {name}")))
}

fn compliance(message: impl Into<String>) -> ConxianError {
    ConxianError::Compliance(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_pacs008_values_without_simulated_defaults() {
        let xml = br#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>MSG-42</MsgId><IntrBkSttlmDt>2026-10-13</IntrBkSttlmDt></GrpHdr><CdtTrfTxInf><PmtId><InstrId>INSTR-7</InstrId><EndToEndId>E2E-9</EndToEndId></PmtId><IntrBkSttlmAmt Ccy="ZAR">123.45</IntrBkSttlmAmt><Dbtr><Nm>Sender Bank</Nm></Dbtr><Cdtr><Nm>Receiver Bank</Nm></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;
        let event = translate(xml).expect("valid profile");
        assert_eq!(event.message_id, "MSG-42");
        assert_eq!(event.amount_minor, 12_345);
        assert_eq!(event.amount_scale, 2);
        assert_eq!(event.currency, "ZAR");
        assert_eq!(event.debtor, "Sender Bank");
        assert_eq!(event.creditor, "Receiver Bank");
    }

    #[test]
    fn translates_json_profile() {
        let json = br#"{"message_type":"pain001","message_id":"MSG-7","instruction_id":"I-1","end_to_end_id":"E-1","amount":"10.005","currency":"usd","debtor":"Alice","creditor":"Bob","settlement_date":"2026-10-13"}"#;
        let event = translate(json).expect("valid profile");
        assert_eq!(event.amount_minor, 10_005);
        assert_eq!(event.amount_scale, 3);
        assert_eq!(event.currency, "USD");
    }

    #[test]
    fn rejects_doctype_and_negative_amounts() {
        assert!(translate(br#"<!DOCTYPE foo><Document/>"#).is_err());
        assert!(decimal_to_minor_units("-1.00").is_err());
    }

    #[test]
    fn binds_currency_to_the_selected_amount_element() {
        let xml = br#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>MSG-FX</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="USD">100.00</IntrBkSttlmAmt><InstdAmt Ccy="EUR">90.00</InstdAmt><Dbtr><Nm>Alice</Nm></Dbtr><Cdtr><Nm>Bob</Nm></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;
        let event = translate(xml).expect("valid single-transaction FX profile");
        assert_eq!(event.amount_minor, 10_000);
        assert_eq!(event.currency, "USD");
    }

    #[test]
    fn rejects_multiple_credit_transfer_transactions() {
        let xml = br#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>MSG-MULTI</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="USD">100.00</IntrBkSttlmAmt><Dbtr><Nm>Alice</Nm></Dbtr><Cdtr><Nm>Bob</Nm></Cdtr></CdtTrfTxInf><CdtTrfTxInf><IntrBkSttlmAmt Ccy="EUR">90.00</IntrBkSttlmAmt><Dbtr><Nm>Carol</Nm></Dbtr><Cdtr><Nm>Dave</Nm></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;
        assert!(translate(xml).is_err());
    }

    #[test]
    fn accumulates_text_fragments_and_cdata() {
        let fragmented = br#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>MSG-TEXT</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="USD">100.00</IntrBkSttlmAmt><Dbtr><Nm>Alice<!-- split --> Smith</Nm></Dbtr><Cdtr><Nm><![CDATA[Bob Jones]]></Nm></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;
        let event = translate(fragmented).expect("fragmented text profile");
        assert_eq!(event.debtor, "Alice Smith");
        assert_eq!(event.creditor, "Bob Jones");
    }
}
