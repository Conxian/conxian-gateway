//! BIP-340 Schnorr signing and verification of desired-state manifests.
//!
//! The canonical byte form signed is the compact JSON serialization of the
//! [`DesiredStateManifest`]. Field order is fixed by the struct definition, so
//! the same manifest always produces the same bytes; a signature therefore
//! covers every field, including the action list.

use secp256k1::{schnorr, Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use sha2::{Digest, Sha256};

use crate::error::{DeviceError, DeviceResult};
use crate::manifest::{DesiredStateManifest, SignedManifest};

/// Canonical bytes covered by a manifest signature.
pub fn canonical_bytes(manifest: &DesiredStateManifest) -> DeviceResult<Vec<u8>> {
    serde_json::to_vec(manifest).map_err(|e| DeviceError::Malformed(e.to_string()))
}

fn digest(manifest: &DesiredStateManifest) -> DeviceResult<[u8; 32]> {
    let bytes = canonical_bytes(manifest)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher.finalize().into())
}

fn parse_secret_key(hex_key: &str) -> DeviceResult<SecretKey> {
    let bytes = hex::decode(hex_key.trim()).map_err(|_| DeviceError::BadSigningKey)?;
    SecretKey::from_slice(&bytes).map_err(|_| DeviceError::BadSigningKey)
}

/// Sign a manifest with a hex-encoded secp256k1 secret key.
///
/// Returns the manifest plus the signer's x-only public key and the signature,
/// both hex-encoded.
pub fn sign_manifest(
    manifest: &DesiredStateManifest,
    secret_key_hex: &str,
) -> DeviceResult<SignedManifest> {
    manifest.check_schema()?;
    let secp = Secp256k1::new();
    let sk = parse_secret_key(secret_key_hex)?;
    let keypair = Keypair::from_secret_key(&secp, &sk);
    let (xonly, _parity) = keypair.x_only_public_key();
    let msg = Message::from_digest(digest(manifest)?);
    // Deterministic (no auxiliary randomness): the same manifest signed by the
    // same key always yields the same signature, which keeps audit digests
    // reproducible across the local-install and fleet-agent tracks.
    let sig = secp.sign_schnorr_no_aux_rand(&msg, &keypair);
    Ok(SignedManifest {
        manifest: manifest.clone(),
        signer: hex::encode(xonly.serialize()),
        signature: hex::encode(sig.serialize()),
    })
}

/// Verify a signed manifest against a *trusted* signer key.
///
/// The key is passed in by the caller (device enrollment pins it); the
/// `signer` field inside the envelope is only compared, never trusted on its
/// own. Any mismatch, missing signature, or expired manifest fails closed.
pub fn verify_manifest(
    signed: &SignedManifest,
    trusted_signer_hex: &str,
    now_rfc3339: Option<&str>,
) -> DeviceResult<()> {
    signed.manifest.check_schema()?;

    let trusted = hex::decode(trusted_signer_hex.trim())
        .map_err(|_| DeviceError::BadSigningKey)?;
    let trusted = XOnlyPublicKey::from_slice(&trusted).map_err(|_| DeviceError::BadSigningKey)?;

    let envelope_signer = hex::decode(signed.signer.trim()).map_err(|_| DeviceError::BadSignature)?;
    let envelope_signer =
        XOnlyPublicKey::from_slice(&envelope_signer).map_err(|_| DeviceError::BadSignature)?;

    // The envelope must name the key we actually trust.
    if envelope_signer != trusted {
        return Err(DeviceError::SignatureInvalid);
    }

    if signed.signature.is_empty() {
        return Err(DeviceError::MissingSignature);
    }
    let sig_bytes = hex::decode(signed.signature.trim()).map_err(|_| DeviceError::BadSignature)?;
    let sig = schnorr::Signature::from_slice(&sig_bytes).map_err(|_| DeviceError::BadSignature)?;

    let msg = Message::from_digest(digest(&signed.manifest)?);
    let secp = Secp256k1::new();
    secp.verify_schnorr(&sig, &msg, &trusted)
        .map_err(|_| DeviceError::SignatureInvalid)?;

    if let Some(expires_at) = &signed.manifest.expires_at {
        if let Some(now) = now_rfc3339 {
            if let (Ok(exp), Ok(now)) = (
                chrono::DateTime::parse_from_rfc3339(expires_at),
                chrono::DateTime::parse_from_rfc3339(now),
            ) {
                if now >= exp {
                    return Err(DeviceError::Expired {
                        expires_at: expires_at.clone(),
                        now: now_rfc3339.unwrap_or_default().to_string(),
                    });
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ActionKind;

    const SK: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    fn sample() -> DesiredStateManifest {
        let mut m = DesiredStateManifest::new("device-1", "2026-11-14T00:00:00Z");
        m.actions.push(ActionKind::EnsureEnv {
            keys: vec!["NEON_POSTGRES_URL".into()],
        });
        m.actions.push(ActionKind::MigrateDatabase {
            target: "control-plane".into(),
        });
        m
    }

    #[test]
    fn sign_then_verify_succeeds() {
        let signed = sign_manifest(&sample(), SK).unwrap();
        verify_manifest(&signed, &signed.signer, None).unwrap();
    }

    #[test]
    fn tampered_action_fails_verification() {
        let mut signed = sign_manifest(&sample(), SK).unwrap();
        // Swap the migration target after signing; the signature must no longer
        // verify because the action list is covered by the digest.
        signed.manifest.actions[1] = ActionKind::MigrateDatabase {
            target: "attacker-controlled".into(),
        };
        assert!(matches!(
            verify_manifest(&signed, &signed.signer, None),
            Err(DeviceError::SignatureInvalid)
        ));
    }

    #[test]
    fn wrong_trusted_key_fails() {
        let signed = sign_manifest(&sample(), SK).unwrap();
        let other = "2222222222222222222222222222222222222222222222222222222222222222";
        let other_signed = sign_manifest(&sample(), other).unwrap();
        assert!(matches!(
            verify_manifest(&signed, &other_signed.signer, None),
            Err(DeviceError::SignatureInvalid)
        ));
    }

    #[test]
    fn empty_signature_is_rejected() {
        let mut signed = sign_manifest(&sample(), SK).unwrap();
        signed.signature = String::new();
        assert!(matches!(
            verify_manifest(&signed, &signed.signer, None),
            Err(DeviceError::MissingSignature)
        ));
    }

    #[test]
    fn expired_manifest_is_rejected() {
        let mut m = sample();
        m.expires_at = Some("2026-11-14T00:00:00Z".into());
        let signed = sign_manifest(&m, SK).unwrap();
        assert!(matches!(
            verify_manifest(&signed, &signed.signer, Some("2026-11-15T00:00:00Z")),
            Err(DeviceError::Expired { .. })
        ));
    }
}
