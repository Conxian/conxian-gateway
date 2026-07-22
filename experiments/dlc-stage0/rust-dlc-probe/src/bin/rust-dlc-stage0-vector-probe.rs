//! Bounded direct-consumption probe for the pinned dlcspecs vectors.

use std::path::{Path, PathBuf};

use lightning::ln::wire::Type;
use lightning::util::ser::Writeable;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct TestVectorPart<T> {
    message: T,
    #[serde(deserialize_with = "dlc_messages::serde_utils::deserialize_hex_string")]
    serialized: Vec<u8>,
}

#[derive(Deserialize)]
struct TestVector {
    offer_message: TestVectorPart<dlc_messages::OfferDlc>,
    accept_message: TestVectorPart<dlc_messages::AcceptDlc>,
    sign_message: TestVectorPart<dlc_messages::SignDlc>,
}

fn serialized_bytes_match<T: Writeable + Type>(part: &TestVectorPart<T>) -> bool {
    serialized_bytes(part) == part.serialized
}

fn serialized_bytes<T: Writeable + Type>(part: &TestVectorPart<T>) -> Vec<u8> {
    let mut encoded = Vec::new();
    part.message
        .type_id()
        .write(&mut encoded)
        .expect("message type id serializes");
    part.message
        .write(&mut encoded)
        .expect("message serializes");
    encoded
}

fn first_byte_difference(
    expected: &[u8],
    actual: &[u8],
) -> Option<(usize, Option<u8>, Option<u8>)> {
    let shared_len = expected.len().min(actual.len());
    for offset in 0..shared_len {
        if expected[offset] != actual[offset] {
            return Some((offset, Some(expected[offset]), Some(actual[offset])));
        }
    }
    if expected.len() != actual.len() {
        return Some((
            shared_len,
            expected.get(shared_len).copied(),
            actual.get(shared_len).copied(),
        ));
    }
    None
}

fn hex_window(bytes: &[u8], offset: usize) -> String {
    let start = offset.saturating_sub(8);
    let end = (offset + 8).min(bytes.len());
    bytes[start..end]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn indexed_window(bytes: &[u8], offset: usize) -> String {
    let start = offset.saturating_sub(8);
    let end = (offset + 24).min(bytes.len());
    (start..end)
        .map(|index| format!("{index}:{:02x}", bytes[index]))
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_payout_fields(value: &mut Value) -> Result<usize, String> {
    match value {
        Value::Array(values) => values.iter_mut().try_fold(0usize, |count, value| {
            Ok(count + normalize_payout_fields(value)?)
        }),
        Value::Object(fields) => {
            let mut normalized = 0usize;
            if fields.contains_key("localPayout") {
                if !fields.contains_key("outcome") {
                    return Err("localPayout appeared outside an outcome payout object".into());
                }
                if fields.contains_key("offerPayout") {
                    return Err("payout object contains both localPayout and offerPayout".into());
                }
                let payout = fields
                    .remove("localPayout")
                    .expect("localPayout presence checked above");
                fields.insert("offerPayout".into(), payout);
                normalized += 1;
            }
            for value in fields.values_mut() {
                normalized += normalize_payout_fields(value)?;
            }
            Ok(normalized)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(0),
    }
}

fn normalize_offer_message(value: &mut Value) -> Result<usize, String> {
    let offer_message = value
        .get_mut("offer_message")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "missing offer_message object".to_owned())?;
    let message = offer_message
        .get_mut("message")
        .ok_or_else(|| "missing offer_message.message value".to_owned())?;
    normalize_payout_fields(message)
}

fn vector_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>")
}

fn vector_files(directory: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files: Vec<_> = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    files.retain(|path| path.extension().and_then(|extension| extension.to_str()) == Some("json"));
    files.sort();
    Ok(files)
}

fn parse_vector(
    text: &str,
    compatibility: bool,
) -> Result<(TestVector, usize), Box<dyn std::error::Error>> {
    if !compatibility {
        return Ok((serde_json::from_str(text)?, 0));
    }

    let mut value: Value = serde_json::from_str(text)?;
    let normalized = normalize_offer_message(&mut value)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    Ok((serde_json::from_value(value)?, normalized))
}

fn parse_vectors(directory: &Path, compatibility: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut parsed = 0usize;
    let mut blocked = 0usize;
    let mut byte_matches = 0usize;
    let mut normalized_fields = 0usize;
    for path in vector_files(directory)? {
        let text = std::fs::read_to_string(&path)?;
        match parse_vector(&text, compatibility) {
            Ok((vector, normalized)) => {
                parsed += 1;
                normalized_fields += normalized;
                let offer_match = serialized_bytes_match(&vector.offer_message);
                let accept_match = serialized_bytes_match(&vector.accept_message);
                let sign_match = serialized_bytes_match(&vector.sign_message);
                if offer_match && accept_match && sign_match {
                    byte_matches += 1;
                }
                println!(
                    "{}={}-parse-pass normalized_local_payouts={} offer_bytes={} accept_bytes={} sign_bytes={}",
                    vector_name(&path),
                    if compatibility { "compatibility" } else { "direct" },
                    normalized,
                    offer_match,
                    accept_match,
                    sign_match
                );
                if !offer_match {
                    let actual = serialized_bytes(&vector.offer_message);
                    if let Some((offset, expected, actual_byte)) =
                        first_byte_difference(&vector.offer_message.serialized, &actual)
                    {
                        println!(
                            "{}=offer-first-diff offset={} expected={:?} actual={:?} expected_window={} actual_window={} expected_len={} actual_len={}",
                            vector_name(&path),
                            offset,
                            expected,
                            actual_byte,
                            hex_window(&vector.offer_message.serialized, offset),
                            hex_window(&actual, offset),
                            vector.offer_message.serialized.len(),
                            actual.len(),
                        );
                        println!(
                            "{}=offer-indexed-window expected={} actual={}",
                            vector_name(&path),
                            indexed_window(&vector.offer_message.serialized, offset),
                            indexed_window(&actual, offset),
                        );
                        if vector_name(&path) == "single_oracle_numerical_hyperbola_test.json"
                            && offset == 104
                        {
                            println!(
                                "{}=offer-mismatch-field field=translate_outcome spec_encoding=sign:u8+integer:u64+extra_precision:u16 rust_dlc_encoding=ieee754_f64",
                                vector_name(&path)
                            );
                        }
                    }
                }
            }
            Err(error) => {
                blocked += 1;
                println!("{}=blocked: {}", vector_name(&path), error);
            }
        }
    }
    println!(
        "summary=mode:{} parsed:{} blocked:{} all_bytes_match:{} normalized_local_payouts:{}",
        if compatibility {
            "compatibility"
        } else {
            "direct"
        },
        parsed,
        blocked,
        byte_matches,
        normalized_fields
    );
    println!(
        "reason=official offer_message payout objects use localPayout; rust-dlc v0.8.0 serde schema requires offerPayout"
    );
    Ok(())
}

fn usage() -> &'static str {
    "usage: rust-dlc-stage0-vector-probe [--compatibility] --vectors <path-to-test_vectors>"
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut compatibility = false;
    let mut directory = None;
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--compatibility") => compatibility = true,
            Some("--vectors") => directory = Some(args.next().ok_or(usage())?),
            _ => return Err(usage().into()),
        }
    }
    let directory = directory.ok_or(usage())?;
    parse_vectors(Path::new(&directory), compatibility)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::normalize_offer_message;

    #[test]
    fn normalizes_only_offer_message_payout_fields() {
        let mut value = json!({
            "offer_message": {
                "message": {
                    "contractInfo": {
                        "payouts": [{"outcome": "yes", "localPayout": 1}]
                    }
                }
            },
            "accept_message": {"message": {"localPayout": 2}}
        });
        let original = value.clone();

        assert_eq!(normalize_offer_message(&mut value).unwrap(), 1);
        assert_eq!(
            value["offer_message"]["message"]["contractInfo"]["payouts"][0]["offerPayout"],
            1
        );
        assert!(
            value["offer_message"]["message"]["contractInfo"]["payouts"][0]
                .get("localPayout")
                .is_none()
        );
        assert_eq!(value["accept_message"]["message"]["localPayout"], 2);
        assert_eq!(original["accept_message"]["message"]["localPayout"], 2);
    }

    #[test]
    fn rejects_ambiguous_payout_fields() {
        let mut value = json!({
            "offer_message": {
                "message": {
                    "payout": {"outcome": "yes", "localPayout": 1, "offerPayout": 1}
                }
            }
        });

        let error = normalize_offer_message(&mut value).unwrap_err();
        assert!(error.contains("both localPayout and offerPayout"));
    }
}
