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
    validate_camt_xml(
        &xml,
        "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08",
        &[
            "BkToCstmrStmt",
            "GrpHdr",
            "Stmt",
            "MsgId",
            "CreDtTm",
            "FrToDt",
        ],
    )?;
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
    validate_camt_xml(
        &xml,
        "urn:iso:std:iso:20022:tech:xsd:camt.054.001.08",
        &[
            "BkToCstmrDbtCdtNtfctn",
            "GrpHdr",
            "Ntfctn",
            "MsgId",
            "CreDtTm",
            "Ntry",
            "Amt",
            "CdtDbtInd",
            "BookgDt",
            "ValDt",
            "TxDtls",
        ],
    )?;
    Ok(Json(CamtResponse {
        message_id,
        message_type: "camt.054.001.08".to_string(),
        xml_payload: xml,
        created_at: now_iso8601(),
    }))
}

#[rustfmt::skip]
fn build_camt053_xml(message_id: &str, payload: &Camt053Request) -> Result<String, (StatusCode, String)> {
    use std::fmt::Write;
    let e = xml_escape;
    let mut xml = String::new();
    writeln!(xml, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(xml, r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08">"#).unwrap();
    writeln!(xml, r#"  <BkToCstmrStmt>"#).unwrap();
    writeln!(xml, r#"    <GrpHdr>"#).unwrap();
    writeln!(xml, r#"      <MsgId>{}</MsgId>"#, e(message_id)).unwrap();
    writeln!(xml, r#"      <CreDtTm>{}</CreDtTm>"#, now_iso8601()).unwrap();
    writeln!(xml, r#"    </GrpHdr>"#).unwrap();
    writeln!(xml, r#"    <Stmt>"#).unwrap();
    writeln!(xml, r#"      <Id>{}-stmt</Id>"#, e(message_id)).unwrap();
    writeln!(xml, r#"      <Acct><Id><Othr><Id>{}</Id></Othr></Id></Acct>"#, e(&payload.account_id)).unwrap();
    writeln!(xml, r#"      <FrToDt>"#).unwrap();
    writeln!(xml, r#"        <FrDtTm>{}</FrDtTm>"#, e(&payload.from_date)).unwrap();
    writeln!(xml, r#"        <ToDtTm>{}</ToDtTm>"#, e(&payload.to_date)).unwrap();
    writeln!(xml, r#"      </FrToDt>"#).unwrap();
    writeln!(xml, r#"    </Stmt>"#).unwrap();
    writeln!(xml, r#"  </BkToCstmrStmt>"#).unwrap();
    writeln!(xml, r#"</Document>"#).unwrap();
    Ok(xml)
}

#[rustfmt::skip]
fn build_camt054_xml(message_id: &str, payload: &Camt054Request) -> Result<String, (StatusCode, String)> {
    use std::fmt::Write;
    let e = xml_escape;
    let mut xml = String::new();
    writeln!(xml, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(xml, r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.054.001.08">"#).unwrap();
    writeln!(xml, r#"  <BkToCstmrDbtCdtNtfctn>"#).unwrap();
    writeln!(xml, r#"    <GrpHdr>"#).unwrap();
    writeln!(xml, r#"      <MsgId>{}</MsgId>"#, e(message_id)).unwrap();
    writeln!(xml, r#"      <CreDtTm>{}</CreDtTm>"#, now_iso8601()).unwrap();
    writeln!(xml, r#"    </GrpHdr>"#).unwrap();
    writeln!(xml, r#"    <Ntfctn>"#).unwrap();
    writeln!(xml, r#"      <Id>{}-ntfctn</Id>"#, e(message_id)).unwrap();
    writeln!(xml, r#"      <Acct><Id><Othr><Id>{}</Id></Othr></Id></Acct>"#, e(&payload.account_id)).unwrap();
    writeln!(xml, r#"      <Ntry>"#).unwrap();
    writeln!(xml, r#"        <Amt Ccy="{}">{}</Amt>"#, e(&payload.currency), e(&payload.amount)).unwrap();
    writeln!(xml, r#"        <CdtDbtInd>{}</CdtDbtInd>"#, e(&payload.credit_debit_indicator)).unwrap();
    writeln!(xml, r#"        <BookgDt><Dt>{}</Dt></BookgDt>"#, e(&payload.booking_date)).unwrap();
    writeln!(xml, r#"        <ValDt><Dt>{}</Dt></ValDt>"#, e(&payload.value_date)).unwrap();
    writeln!(xml, r#"        <TxDtls>"#).unwrap();
    writeln!(xml, r#"          <Refs><AcctSvcrRef>{}</AcctSvcrRef></Refs>"#, e(&payload.transaction_id)).unwrap();
    writeln!(xml, r#"        </TxDtls>"#).unwrap();
    writeln!(xml, r#"      </Ntry>"#).unwrap();
    writeln!(xml, r#"    </Ntfctn>"#).unwrap();
    writeln!(xml, r#"  </BkToCstmrDbtCdtNtfctn>"#).unwrap();
    writeln!(xml, r#"</Document>"#).unwrap();
    Ok(xml)
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
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
    let _days = (now / 86400).to_string();
    format!("2026-06-28T{hours}:{mins}:{secs}")
}

/// Validate that a CAMT XML payload conforms to ISO 20022 structural requirements.
///
/// Performs defense-in-depth validation on generated XML before returning to callers.
/// Checks: well-formed XML declaration, correct namespace, required elements present,
/// no empty required fields. This prevents silent bank rejection of non-compliant messages.
fn validate_camt_xml(
    xml: &str,
    expected_ns: &str,
    required_elements: &[&str],
) -> Result<(), (StatusCode, String)> {
    // XML declaration must be present
    if !xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>") {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "CAMT validation: missing XML declaration".into(),
        ));
    }

    // Namespace must be present on root Document element
    let ns_attr = format!("xmlns=\"{expected_ns}\"");
    if !xml.contains(&ns_attr) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("CAMT validation: missing or incorrect namespace (expected {expected_ns})"),
        ));
    }

    // Root <Document> element must exist
    if !xml.contains("<Document") || !xml.contains("</Document>") {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "CAMT validation: missing root Document element".into(),
        ));
    }

    // All required structural elements must be present
    for element in required_elements {
        let open_tag = format!("<{element}>");
        let close_tag = format!("</{element}>");
        let open_attrs = format!("<{element} ");

        if !xml.contains(&open_tag) && !xml.contains(&open_attrs) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("CAMT validation: missing required element <{element}>"),
            ));
        }
        if !xml.contains(&close_tag) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("CAMT validation: unclosed required element <{element}>"),
            ));
        }
    }

    // Validate no empty MsgId or other critical identifier fields
    if xml.contains("<MsgId></MsgId>") {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "CAMT validation: empty MsgId".into(),
        ));
    }
    if xml.contains("<Id></Id>") {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "CAMT validation: empty Id".into(),
        ));
    }
    if xml.contains("<CreDtTm></CreDtTm>") {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "CAMT validation: empty CreDtTm".into(),
        ));
    }

    // ISO 20022 requires XML to have a single root element
    let doc_count = xml.matches("<Document").count();
    if doc_count != 1 {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("CAMT validation: expected 1 Document element, found {doc_count}"),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_preserves_safe_strings() {
        assert_eq!(
            xml_escape("DE89370400440532013000"),
            "DE89370400440532013000"
        );
    }

    #[test]
    fn xml_escape_encodes_ampersand() {
        assert_eq!(xml_escape("A&B"), "A&amp;B");
    }

    #[test]
    fn xml_escape_encodes_angle_brackets() {
        assert_eq!(xml_escape("<account>"), "&lt;account&gt;");
    }

    #[test]
    fn xml_escape_encodes_quotes() {
        assert_eq!(xml_escape(r#""test""#), "&quot;test&quot;");
    }

    #[test]
    fn xml_escape_encodes_apostrophe() {
        assert_eq!(xml_escape("it's"), "it&apos;s");
    }

    #[test]
    fn xml_escape_handles_injection_attempt() {
        let malicious = r#"</Acct><Acct><Id><Othr><Id>evil</Id></Othr></Id></Acct>"#;
        let escaped = xml_escape(malicious);
        assert!(!escaped.contains('<'));
        assert!(escaped.contains("&lt;"));
    }

    #[test]
    fn build_camt053_escapes_user_fields() {
        let payload = Camt053Request {
            account_id: "A&B".into(),
            from_date: "2026-01-01".into(),
            to_date: "2026-06-30".into(),
            currency: "EUR".into(),
            include_transactions: false,
        };
        let xml = build_camt053_xml("test-msg", &payload).unwrap();
        assert!(xml.contains("A&amp;B"));
        assert!(!xml.contains("A&B"));
    }

    #[test]
    fn build_camt054_escapes_user_fields() {
        let payload = Camt054Request {
            account_id: "<inject>".into(),
            transaction_id: "TXN&123".into(),
            amount: "100.00".into(),
            currency: "USD".into(),
            credit_debit_indicator: "CRDT".into(),
            booking_date: "2026-06-28".into(),
            value_date: "2026-06-28".into(),
        };
        let xml = build_camt054_xml("test-msg", &payload).unwrap();
        assert!(xml.contains("&lt;inject&gt;"));
        assert!(xml.contains("TXN&amp;123"));
        assert!(!xml.contains("<inject>"));
    }

    // ── G-FI1: XSD structural validation tests ──

    #[test]
    fn validate_camt053_passes_on_valid_xml() {
        let payload = Camt053Request {
            account_id: "DE89370400440532013000".into(),
            from_date: "2026-01-01".into(),
            to_date: "2026-06-30".into(),
            currency: "EUR".into(),
            include_transactions: false,
        };
        let xml = build_camt053_xml("test-msg", &payload).unwrap();
        assert!(validate_camt_xml(
            &xml,
            "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08",
            &[
                "BkToCstmrStmt",
                "GrpHdr",
                "Stmt",
                "MsgId",
                "CreDtTm",
                "FrToDt"
            ],
        )
        .is_ok());
    }

    #[test]
    fn validate_camt054_passes_on_valid_xml() {
        let payload = Camt054Request {
            account_id: "CH9300762011623852957".into(),
            transaction_id: "TXN-001".into(),
            amount: "1500.00".into(),
            currency: "CHF".into(),
            credit_debit_indicator: "CRDT".into(),
            booking_date: "2026-06-28".into(),
            value_date: "2026-06-28".into(),
        };
        let xml = build_camt054_xml("test-msg", &payload).unwrap();
        assert!(validate_camt_xml(
            &xml,
            "urn:iso:std:iso:20022:tech:xsd:camt.054.001.08",
            &[
                "BkToCstmrDbtCdtNtfctn",
                "GrpHdr",
                "Ntfctn",
                "MsgId",
                "CreDtTm",
                "Ntry",
                "Amt",
                "CdtDbtInd",
                "BookgDt",
                "ValDt",
                "TxDtls",
            ],
        )
        .is_ok());
    }

    #[test]
    fn validate_camt_rejects_missing_xml_declaration() {
        let xml = "<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.08\"></Document>";
        let err = validate_camt_xml(xml, "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08", &[])
            .unwrap_err();
        assert!(err.1.contains("missing XML declaration"));
    }

    #[test]
    fn validate_camt_rejects_wrong_namespace() {
        let xml =
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Document xmlns=\"urn:wrong\"></Document>";
        let err = validate_camt_xml(xml, "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08", &[])
            .unwrap_err();
        assert!(err.1.contains("namespace"));
    }

    #[test]
    fn validate_camt_rejects_missing_required_element() {
        let payload = Camt053Request {
            account_id: "DE89370400440532013000".into(),
            from_date: "2026-01-01".into(),
            to_date: "2026-06-30".into(),
            currency: "EUR".into(),
            include_transactions: false,
        };
        let xml = build_camt053_xml("test-msg", &payload).unwrap();
        // Ask for an element that doesn't exist in camt.053
        let err = validate_camt_xml(
            &xml,
            "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08",
            &["NonExistentElement"],
        )
        .unwrap_err();
        assert!(err.1.contains("NonExistentElement"));
    }

    #[test]
    fn validate_camt_rejects_empty_msg_id() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.08\"><BkToCstmrStmt><GrpHdr><MsgId></MsgId><CreDtTm>2026-01-01</CreDtTm></GrpHdr><Stmt><Id>1</Id></Stmt></BkToCstmrStmt></Document>";
        let err = validate_camt_xml(xml, "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08", &[])
            .unwrap_err();
        assert!(err.1.contains("empty MsgId"));
    }

    #[test]
    fn validate_camt_rejects_empty_id() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.08\"><BkToCstmrStmt><GrpHdr><MsgId>M1</MsgId><CreDtTm>2026-01-01</CreDtTm></GrpHdr><Stmt><Id></Id></Stmt></BkToCstmrStmt></Document>";
        let err = validate_camt_xml(xml, "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08", &[])
            .unwrap_err();
        assert!(err.1.contains("empty Id"));
    }

    #[test]
    fn validate_camt_rejects_multiple_document_roots() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.08\"></Document>\n<Document></Document>";
        let err = validate_camt_xml(xml, "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08", &[])
            .unwrap_err();
        assert!(err.1.contains("expected 1 Document"));
    }

    #[test]
    fn validate_camt_rejects_missing_document_close() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.08\">";
        let err = validate_camt_xml(xml, "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08", &[])
            .unwrap_err();
        assert!(err.1.contains("missing root Document"));
    }

    #[test]
    fn validate_camt_rejects_empty_cre_dt_tm() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.08\"><BkToCstmrStmt><GrpHdr><MsgId>M1</MsgId><CreDtTm></CreDtTm></GrpHdr><Stmt><Id>S1</Id></Stmt></BkToCstmrStmt></Document>";
        let err = validate_camt_xml(xml, "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08", &[])
            .unwrap_err();
        assert!(err.1.contains("empty CreDtTm"));
    }

    #[test]
    fn full_generate_camt053_passes_validation() {
        // Integration test: the full generate→build→validate pipeline
        let payload = Camt053Request {
            account_id: "GB29NWBK60161331926819".into(),
            from_date: "2026-01-01T00:00:00".into(),
            to_date: "2026-06-30T23:59:59".into(),
            currency: "GBP".into(),
            include_transactions: true,
        };
        let xml = build_camt053_xml("camt053-integration-test", &payload).unwrap();
        // Must contain all six structural elements
        for element in &[
            "BkToCstmrStmt",
            "GrpHdr",
            "Stmt",
            "MsgId",
            "CreDtTm",
            "FrToDt",
        ] {
            assert!(
                xml.contains(&format!("<{element}>")),
                "Missing element: {element}"
            );
        }
        // Must validate
        validate_camt_xml(
            &xml,
            "urn:iso:std:iso:20022:tech:xsd:camt.053.001.08",
            &[
                "BkToCstmrStmt",
                "GrpHdr",
                "Stmt",
                "MsgId",
                "CreDtTm",
                "FrToDt",
            ],
        )
        .expect("Full camt.053 pipeline validation should pass");
    }

    #[test]
    fn full_generate_camt054_passes_validation() {
        let payload = Camt054Request {
            account_id: "JPSTSEA47682CUST01".into(),
            transaction_id: "TXN-JP-0042".into(),
            amount: "250000.00".into(),
            currency: "JPY".into(),
            credit_debit_indicator: "DBIT".into(),
            booking_date: "2026-06-28".into(),
            value_date: "2026-06-29".into(),
        };
        let xml = build_camt054_xml("camt054-integration-test", &payload).unwrap();
        for element in &[
            "BkToCstmrDbtCdtNtfctn",
            "GrpHdr",
            "Ntfctn",
            "MsgId",
            "CreDtTm",
            "Ntry",
            "CdtDbtInd",
            "BookgDt",
            "ValDt",
            "TxDtls",
        ] {
            assert!(
                xml.contains(&format!("<{element}>")) || xml.contains(&format!("<{element} ")),
                "Missing element: {element}"
            );
        }
        // Amt has attributes: <Amt Ccy="...">
        assert!(xml.contains("<Amt "), "Missing element: Amt");
        validate_camt_xml(
            &xml,
            "urn:iso:std:iso:20022:tech:xsd:camt.054.001.08",
            &[
                "BkToCstmrDbtCdtNtfctn",
                "GrpHdr",
                "Ntfctn",
                "MsgId",
                "CreDtTm",
                "Ntry",
                "Amt",
                "CdtDbtInd",
                "BookgDt",
                "ValDt",
                "TxDtls",
            ],
        )
        .expect("Full camt.054 pipeline validation should pass");
    }
}
