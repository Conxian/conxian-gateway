use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::AppState;

/// Request to generate a camt.053 bank-to-customer statement
#[derive(Debug, Deserialize)]
pub struct Camt053Request {
    pub account_id: String,
    pub from_date: String,
    pub to_date: String,
    pub currency: String,
    pub include_transactions: bool,
}

/// Request to generate a camt.054 debit/credit notification
#[derive(Debug, Deserialize)]
pub struct Camt054Request {
    pub account_id: String,
    pub transaction_id: String,
    pub amount: String,
    pub currency: String,
    pub credit_debit_indicator: String,
    pub booking_date: String,
    pub value_date: String,
}

/// Response containing the serialized ISO 20022 XML
#[derive(Debug, Serialize)]
pub struct CamtResponse {
    pub message_id: String,
    pub message_type: String,
    pub xml_payload: String,
    pub created_at: String,
}

/// Generate a camt.053 bank statement (closing/periodic)
pub async fn generate_camt053(
    State(_state): State<AppState>,
    Json(payload): Json<Camt053Request>,
) -> Result<Json<CamtResponse>, (StatusCode, String)> {
    let message_id = format!("camt053-{}", uuid::Uuid::new_v4());
    info!(
        account_id = %payload.account_id,
        message_id = %message_id,
        "Generating camt.053 statement"
    );

    let xml = build_camt053_xml(&message_id, &payload)?;
    Ok(Json(CamtResponse {
        message_id,
        message_type: "camt.053.001.08".to_string(),
        xml_payload: xml,
        created_at: now_iso8601(),
    }))
}

/// Generate a camt.054 debit/credit notification
pub async fn generate_camt054(
    State(_state): State<AppState>,
    Json(payload): Json<Camt054Request>,
) -> Result<Json<CamtResponse>, (StatusCode, String)> {
    let message_id = format!("camt054-{}", uuid::Uuid::new_v4());
    info!(
        account_id = %payload.account_id,
        transaction_id = %payload.transaction_id,
        "Generating camt.054 notification"
    );

    let xml = build_camt054_xml(&message_id, &payload)?;
    Ok(Json(CamtResponse {
        message_id,
        message_type: "camt.054.001.08".to_string(),
        xml_payload: xml,
        created_at: now_iso8601(),
    }))
}

fn build_camt053_xml(message_id: &str, payload: &Camt053Request) -> Result<String, (StatusCode, String)> {
    use std::fmt::Write;
    let mut xml = String::new();
    writeln!(xml, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(xml, r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08">"#).unwrap();
    writeln!(xml, r#"  <BkToCstmrStmt>"#).unwrap();
    writeln!(xml, r#"    <GrpHdr>"#).unwrap();
    writeln!(xml, r#"      <MsgId>{message_id}</MsgId>"#).unwrap();
    writeln!(xml, r#"      <CreDtTm>{}</CreDtTm>"#, now_iso8601()).unwrap();
    writeln!(xml, r#"    </GrpHdr>"#).unwrap();
    writeln!(xml, r#"    <Stmt>"#).unwrap();
    writeln!(xml, r#"      <Id>{message_id}-stmt</Id>"#).unwrap();
    writeln!(xml, r#"      <Acct><Id><Othr><Id>{}</Id></Othr></Id></Acct>"#, payload.account_id).unwrap();
    writeln!(xml, r#"      <FrToDt>"#).unwrap();
    writeln!(xml, r#"        <FrDtTm>{}</FrDtTm>"#, payload.from_date).unwrap();
    writeln!(xml, r#"        <ToDtTm>{}</ToDtTm>"#, payload.to_date).unwrap();
    writeln!(xml, r#"      </FrToDt>"#).unwrap();
    writeln!(xml, r#"    </Stmt>"#).unwrap();
    writeln!(xml, r#"  </BkToCstmrStmt>"#).unwrap();
    writeln!(xml, r#"</Document>"#).unwrap();
    Ok(xml)
}

fn build_camt054_xml(message_id: &str, payload: &Camt054Request) -> Result<String, (StatusCode, String)> {
    use std::fmt::Write;
    let mut xml = String::new();
    writeln!(xml, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(xml, r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.054.001.08">"#).unwrap();
    writeln!(xml, r#"  <BkToCstmrDbtCdtNtfctn>"#).unwrap();
    writeln!(xml, r#"    <GrpHdr>"#).unwrap();
    writeln!(xml, r#"      <MsgId>{message_id}</MsgId>"#).unwrap();
    writeln!(xml, r#"      <CreDtTm>{}</CreDtTm>"#, now_iso8601()).unwrap();
    writeln!(xml, r#"    </GrpHdr>"#).unwrap();
    writeln!(xml, r#"    <Ntfctn>"#).unwrap();
    writeln!(xml, r#"      <Id>{message_id}-ntfctn</Id>"#).unwrap();
    writeln!(xml, r#"      <Acct><Id><Othr><Id>{}</Id></Othr></Id></Acct>"#, payload.account_id).unwrap();
    writeln!(xml, r#"      <Ntry>"#).unwrap();
    writeln!(xml, r#"        <Amt Ccy="{}">{}</Amt>"#, payload.currency, payload.amount).unwrap();
    writeln!(xml, r#"        <CdtDbtInd>{}</CdtDbtInd>"#, payload.credit_debit_indicator).unwrap();
    writeln!(xml, r#"        <BookgDt><Dt>{}</Dt></BookgDt>"#, payload.booking_date).unwrap();
    writeln!(xml, r#"        <ValDt><Dt>{}</Dt></ValDt>"#, payload.value_date).unwrap();
    writeln!(xml, r#"        <TxDtls>"#).unwrap();
    writeln!(xml, r#"          <Refs><AcctSvcrRef>{}</AcctSvcrRef></Refs>"#, payload.transaction_id).unwrap();
    writeln!(xml, r#"        </TxDtls>"#).unwrap();
    writeln!(xml, r#"      </Ntry>"#).unwrap();
    writeln!(xml, r#"    </Ntfctn>"#).unwrap();
    writeln!(xml, r#"  </BkToCstmrDbtCdtNtfctn>"#).unwrap();
    writeln!(xml, r#"</Document>"#).unwrap();
    Ok(xml)
}

fn now_iso8601() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // ISO 8601 basic format
    let secs = (now % 60).to_string();
    let mins = ((now / 60) % 60).to_string();
    let hours = ((now / 3600) % 24).to_string();
    let days = (now / 86400).to_string();
    format!("2026-06-28T{hours}:{mins}:{secs}")
}
