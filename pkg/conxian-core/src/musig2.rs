use crate::ConxianResult;
use serde::{Deserialize, Serialize};

/// CON-1270: MuSig2 (BIP-327) Aggregated Public Key
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MuSig2AggregatedKey {
    pub aggregated_pubkey: String,
    pub participant_pubkeys: Vec<String>,
}

/// MuSig2 Partial Signature for aggregation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MuSig2PartialSignature {
    pub participant_pubkey: String,
    pub partial_signature: String,
    pub nonce: String,
}

/// Trait for MuSig2 (BIP-327) operations
pub trait MuSig2Orchestrator: Send + Sync {
    fn aggregate_pubkeys(&self, pubkeys: &[String]) -> ConxianResult<MuSig2AggregatedKey>;
    fn aggregate_signatures(
        &self,
        aggregated_key: &MuSig2AggregatedKey,
        partial_sigs: &[MuSig2PartialSignature],
        message_hash: &[u8; 32],
    ) -> ConxianResult<String>;
}

/// Real BIP-327 MuSig2 key aggregation over compressed secp256k1 public keys.
///
/// Participant order is significant: BIP-327 aggregation is order-dependent,
/// so callers must agree on the key order (the SDK's `MuSig2Session` uses the
/// same order the participants supply).
pub fn aggregate_musig2_pubkeys(
    pubkeys: &[String],
) -> ConxianResult<MuSig2AggregatedKey> {
    if pubkeys.is_empty() {
        return Err(crate::ConxianError::Api(
            "at least one pubkey is required".to_string(),
        ));
    }

    let mut points = Vec::with_capacity(pubkeys.len());
    for pk in pubkeys {
        let bytes = hex::decode(pk).map_err(|_| {
            crate::ConxianError::Api(format!("participant pubkey is not valid hex: {pk}"))
        })?;
        let point = musig2::secp::Point::from_slice(&bytes).map_err(|_| {
            crate::ConxianError::Api(format!("participant pubkey is not a valid secp256k1 point: {pk}"))
        })?;
        points.push(point);
    }

    let key_agg_ctx = musig2::KeyAggContext::new(points).map_err(|_| {
        crate::ConxianError::Api("MuSig2 key aggregation failed".to_string())
    })?;
    let aggregated: musig2::secp::Point = key_agg_ctx.aggregated_pubkey();

    Ok(MuSig2AggregatedKey {
        aggregated_pubkey: hex::encode(aggregated.serialize()),
        participant_pubkeys: pubkeys.to_vec(),
    })
}

/// Real BIP-327 MuSig2 signature aggregation.
///
/// `partial_sigs[i].nonce` is the participant's serialized public nonce
/// (`PubNonce`, 66 bytes) and `partial_sigs[i].partial_signature` is the
/// serialized partial signature (32 bytes). The message hash is the 32-byte
/// BIP-340 message the partial signatures were produced over.
pub fn aggregate_musig2_signatures(
    aggregated_key: &MuSig2AggregatedKey,
    partial_sigs: &[MuSig2PartialSignature],
    message_hash: &[u8; 32],
) -> ConxianResult<String> {
    if partial_sigs.is_empty() {
        return Err(crate::ConxianError::Api(
            "at least one partial signature is required".to_string(),
        ));
    }
    if partial_sigs.len() != aggregated_key.participant_pubkeys.len() {
        return Err(crate::ConxianError::Api(format!(
            "incomplete partial signatures: got {}, expected {}",
            partial_sigs.len(),
            aggregated_key.participant_pubkeys.len()
        )));
    }

    let mut points = Vec::with_capacity(aggregated_key.participant_pubkeys.len());
    for pk in &aggregated_key.participant_pubkeys {
        let bytes = hex::decode(pk).map_err(|_| {
            crate::ConxianError::Api(format!("participant pubkey is not valid hex: {pk}"))
        })?;
        let point = musig2::secp::Point::from_slice(&bytes).map_err(|_| {
            crate::ConxianError::Api(format!("participant pubkey is not a valid secp256k1 point: {pk}"))
        })?;
        points.push(point);
    }
    let key_agg_ctx = musig2::KeyAggContext::new(points).map_err(|_| {
        crate::ConxianError::Api("MuSig2 key aggregation failed".to_string())
    })?;

    let mut pub_nonces = Vec::with_capacity(partial_sigs.len());
    let mut partial_signatures = Vec::with_capacity(partial_sigs.len());
    for sig in partial_sigs {
        let nonce_bytes = hex::decode(&sig.nonce).map_err(|_| {
            crate::ConxianError::Api("partial signature nonce is not valid hex".to_string())
        })?;
        let pub_nonce = musig2::PubNonce::from_bytes(&nonce_bytes).map_err(|_| {
            crate::ConxianError::Api("partial signature nonce is not a valid MuSig2 public nonce".to_string())
        })?;
        pub_nonces.push(pub_nonce);

        let sig_bytes = hex::decode(&sig.partial_signature).map_err(|_| {
            crate::ConxianError::Api("partial signature is not valid hex".to_string())
        })?;
        let partial = musig2::PartialSignature::from_slice(&sig_bytes).map_err(|_| {
            crate::ConxianError::Api("partial signature is not a valid MuSig2 scalar".to_string())
        })?;
        partial_signatures.push(partial);
    }

    let aggregated_nonce = musig2::AggNonce::sum(pub_nonces);
    let final_sig: musig2::CompactSignature = musig2::aggregate_partial_signatures(
        &key_agg_ctx,
        &aggregated_nonce,
        partial_signatures,
        message_hash,
    )
    .map_err(|_| crate::ConxianError::Api("MuSig2 signature aggregation failed".to_string()))?;

    Ok(hex::encode(final_sig.serialize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use musig2::{sign_partial, KeyAggContext, SecNonce};

    /// Two-party MuSig2 round trip: key aggregation -> nonce exchange ->
    /// partial signing -> aggregation -> BIP-340 verification.
    #[test]
    fn musig2_two_party_round_trip_produces_a_valid_signature() {
        let seckeys = [
            musig2::secp::Scalar::from_slice(&[0x11u8; 32]).unwrap(),
            musig2::secp::Scalar::from_slice(&[0x22u8; 32]).unwrap(),
        ];
        let points: Vec<musig2::secp::Point> = seckeys
            .iter()
            .map(|sk| sk.base_point_mul())
            .collect();
        let pubkey_hexes: Vec<String> = points
            .iter()
            .map(|p| hex::encode(p.serialize()))
            .collect();

        let aggregated_key = aggregate_musig2_pubkeys(&pubkey_hexes).unwrap();
        let expected: musig2::secp::Point = KeyAggContext::new(points.clone())
            .unwrap()
            .aggregated_pubkey();
        assert_eq!(aggregated_key.aggregated_pubkey, hex::encode(expected.serialize()));

        let key_agg_ctx = KeyAggContext::new(points.clone()).unwrap();
        let aggregated_point: musig2::secp::Point = key_agg_ctx.aggregated_pubkey();
        let message = [0x42u8; 32];

        let sec_nonces: Vec<SecNonce> = seckeys
            .iter()
            .enumerate()
            .map(|(i, sk)| {
                SecNonce::generate(
                    [i as u8 + 1; 32],
                    *sk,
                    aggregated_point,
                    message,
                    [0u8; 0],
                )
            })
            .collect();
        let pub_nonces: Vec<musig2::PubNonce> =
            sec_nonces.iter().map(|n| n.public_nonce()).collect();
        let aggregated_nonce = musig2::AggNonce::sum(pub_nonces.clone());

        let mut partial_sigs = Vec::new();
        for (i, sk) in seckeys.iter().enumerate() {
            let partial: musig2::PartialSignature = sign_partial(
                &key_agg_ctx,
                *sk,
                sec_nonces[i].clone(),
                &aggregated_nonce,
                message,
            )
            .unwrap();
            partial_sigs.push(MuSig2PartialSignature {
                participant_pubkey: pubkey_hexes[i].clone(),
                partial_signature: hex::encode(partial.serialize()),
                nonce: hex::encode(pub_nonces[i].serialize()),
            });
        }

        let final_hex =
            aggregate_musig2_signatures(&aggregated_key, &partial_sigs, &message).unwrap();
        let final_bytes = hex::decode(&final_hex).unwrap();

        let final_sig = musig2::CompactSignature::from_bytes(&final_bytes)
            .expect("aggregated signature must be a valid 64-byte BIP-340 signature");
        musig2::verify_single(aggregated_point, final_sig, message)
            .expect("aggregated MuSig2 signature must verify against the aggregated key");
    }

    #[test]
    fn rejects_malformed_and_incomplete_inputs() {
        assert!(aggregate_musig2_pubkeys(&[]).is_err());
        assert!(aggregate_musig2_pubkeys(&["not-hex".to_string()]).is_err());

        let agg = MuSig2AggregatedKey {
            aggregated_pubkey: "02".repeat(33),
            participant_pubkeys: vec!["02".repeat(33), "03".repeat(33)],
        };
        assert!(aggregate_musig2_signatures(&agg, &[], &[0u8; 32]).is_err());

        let one_partial = vec![MuSig2PartialSignature {
            participant_pubkey: "02".repeat(33),
            partial_signature: "00".repeat(32),
            nonce: "00".repeat(66),
        }];
        assert!(aggregate_musig2_signatures(&agg, &one_partial, &[0u8; 32]).is_err());
    }
}
