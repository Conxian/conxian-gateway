use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use secp256k1::{schnorr::Signature, Message, Secp256k1, XOnlyPublicKey};
use serde::Deserialize;
use thiserror::Error;

use crate::bitcoin::rgb_stash::IssuerSignatureValidator;

const POLICY_VERSION: u8 = 1;
const BIP340_ALGORITHM: &str = "bip340-secp256k1";
const BIP340_PUBLIC_KEY_BYTES: usize = 32;
const BIP340_SIGNATURE_BYTES: usize = 64;
const CALLBACK_MESSAGE_BYTES: usize = 32;
const MAX_POLICY_FILE_BYTES: u64 = 64 * 1024;

/// A versioned, fail-closed allowlist for RGB issuer signatures.
///
/// Each configured identity is matched byte-for-byte and case-sensitively to a
/// pinned BIP340 x-only public key. Validation treats the RGB callback's 32
/// bytes as the BIP340 message directly; it does not hash or encode them again.
#[derive(Debug, Clone)]
pub struct Bip340IssuerPolicy {
    issuers: BTreeMap<String, XOnlyPublicKey>,
}

/// Errors produced while loading or validating an RGB issuer policy.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum IssuerPolicyError {
    #[error("RGB issuer policy is not valid JSON: {0}")]
    InvalidJson(String),
    #[error("unsupported RGB issuer policy version {0}; expected version 1")]
    UnsupportedVersion(u64),
    #[error("RGB issuer policy must contain at least one issuer")]
    EmptyIssuerList,
    #[error("RGB issuer identity must not be empty")]
    EmptyIdentity,
    #[error("RGB issuer identity must contain printable ASCII only")]
    InvalidIdentity,
    #[error("duplicate RGB issuer identity: {0}")]
    DuplicateIdentity(String),
    #[error("unsupported RGB issuer algorithm for identity {identity}: {algorithm}")]
    UnsupportedAlgorithm { identity: String, algorithm: String },
    #[error("invalid BIP340 x-only public key for RGB issuer identity {0}")]
    InvalidPublicKey(String),
    #[error("failed to inspect RGB issuer policy file")]
    FileMetadata,
    #[error("RGB issuer policy path must be a regular file and must not be a symlink")]
    NotRegularFile,
    #[error("RGB issuer policy file exceeds the 65536-byte limit")]
    FileTooLarge,
    #[error("failed to open RGB issuer policy file")]
    FileOpen,
    #[error("failed to read RGB issuer policy file")]
    FileRead,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyDocument {
    version: u64,
    issuers: Vec<PolicyIssuer>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyIssuer {
    identity: String,
    algorithm: String,
    xonly_public_key_hex: String,
}

impl Bip340IssuerPolicy {
    /// Parses a complete version-1 JSON issuer policy from bytes.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, IssuerPolicyError> {
        let document: PolicyDocument = serde_json::from_slice(bytes)
            .map_err(|error| IssuerPolicyError::InvalidJson(error.to_string()))?;
        if document.version != u64::from(POLICY_VERSION) {
            return Err(IssuerPolicyError::UnsupportedVersion(document.version));
        }
        if document.issuers.is_empty() {
            return Err(IssuerPolicyError::EmptyIssuerList);
        }

        let mut issuers = BTreeMap::new();
        for issuer in document.issuers {
            validate_identity(&issuer.identity)?;
            if issuer.algorithm != BIP340_ALGORITHM {
                return Err(IssuerPolicyError::UnsupportedAlgorithm {
                    identity: issuer.identity,
                    algorithm: issuer.algorithm,
                });
            }
            let key_bytes = decode_public_key(&issuer.xonly_public_key_hex)
                .ok_or_else(|| IssuerPolicyError::InvalidPublicKey(issuer.identity.clone()))?;
            let public_key = XOnlyPublicKey::from_slice(&key_bytes)
                .map_err(|_| IssuerPolicyError::InvalidPublicKey(issuer.identity.clone()))?;
            if issuers
                .insert(issuer.identity.clone(), public_key)
                .is_some()
            {
                return Err(IssuerPolicyError::DuplicateIdentity(issuer.identity));
            }
        }

        Ok(Self { issuers })
    }

    /// Parses a complete version-1 JSON issuer policy from UTF-8 text.
    pub fn from_json_str(json: &str) -> Result<Self, IssuerPolicyError> {
        Self::from_json_bytes(json.as_bytes())
    }

    /// Loads a bounded policy from a regular file without following symlinks on
    /// Unix. Other platforms reject paths identified as symlinks before open.
    pub fn load_json_file(path: impl AsRef<Path>) -> Result<Self, IssuerPolicyError> {
        let path = path.as_ref();
        let path_metadata =
            std::fs::symlink_metadata(path).map_err(|_| IssuerPolicyError::FileMetadata)?;
        if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
            return Err(IssuerPolicyError::NotRegularFile);
        }

        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
        let file = options
            .open(path)
            .map_err(|_| IssuerPolicyError::FileOpen)?;
        read_policy_file(file)
    }
}

impl IssuerSignatureValidator for Bip340IssuerPolicy {
    fn validate(&self, articles_id: &[u8], issuer: &str, signature: &[u8]) -> Result<(), String> {
        let public_key = self
            .issuers
            .get(issuer)
            .ok_or_else(|| "RGB issuer identity is not pinned by policy".to_string())?;
        let message_bytes: [u8; CALLBACK_MESSAGE_BYTES] = articles_id
            .try_into()
            .map_err(|_| "RGB issuer callback message must be exactly 32 bytes".to_string())?;
        if signature.len() != BIP340_SIGNATURE_BYTES {
            return Err("RGB issuer signature must be exactly 64 bytes".to_string());
        }
        let signature = Signature::from_slice(signature).map_err(|_| {
            "RGB issuer signature must be a valid raw 64-byte BIP340 signature".to_string()
        })?;
        let message = Message::from_digest(message_bytes);
        Secp256k1::verification_only()
            .verify_schnorr(&signature, &message, public_key)
            .map_err(|_| "RGB issuer BIP340 signature verification failed".to_string())
    }
}

fn validate_identity(identity: &str) -> Result<(), IssuerPolicyError> {
    if identity.is_empty() {
        return Err(IssuerPolicyError::EmptyIdentity);
    }
    if !identity.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        return Err(IssuerPolicyError::InvalidIdentity);
    }
    Ok(())
}

fn decode_public_key(value: &str) -> Option<[u8; BIP340_PUBLIC_KEY_BYTES]> {
    if value.len() != BIP340_PUBLIC_KEY_BYTES * 2 {
        return None;
    }
    let bytes = hex::decode(value).ok()?;
    bytes.try_into().ok()
}

fn read_policy_file(file: File) -> Result<Bip340IssuerPolicy, IssuerPolicyError> {
    let metadata = file
        .metadata()
        .map_err(|_| IssuerPolicyError::FileMetadata)?;
    if !metadata.is_file() {
        return Err(IssuerPolicyError::NotRegularFile);
    }
    if metadata.len() > MAX_POLICY_FILE_BYTES {
        return Err(IssuerPolicyError::FileTooLarge);
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_POLICY_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| IssuerPolicyError::FileRead)?;
    if bytes.len() as u64 > MAX_POLICY_FILE_BYTES {
        return Err(IssuerPolicyError::FileTooLarge);
    }
    Bip340IssuerPolicy::from_json_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;

    use bitcoin::hashes::{sha256, Hash};
    use secp256k1::{Keypair, SecretKey};

    use super::*;
    use crate::bitcoin::rgb_stash::RejectIssuerSignatures;

    const IDENTITY: &str = "did:example:conxian-rgb-issuer";
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (Bip340IssuerPolicy, Secp256k1<secp256k1::All>, Keypair) {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[0x11; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let (public_key, _) = XOnlyPublicKey::from_keypair(&keypair);
        let json = format!(
            r#"{{"version":1,"issuers":[{{"identity":"{IDENTITY}","algorithm":"{BIP340_ALGORITHM}","xonly_public_key_hex":"{}"}}]}}"#,
            hex::encode_upper(public_key.serialize())
        );
        (
            Bip340IssuerPolicy::from_json_str(&json).unwrap(),
            secp,
            keypair,
        )
    }

    fn signed_message(
        secp: &Secp256k1<secp256k1::All>,
        keypair: &Keypair,
        digest: [u8; 32],
    ) -> Signature {
        secp.sign_schnorr_no_aux_rand(&Message::from_digest(digest), keypair)
    }

    fn policy_json(identity: &str, algorithm: &str, key: &str) -> String {
        format!(
            r#"{{"version":1,"issuers":[{{"identity":"{identity}","algorithm":"{algorithm}","xonly_public_key_hex":"{key}"}}]}}"#
        )
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "conxian-rgb-policy-{name}-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn valid_pinned_identity_and_signature_succeeds() {
        let (policy, secp, keypair) = fixture();
        let digest = [0x42; 32];
        let signature = signed_message(&secp, &keypair, digest);
        assert_eq!(
            policy.validate(&digest, IDENTITY, signature.as_ref()),
            Ok(())
        );
    }

    #[test]
    fn identity_is_exact_and_case_sensitive() {
        let (policy, secp, keypair) = fixture();
        let digest = [0x42; 32];
        let signature = signed_message(&secp, &keypair, digest);
        assert!(policy
            .validate(
                &digest,
                "DID:example:conxian-rgb-issuer",
                signature.as_ref()
            )
            .is_err());
        assert!(policy
            .validate(&digest, "did:example:other", signature.as_ref())
            .is_err());
    }

    #[test]
    fn wrong_key_mutated_commitment_and_malformed_signatures_fail() {
        let (policy, secp, keypair) = fixture();
        let digest = [0x42; 32];
        let signature = signed_message(&secp, &keypair, digest);

        let wrong_keypair =
            Keypair::from_secret_key(&secp, &SecretKey::from_slice(&[0x22; 32]).unwrap());
        let wrong_signature = signed_message(&secp, &wrong_keypair, digest);
        assert!(policy
            .validate(&digest, IDENTITY, wrong_signature.as_ref())
            .is_err());

        let mut mutated = digest;
        mutated[0] ^= 1;
        assert!(policy
            .validate(&mutated, IDENTITY, signature.as_ref())
            .is_err());
        assert!(policy.validate(&digest, IDENTITY, &[0u8; 63]).is_err());
        assert!(policy.validate(&digest, IDENTITY, &[0xff; 64]).is_err());
        assert!(policy
            .validate(&digest[..31], IDENTITY, signature.as_ref())
            .is_err());
    }

    #[test]
    fn callback_digest_is_used_directly_without_second_hash() {
        let (policy, secp, keypair) = fixture();
        let callback_digest = [0x5a; 32];
        let direct_signature = signed_message(&secp, &keypair, callback_digest);
        assert!(policy
            .validate(&callback_digest, IDENTITY, direct_signature.as_ref())
            .is_ok());

        let rehashed = sha256::Hash::hash(&callback_digest).to_byte_array();
        let rehashed_signature = signed_message(&secp, &keypair, rehashed);
        assert!(policy
            .validate(&callback_digest, IDENTITY, rehashed_signature.as_ref())
            .is_err());
    }

    #[test]
    fn parser_rejects_invalid_policy_shapes_and_values() {
        let (_, _, keypair) = fixture();
        let (key, _) = XOnlyPublicKey::from_keypair(&keypair);
        let key = hex::encode(key.serialize());

        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(
                &policy_json(IDENTITY, BIP340_ALGORITHM, &key).replacen(
                    "\"version\":1",
                    "\"version\":2",
                    1
                )
            ),
            Err(IssuerPolicyError::UnsupportedVersion(2))
        ));
        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(r#"{"version":1,"issuers":[]}"#),
            Err(IssuerPolicyError::EmptyIssuerList)
        ));
        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(&policy_json("", BIP340_ALGORITHM, &key)),
            Err(IssuerPolicyError::EmptyIdentity)
        ));
        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(&policy_json("issuer-é", BIP340_ALGORITHM, &key)),
            Err(IssuerPolicyError::InvalidIdentity)
        ));
        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(&policy_json(IDENTITY, "auto", &key)),
            Err(IssuerPolicyError::UnsupportedAlgorithm { .. })
        ));
        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(&policy_json(IDENTITY, BIP340_ALGORITHM, "00")),
            Err(IssuerPolicyError::InvalidPublicKey(_))
        ));
        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(r#"{"version":1,"issuers":[],"unexpected":true}"#),
            Err(IssuerPolicyError::InvalidJson(_))
        ));
        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(&format!(
                r#"{{"version":1,"issuers":[{{"identity":"{IDENTITY}","algorithm":"{BIP340_ALGORITHM}","xonly_public_key_hex":"{key}","unexpected":true}}]}}"#
            )),
            Err(IssuerPolicyError::InvalidJson(_))
        ));
    }

    #[test]
    fn parser_rejects_duplicate_exact_identities() {
        let (_, _, keypair) = fixture();
        let (key, _) = XOnlyPublicKey::from_keypair(&keypair);
        let key = hex::encode(key.serialize());
        let json = format!(
            r#"{{"version":1,"issuers":[{{"identity":"{IDENTITY}","algorithm":"{BIP340_ALGORITHM}","xonly_public_key_hex":"{key}"}},{{"identity":"{IDENTITY}","algorithm":"{BIP340_ALGORITHM}","xonly_public_key_hex":"{key}"}}]}}"#
        );
        assert!(matches!(
            Bip340IssuerPolicy::from_json_str(&json),
            Err(IssuerPolicyError::DuplicateIdentity(identity)) if identity == IDENTITY
        ));
    }

    #[test]
    fn file_loader_accepts_regular_bounded_policy_and_rejects_oversized_file() {
        let (policy, _, _) = fixture();
        assert!(policy.issuers.contains_key(IDENTITY));
        let (_, _, keypair) = fixture();
        let (key, _) = XOnlyPublicKey::from_keypair(&keypair);
        let path = temp_path("regular");
        fs::write(
            &path,
            policy_json(IDENTITY, BIP340_ALGORITHM, &hex::encode(key.serialize())),
        )
        .unwrap();
        assert!(Bip340IssuerPolicy::load_json_file(&path).is_ok());
        fs::write(&path, vec![b' '; MAX_POLICY_FILE_BYTES as usize + 1]).unwrap();
        assert!(matches!(
            Bip340IssuerPolicy::load_json_file(&path),
            Err(IssuerPolicyError::FileTooLarge)
        ));
        fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn file_loader_rejects_symlinks_and_non_regular_files() {
        let target = temp_path("target");
        let link = temp_path("link");
        fs::write(&target, r#"{"version":1,"issuers":[]}"#).unwrap();
        symlink(&target, &link).unwrap();
        assert!(matches!(
            Bip340IssuerPolicy::load_json_file(&link),
            Err(IssuerPolicyError::NotRegularFile)
        ));

        let socket = temp_path("socket");
        let listener = UnixListener::bind(&socket).unwrap();
        assert!(matches!(
            Bip340IssuerPolicy::load_json_file(&socket),
            Err(IssuerPolicyError::NotRegularFile)
        ));
        drop(listener);
        fs::remove_file(socket).unwrap();
        fs::remove_file(link).unwrap();
        fs::remove_file(target).unwrap();
    }

    #[test]
    fn default_reject_policy_remains_fail_closed() {
        assert!(RejectIssuerSignatures
            .validate(&[0u8; 32], IDENTITY, &[0u8; 64])
            .is_err());
    }
}
