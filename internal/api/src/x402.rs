use crate::{lightning::LightningAdapterError, AppState};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use base64::{
    engine::general_purpose::{
        STANDARD as BASE64_STANDARD, URL_SAFE as BASE64_URL_SAFE, URL_SAFE_NO_PAD,
    },
    Engine,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{info, warn};

const PAYMENT_REQUIRED_HEADER: &str = "payment-required";
const PAYMENT_SIGNATURE_HEADER: &str = "payment-signature";
const LEGACY_PAYMENT_HEADER: &str = "x-402-payment";
const PAYMENT_REQUIRED_ALIASES: &[&str] = &[PAYMENT_REQUIRED_HEADER, "x-payment-required"];
const PAYMENT_SIGNATURE_ALIASES: &[&str] = &[PAYMENT_SIGNATURE_HEADER, "x-payment"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X402PaymentPayload {
    pub amount: u128,
    pub asset: String,
    pub challenge: String,
    pub expiry: u64,
    pub proof_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402ParseError {
    MissingHeader {
        header: &'static str,
    },
    MissingField {
        field: &'static str,
    },
    MalformedHeader {
        header: &'static str,
        detail: &'static str,
    },
    InvalidField {
        field: &'static str,
        detail: &'static str,
    },
}

impl X402ParseError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::MissingHeader { .. } | Self::MissingField { .. } => StatusCode::PAYMENT_REQUIRED,
            Self::MalformedHeader { .. } | Self::InvalidField { .. } => StatusCode::BAD_REQUEST,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingHeader { .. } => "x402_missing_header",
            Self::MissingField { .. } => "x402_missing_field",
            Self::MalformedHeader { .. } => "x402_malformed_header",
            Self::InvalidField { .. } => "x402_invalid_field",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::MissingHeader { header } => format!("Missing required x402 header: {header}"),
            Self::MissingField { field } => format!("Missing required x402 field: {field}"),
            Self::MalformedHeader { header, detail } => {
                format!("Malformed x402 header {header}: {detail}")
            }
            Self::InvalidField { field, detail } => {
                format!("Invalid x402 field {field}: {detail}")
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct LegacyX402Payload {
    amount_satoshi: Option<u64>,
    amount: Option<u128>,
    asset: String,
    challenge: String,
    expiry: u64,
    proof_ref: Option<String>,
    proof_refs: Option<Vec<String>>,
}

pub async fn x402_filter(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let path = req.uri().path().to_string();
    let requires_payment = is_strictly_protected_path(&path);
    let has_any_header = has_any_x402_header(req.headers());

    if !requires_payment && !has_any_header {
        return Ok(next.run(req).await);
    }

    if requires_payment && !has_any_header {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({
                "error": "Payment Required",
                "code": "x402_required",
                "challenge": uuid::Uuid::new_v4().to_string(),
                "amount_satoshi": 1000,
                "asset": "sBTC"
            })),
        ));
    }

    let payload = match parse_gateway_x402_payload(req.headers()) {
        Ok(payload) => payload,
        Err(error) => {
            warn!(
                path = %path,
                status = %error.status_code(),
                code = %error.code(),
                "x402 parse rejected: {}",
                error.message()
            );
            return Err((
                error.status_code(),
                Json(json!({
                    "error": error.message(),
                    "code": error.code(),
                })),
            ));
        }
    };

    match state.lightning.execute_payment(&payload).await {
        Ok(receipt) => {
            info!(
                path = %path,
                challenge = %receipt.challenge,
                amount = %receipt.settled_amount,
                proof = %receipt.proof,
                "x402 payment validated via canonical Lightning adapter"
            );
            req.extensions_mut().insert(payload);
            req.extensions_mut().insert(receipt);
            Ok(next.run(req).await)
        }
        Err(error) => {
            warn!(
                path = %path,
                status = %error.status_code(),
                code = %error.code(),
                "x402 lightning execution rejected: {}",
                error.message()
            );
            Err(lightning_error_response(error))
        }
    }
}

fn lightning_error_response(error: LightningAdapterError) -> (StatusCode, Json<Value>) {
    (
        error.status_code(),
        Json(json!({
            "error": error.message(),
            "code": error.code(),
        })),
    )
}

fn has_any_x402_header(headers: &HeaderMap) -> bool {
    headers.contains_key(LEGACY_PAYMENT_HEADER)
        || PAYMENT_REQUIRED_ALIASES
            .iter()
            .any(|header| headers.contains_key(*header))
        || PAYMENT_SIGNATURE_ALIASES
            .iter()
            .any(|header| headers.contains_key(*header))
}

pub fn is_strictly_protected_path(path: &str) -> bool {
    path.contains("/settle") || path.contains("/ingress/") || path.contains("/erp/sync")
}

pub fn parse_gateway_x402_payload(
    headers: &HeaderMap,
) -> Result<X402PaymentPayload, X402ParseError> {
    if let Some(raw) = headers.get(LEGACY_PAYMENT_HEADER) {
        let token = raw.to_str().map_err(|_| X402ParseError::MalformedHeader {
            header: LEGACY_PAYMENT_HEADER,
            detail: "header value must be valid UTF-8",
        })?;

        let trimmed = token.trim();
        if trimmed.is_empty() {
            return Err(X402ParseError::MalformedHeader {
                header: LEGACY_PAYMENT_HEADER,
                detail: "header must not be empty",
            });
        }

        return parse_legacy_x402_payload(trimmed);
    }

    parse_x402_payload(headers)
}

fn parse_legacy_x402_payload(raw: &str) -> Result<X402PaymentPayload, X402ParseError> {
    if raw.starts_with("proof-") || raw.starts_with("test-pay-") || raw.starts_with("preimage-") {
        let now = now_unix_secs();
        return Ok(X402PaymentPayload {
            amount: 1_000,
            asset: "sBTC".to_string(),
            challenge: format!("legacy-{}", uuid::Uuid::new_v4()),
            expiry: now + 300,
            proof_refs: vec![raw.to_string()],
        });
    }

    let typed: LegacyX402Payload =
        serde_json::from_str(raw).map_err(|_| X402ParseError::MalformedHeader {
            header: LEGACY_PAYMENT_HEADER,
            detail: "expected proof token or JSON payload",
        })?;

    let amount = typed
        .amount
        .or(typed.amount_satoshi.map(u128::from))
        .ok_or(X402ParseError::MissingField { field: "amount" })?;

    let mut proof_refs = Vec::new();
    if let Some(proof_ref) = typed.proof_ref {
        if !proof_ref.trim().is_empty() {
            proof_refs.push(proof_ref.trim().to_string());
        }
    }

    if let Some(values) = typed.proof_refs {
        for value in values.into_iter().map(|value| value.trim().to_string()) {
            if !value.is_empty() {
                proof_refs.push(value);
            }
        }
    }

    if proof_refs.is_empty() {
        return Err(X402ParseError::MissingField {
            field: "proof_refs",
        });
    }

    Ok(X402PaymentPayload {
        amount,
        asset: typed.asset,
        challenge: typed.challenge,
        expiry: typed.expiry,
        proof_refs,
    })
}

pub fn parse_x402_payload(headers: &HeaderMap) -> Result<X402PaymentPayload, X402ParseError> {
    // Canonical x402 field mapping with local/legacy compatibility:
    // - amount: accepts[0].amount | accepts[0].maxAmountRequired | payload.authorization.value
    // - asset: accepts[0].asset | payload.authorization.asset
    // - challenge: challenge | nonce | payload.authorization.nonce
    // - expiry: validBefore | expiry | accepts[0].maxTimeoutSeconds (derived absolute unix time)
    // - proof refs: signature | payload.transaction | proofRef/proofRefs
    let payment_required_header =
        read_header(headers, PAYMENT_REQUIRED_ALIASES, PAYMENT_REQUIRED_HEADER)?;
    let payment_signature_header =
        read_header(headers, PAYMENT_SIGNATURE_ALIASES, PAYMENT_SIGNATURE_HEADER)?;

    let payment_required = parse_header_json(payment_required_header, PAYMENT_REQUIRED_HEADER)?;
    let payment_signature = parse_header_json(payment_signature_header, PAYMENT_SIGNATURE_HEADER)?;

    let amount = parse_amount(headers, &payment_required, &payment_signature)?;
    let asset = parse_asset(headers, &payment_required, &payment_signature)?;
    let challenge = parse_challenge(headers, &payment_required, &payment_signature)?;
    let expiry = parse_expiry(headers, &payment_required, &payment_signature)?;
    let proof_refs = parse_proof_refs(headers, &payment_required, &payment_signature)?;

    Ok(X402PaymentPayload {
        amount,
        asset,
        challenge,
        expiry,
        proof_refs,
    })
}

fn parse_amount(
    headers: &HeaderMap,
    payment_required: &Value,
    payment_signature: &Value,
) -> Result<u128, X402ParseError> {
    let amount_raw = first_string([
        header_string(headers, &["x402-amount", "payment-required-amount"]),
        value_to_string(payment_required.get("amount")),
        first_accept_value(payment_required, "amount"),
        first_accept_value(payment_required, "maxAmountRequired"),
        value_to_string(payment_signature.pointer("/payload/authorization/value")),
        value_to_string(payment_signature.get("amount")),
    ])
    .ok_or(X402ParseError::MissingField { field: "amount" })?;

    amount_raw
        .parse::<u128>()
        .map_err(|_| X402ParseError::InvalidField {
            field: "amount",
            detail: "must be an unsigned integer",
        })
}

fn parse_asset(
    headers: &HeaderMap,
    payment_required: &Value,
    payment_signature: &Value,
) -> Result<String, X402ParseError> {
    first_string([
        header_string(headers, &["x402-asset", "payment-required-asset"]),
        value_to_string(payment_required.get("asset")),
        first_accept_value(payment_required, "asset"),
        value_to_string(payment_signature.pointer("/payload/authorization/asset")),
        value_to_string(payment_signature.get("asset")),
    ])
    .ok_or(X402ParseError::MissingField { field: "asset" })
}

fn parse_challenge(
    headers: &HeaderMap,
    payment_required: &Value,
    payment_signature: &Value,
) -> Result<String, X402ParseError> {
    first_string([
        header_string(headers, &["x402-challenge", "payment-required-challenge"]),
        value_to_string(payment_required.get("challenge")),
        value_to_string(payment_required.get("nonce")),
        value_to_string(payment_required.pointer("/extensions/sign-in-with-x/info/nonce")),
        value_to_string(payment_signature.pointer("/payload/authorization/nonce")),
        value_to_string(payment_signature.pointer("/payload/nonce")),
        value_to_string(payment_signature.get("challenge")),
    ])
    .ok_or(X402ParseError::MissingField { field: "challenge" })
}

fn parse_expiry(
    headers: &HeaderMap,
    payment_required: &Value,
    payment_signature: &Value,
) -> Result<u64, X402ParseError> {
    if let Some(raw_expiry) = first_string([
        header_string(headers, &["x402-expiry", "payment-required-expiry"]),
        value_to_string(payment_required.get("expiry")),
        value_to_string(payment_required.get("expires_at")),
        value_to_string(payment_required.get("expiresAt")),
        value_to_string(payment_required.get("validBefore")),
        value_to_string(payment_signature.pointer("/payload/authorization/validBefore")),
        value_to_string(payment_signature.get("expiry")),
    ]) {
        return raw_expiry
            .parse::<u64>()
            .map_err(|_| X402ParseError::InvalidField {
                field: "expiry",
                detail: "must be a unix timestamp in seconds",
            });
    }

    if let Some(timeout_value) = first_accept_value(payment_required, "maxTimeoutSeconds") {
        let timeout = timeout_value
            .parse::<u64>()
            .map_err(|_| X402ParseError::InvalidField {
                field: "expiry",
                detail: "maxTimeoutSeconds must be an unsigned integer",
            })?;

        let now = now_unix_secs();

        return now
            .checked_add(timeout)
            .ok_or(X402ParseError::InvalidField {
                field: "expiry",
                detail: "calculated expiry overflow",
            });
    }

    Err(X402ParseError::MissingField { field: "expiry" })
}

fn parse_proof_refs(
    headers: &HeaderMap,
    payment_required: &Value,
    payment_signature: &Value,
) -> Result<Vec<String>, X402ParseError> {
    let mut proof_refs = HashSet::new();

    if let Some(raw) = header_string(headers, &["x402-proof-ref", "x402-proof-refs"]) {
        for value in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            proof_refs.insert(value.to_string());
        }
    }

    for value in [
        value_to_string(payment_signature.get("signature")),
        value_to_string(payment_signature.pointer("/payload/signature")),
        value_to_string(payment_signature.pointer("/payload/transaction")),
        value_to_string(payment_signature.get("transaction")),
        value_to_string(payment_signature.get("proofRef")),
        value_to_string(payment_required.get("proofRef")),
    ]
    .into_iter()
    .flatten()
    {
        proof_refs.insert(value);
    }

    if let Some(values) = payment_signature.get("proofRefs").and_then(Value::as_array) {
        for value in values
            .iter()
            .filter_map(|value| value_to_string(Some(value)))
        {
            proof_refs.insert(value);
        }
    }

    if let Some(values) = payment_required.get("proofRefs").and_then(Value::as_array) {
        for value in values
            .iter()
            .filter_map(|value| value_to_string(Some(value)))
        {
            proof_refs.insert(value);
        }
    }

    if proof_refs.is_empty() {
        return Err(X402ParseError::MissingField {
            field: "proof_refs",
        });
    }

    let mut refs: Vec<_> = proof_refs.into_iter().collect();
    refs.sort();
    Ok(refs)
}

fn read_header<'a>(
    headers: &'a HeaderMap,
    names: &[&'static str],
    canonical: &'static str,
) -> Result<&'a str, X402ParseError> {
    for name in names {
        if let Some(raw) = headers.get(*name) {
            let value = raw.to_str().map_err(|_| X402ParseError::MalformedHeader {
                header: canonical,
                detail: "header value must be valid UTF-8",
            })?;
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(X402ParseError::MalformedHeader {
                    header: canonical,
                    detail: "header must not be empty",
                });
            }
            return Ok(trimmed);
        }
    }

    Err(X402ParseError::MissingHeader { header: canonical })
}

fn parse_header_json(raw: &str, header: &'static str) -> Result<Value, X402ParseError> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return Ok(value);
    }

    for bytes in [
        BASE64_STANDARD.decode(raw),
        BASE64_URL_SAFE.decode(raw),
        URL_SAFE_NO_PAD.decode(raw),
    ]
    .into_iter()
    .flatten()
    {
        if let Ok(as_str) = std::str::from_utf8(&bytes) {
            if let Ok(value) = serde_json::from_str::<Value>(as_str) {
                return Ok(value);
            }
        }
    }

    Err(X402ParseError::MalformedHeader {
        header,
        detail: "expected JSON or base64-encoded JSON",
    })
}

fn first_accept_value(value: &Value, field: &str) -> Option<String> {
    value
        .get("accepts")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get(field))
        .and_then(|value| value_to_string(Some(value)))
}

fn header_string(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|raw| raw.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn first_string<const N: usize>(values: [Option<String>; N]) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
}

fn value_to_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    fn build_headers(payment_required: Value, payment_signature: Value) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            PAYMENT_REQUIRED_HEADER,
            HeaderValue::from_str(
                &BASE64_STANDARD.encode(serde_json::to_vec(&payment_required).unwrap()),
            )
            .unwrap(),
        );
        headers.insert(
            PAYMENT_SIGNATURE_HEADER,
            HeaderValue::from_str(
                &BASE64_STANDARD.encode(serde_json::to_vec(&payment_signature).unwrap()),
            )
            .unwrap(),
        );
        headers
    }

    #[test]
    fn parse_x402_payload_accepts_base64_json_headers() {
        let payment_required = json!({
            "accepts": [
                {
                    "amount": "1000",
                    "asset": "sBTC",
                    "maxTimeoutSeconds": 600
                }
            ],
            "challenge": "challenge-123"
        });

        let payment_signature = json!({
            "payload": {
                "authorization": {
                    "nonce": "nonce-abc",
                    "validBefore": "2000000000"
                },
                "transaction": "0xdeadbeef"
            },
            "signature": "0xsig"
        });

        let headers = build_headers(payment_required, payment_signature);
        let parsed = parse_x402_payload(&headers).unwrap();

        assert_eq!(parsed.amount, 1000);
        assert_eq!(parsed.asset, "sBTC");
        assert_eq!(parsed.challenge, "challenge-123");
        assert_eq!(parsed.expiry, 2_000_000_000);
        assert_eq!(
            parsed.proof_refs,
            vec!["0xdeadbeef".to_string(), "0xsig".to_string()]
        );
    }

    #[test]
    fn parse_x402_payload_maps_legacy_amount_and_timeout() {
        let payment_required = json!({
            "accepts": [
                {
                    "maxAmountRequired": "42",
                    "asset": "USD",
                    "maxTimeoutSeconds": "120"
                }
            ]
        });

        let payment_signature = json!({
            "payload": {
                "authorization": {
                    "nonce": "nonce-legacy"
                }
            },
            "signature": "proof-legacy"
        });

        let before = now_unix_secs();
        let headers = build_headers(payment_required, payment_signature);
        let parsed = parse_x402_payload(&headers).unwrap();

        assert_eq!(parsed.amount, 42);
        assert_eq!(parsed.asset, "USD");
        assert_eq!(parsed.challenge, "nonce-legacy");
        assert!(parsed.expiry >= before + 120);
        assert_eq!(parsed.proof_refs, vec!["proof-legacy".to_string()]);
    }

    #[test]
    fn parse_x402_payload_rejects_missing_payment_required_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            PAYMENT_SIGNATURE_HEADER,
            HeaderValue::from_static(r#"{"signature":"proof"}"#),
        );

        let error = parse_x402_payload(&headers).unwrap_err();
        assert_eq!(
            error,
            X402ParseError::MissingHeader {
                header: PAYMENT_REQUIRED_HEADER
            }
        );
        assert_eq!(error.status_code(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(error.code(), "x402_missing_header");
    }

    #[test]
    fn parse_x402_payload_rejects_malformed_payment_required_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            PAYMENT_REQUIRED_HEADER,
            HeaderValue::from_static("not-json"),
        );
        headers.insert(
            PAYMENT_SIGNATURE_HEADER,
            HeaderValue::from_static(r#"{"signature":"proof"}"#),
        );

        let error = parse_x402_payload(&headers).unwrap_err();
        assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(error.code(), "x402_malformed_header");
    }

    #[test]
    fn parse_x402_payload_rejects_missing_proof_refs() {
        let payment_required = json!({
            "accepts": [
                {
                    "amount": "1000",
                    "asset": "sBTC",
                    "maxTimeoutSeconds": 60
                }
            ],
            "challenge": "challenge-1"
        });

        let payment_signature = json!({
            "payload": {
                "authorization": {
                    "nonce": "nonce-1",
                    "validBefore": "2000000000"
                }
            }
        });

        let headers = build_headers(payment_required, payment_signature);
        let error = parse_x402_payload(&headers).unwrap_err();

        assert_eq!(
            error,
            X402ParseError::MissingField {
                field: "proof_refs"
            }
        );
        assert_eq!(error.status_code(), StatusCode::PAYMENT_REQUIRED);
    }

    #[test]
    fn parse_gateway_payload_supports_legacy_typed_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_PAYMENT_HEADER,
            HeaderValue::from_static(
                r#"{"amount_satoshi":5000,"asset":"sBTC","challenge":"legacy-c1","expiry":2000000000,"proof_ref":"proof-legacy"}"#,
            ),
        );

        let parsed = parse_gateway_x402_payload(&headers).unwrap();
        assert_eq!(parsed.amount, 5000);
        assert_eq!(parsed.challenge, "legacy-c1");
        assert_eq!(parsed.proof_refs, vec!["proof-legacy".to_string()]);
    }

    #[test]
    fn parse_gateway_payload_supports_simple_legacy_proof_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_PAYMENT_HEADER,
            HeaderValue::from_static("proof-simple-123"),
        );

        let parsed = parse_gateway_x402_payload(&headers).unwrap();
        assert_eq!(parsed.amount, 1000);
        assert_eq!(parsed.asset, "sBTC");
        assert_eq!(parsed.proof_refs, vec!["proof-simple-123".to_string()]);
    }

    #[test]
    fn parse_gateway_payload_rejects_malformed_legacy_header() {
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_PAYMENT_HEADER, HeaderValue::from_static("{bad-json"));

        let error = parse_gateway_x402_payload(&headers).unwrap_err();
        assert_eq!(error.code(), "x402_malformed_header");
    }

    #[test]
    fn parse_error_metadata_is_stable() {
        let errors = vec![
            (
                X402ParseError::MissingField { field: "amount" },
                StatusCode::PAYMENT_REQUIRED,
                "x402_missing_field",
                "amount",
            ),
            (
                X402ParseError::InvalidField {
                    field: "expiry",
                    detail: "invalid",
                },
                StatusCode::BAD_REQUEST,
                "x402_invalid_field",
                "expiry",
            ),
        ];

        for (error, status, code, fragment) in errors {
            assert_eq!(error.status_code(), status);
            assert_eq!(error.code(), code);
            assert!(error.message().contains(fragment));
        }
    }

    #[test]
    fn parse_gateway_payload_rejects_empty_legacy_header() {
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_PAYMENT_HEADER, HeaderValue::from_static("   "));

        let error = parse_gateway_x402_payload(&headers).unwrap_err();
        assert!(matches!(error, X402ParseError::MalformedHeader { .. }));
    }

    #[test]
    fn parse_gateway_payload_rejects_invalid_utf8_legacy_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_PAYMENT_HEADER,
            HeaderValue::from_bytes(&[0x66, 0x6f, 0x80]).unwrap(),
        );

        let error = parse_gateway_x402_payload(&headers).unwrap_err();
        assert!(matches!(error, X402ParseError::MalformedHeader { .. }));
    }

    #[test]
    fn parse_gateway_payload_rejects_missing_legacy_amount_and_proofs() {
        let mut headers = HeaderMap::new();
        headers.insert(
            LEGACY_PAYMENT_HEADER,
            HeaderValue::from_static(
                r#"{"amount_satoshi":5,"asset":"sBTC","challenge":"legacy-c2","expiry":2000000000,"proof_ref":"","proof_refs":["",""]}"#,
            ),
        );

        let error = parse_gateway_x402_payload(&headers).unwrap_err();
        assert_eq!(error.code(), "x402_missing_field");
    }

    #[test]
    fn parse_x402_payload_rejects_invalid_amount() {
        let payment_required = json!({
            "accepts": [{
                "amount": "abc",
                "asset": "sBTC",
                "maxTimeoutSeconds": 60
            }],
            "challenge": "challenge-amount"
        });

        let payment_signature = json!({
            "signature": "proof-amount",
            "payload": {
                "authorization": {
                    "nonce": "nonce-amount"
                }
            }
        });

        let headers = build_headers(payment_required, payment_signature);
        let error = parse_x402_payload(&headers).unwrap_err();
        assert_eq!(error.code(), "x402_invalid_field");
    }

    #[test]
    fn parse_x402_payload_rejects_invalid_or_missing_expiry() {
        let payment_required_invalid_expiry = json!({
            "accepts": [{
                "amount": "5",
                "asset": "sBTC"
            }],
            "challenge": "challenge-expiry",
            "expiry": "not-a-timestamp"
        });

        let payment_signature = json!({
            "signature": "proof-expiry",
            "payload": {
                "authorization": { "nonce": "nonce-expiry" }
            }
        });

        let headers = build_headers(payment_required_invalid_expiry, payment_signature.clone());
        let invalid_error = parse_x402_payload(&headers).unwrap_err();
        assert_eq!(invalid_error.code(), "x402_invalid_field");

        let payment_required_missing_expiry = json!({
            "accepts": [{
                "amount": "5",
                "asset": "sBTC"
            }],
            "challenge": "challenge-expiry-missing"
        });

        let headers = build_headers(payment_required_missing_expiry, payment_signature);
        let missing_error = parse_x402_payload(&headers).unwrap_err();
        assert_eq!(missing_error.code(), "x402_missing_field");
    }

    #[test]
    fn parse_x402_payload_rejects_invalid_timeout_seconds() {
        let payment_required = json!({
            "accepts": [{
                "amount": "5",
                "asset": "sBTC",
                "maxTimeoutSeconds": "invalid"
            }],
            "challenge": "challenge-timeout"
        });

        let payment_signature = json!({
            "signature": "proof-timeout",
            "payload": {
                "authorization": {
                    "nonce": "nonce-timeout"
                }
            }
        });

        let headers = build_headers(payment_required, payment_signature);
        let error = parse_x402_payload(&headers).unwrap_err();
        assert_eq!(error.code(), "x402_invalid_field");
    }

    #[test]
    fn parse_x402_payload_collects_all_proof_reference_sources() {
        let payment_required = json!({
            "accepts": [{
                "amount": "1000",
                "asset": "sBTC",
                "maxTimeoutSeconds": 60
            }],
            "challenge": "challenge-proof-refs",
            "proofRefs": ["proof-required-1", "proof-required-2"]
        });

        let payment_signature = json!({
            "payload": {
                "authorization": {
                    "nonce": "nonce-proof-refs"
                },
                "transaction": "tx-proof-refs"
            },
            "signature": "sig-proof-refs",
            "proofRefs": ["proof-signature-1", "proof-signature-2"]
        });

        let mut headers = build_headers(payment_required, payment_signature);
        headers.insert(
            "x402-proof-refs",
            HeaderValue::from_static("proof-header-1,proof-header-2"),
        );

        let parsed = parse_x402_payload(&headers).unwrap();
        assert!(parsed.proof_refs.contains(&"proof-header-1".to_string()));
        assert!(parsed.proof_refs.contains(&"proof-signature-2".to_string()));
        assert!(parsed.proof_refs.contains(&"proof-required-1".to_string()));
    }

    #[test]
    fn parse_x402_payload_accepts_raw_json_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            PAYMENT_REQUIRED_HEADER,
            HeaderValue::from_static(
                r#"{"accepts":[{"amount":"7","asset":"sBTC","maxTimeoutSeconds":60}],"challenge":"raw-json"}"#,
            ),
        );
        headers.insert(
            PAYMENT_SIGNATURE_HEADER,
            HeaderValue::from_static(
                r#"{"signature":"proof-raw","payload":{"authorization":{"nonce":"nonce-raw"}}}"#,
            ),
        );

        let parsed = parse_x402_payload(&headers).unwrap();
        assert_eq!(parsed.amount, 7);
    }

    #[test]
    fn parse_x402_payload_rejects_invalid_utf8_and_empty_canonical_header_values() {
        let mut utf8_headers = HeaderMap::new();
        utf8_headers.insert(
            PAYMENT_REQUIRED_HEADER,
            HeaderValue::from_bytes(&[0x66, 0x6f, 0x80]).unwrap(),
        );
        utf8_headers.insert(PAYMENT_SIGNATURE_HEADER, HeaderValue::from_static("{}"));

        let utf8_error = parse_x402_payload(&utf8_headers).unwrap_err();
        assert_eq!(utf8_error.code(), "x402_malformed_header");

        let mut empty_headers = HeaderMap::new();
        empty_headers.insert(PAYMENT_REQUIRED_HEADER, HeaderValue::from_static("   "));
        empty_headers.insert(PAYMENT_SIGNATURE_HEADER, HeaderValue::from_static("{}"));

        let empty_error = parse_x402_payload(&empty_headers).unwrap_err();
        assert_eq!(empty_error.code(), "x402_malformed_header");
    }

    #[test]
    fn value_to_string_handles_number_and_non_scalar_values() {
        assert_eq!(value_to_string(Some(&json!(123))), Some("123".to_string()));
        assert_eq!(value_to_string(Some(&json!({"x":1}))), None);
        assert_eq!(value_to_string(None), None);
    }

    #[test]
    fn strict_path_detection_matches_expected_routes() {
        assert!(is_strictly_protected_path("/api/v1/settle"));
        assert!(is_strictly_protected_path("/api/v1/ingress/iso20022"));
        assert!(is_strictly_protected_path("/api/v1/erp/sync"));
        assert!(!is_strictly_protected_path("/api/v1/state"));
    }
}
