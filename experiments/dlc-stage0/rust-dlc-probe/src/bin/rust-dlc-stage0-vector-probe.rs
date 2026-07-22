//! Bounded direct-consumption probe for the pinned dlcspecs vectors.

use std::path::{Path, PathBuf};

use lightning::ln::wire::Type;
use lightning::util::ser::Writeable;
use serde::Deserialize;

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
    let mut encoded = Vec::new();
    if part.message.type_id().write(&mut encoded).is_err() {
        return false;
    }
    if part.message.write(&mut encoded).is_err() {
        return false;
    }
    encoded == part.serialized
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

fn parse_vectors(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut parsed = 0usize;
    let mut blocked = 0usize;
    let mut byte_matches = 0usize;
    for path in vector_files(directory)? {
        let text = std::fs::read_to_string(&path)?;
        match serde_json::from_str::<TestVector>(&text) {
            Ok(vector) => {
                parsed += 1;
                let offer_match = serialized_bytes_match(&vector.offer_message);
                let accept_match = serialized_bytes_match(&vector.accept_message);
                let sign_match = serialized_bytes_match(&vector.sign_message);
                if offer_match && accept_match && sign_match {
                    byte_matches += 1;
                }
                println!(
                    "{}=direct-parse-pass offer_bytes={} accept_bytes={} sign_bytes={}",
                    vector_name(&path),
                    offer_match,
                    accept_match,
                    sign_match
                );
            }
            Err(error) => {
                blocked += 1;
                println!("{}=blocked: {}", vector_name(&path), error);
            }
        }
    }
    println!(
        "summary=parsed:{} blocked:{} all_bytes_match:{}",
        parsed, blocked, byte_matches
    );
    println!(
        "reason=official vectors use localPayout; rust-dlc v0.8.0 serde schema requires offerPayout"
    );
    Ok(())
}

fn usage() -> &'static str {
    "usage: rust-dlc-stage0-vector-probe --vectors <path-to-test_vectors>"
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let flag = args.next().ok_or(usage())?;
    if flag != "--vectors" {
        return Err(usage().into());
    }
    let directory = args.next().ok_or(usage())?;
    if args.next().is_some() {
        return Err(usage().into());
    }
    parse_vectors(Path::new(&directory))
}
