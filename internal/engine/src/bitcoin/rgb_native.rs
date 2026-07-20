//! RGB v0.12 native boundaries, feature-gated behind `rgb-native`.
//!
//! Native boundaries use the RGB filesystem stockpile for consensus presence
//! checks. Metadata lookup remains descriptive only; it is never a proof.

use conxian_core::{ConxianError, ConxianResult};
use std::sync::Arc;

#[cfg(feature = "rgb-native")]
use crate::bitcoin::rgb_stash::StashResolver;
#[cfg(not(feature = "rgb-native"))]
use crate::bitcoin::StashResolver;

const INVALID_CONTRACT_ID: &str = "invalid RGB contract ID; expected contract: Baid64";

/// Verifies an RGB transition against a persisted RGB stockpile contract.
///
/// The input must be a canonical `contract:` Baid64 ID. A valid but unknown
/// contract is rejected (`Ok(false)`); an invalid ID or missing native resolver
/// is returned as an actionable error.
#[cfg(feature = "rgb-native")]
pub fn verify_transition_native(
    transition_id: &str,
    stash: &Option<Arc<StashResolver>>,
) -> ConxianResult<bool> {
    let canonical_id = normalize_contract_id(transition_id)?;
    let resolver = stash.as_ref().ok_or_else(|| {
        conxian_core::ConxianError::Rgb("RGB native resolver is not configured".to_string())
    })?;
    resolver.verify_transition(&canonical_id)
}

/// Resolves descriptive RGB contract metadata from the JSON cache.
///
/// This function is intentionally not a consensus-verification API. Callers
/// must use [`verify_transition_native`] for the stockpile boundary.
#[cfg(feature = "rgb-native")]
pub fn lookup_contract_native(
    contract_id: &str,
    stash: &Option<Arc<StashResolver>>,
) -> ConxianResult<Option<serde_json::Value>> {
    let canonical_id = normalize_contract_id(contract_id)?;
    let resolver = stash.as_ref().ok_or_else(|| {
        conxian_core::ConxianError::Rgb("RGB native resolver is not configured".to_string())
    })?;

    resolver.lookup_contract(&canonical_id).map(|meta| {
        meta.map(|meta| {
            serde_json::json!({
                "contract_id": meta.contract_id,
                "ticker": meta.ticker,
                "name": meta.name,
                "supply": meta.supply,
                "precision": meta.precision,
                "last_transition": meta.last_transition,
                "resolved_via": "rgb-std-stash",
            })
        })
    })
}

/// Normalizes an RGB contract ID for all adapter boundaries.
///
/// Native builds use `rgb::ContractId` for full Baid64 checksum and payload
/// validation. Default-feature builds still reject empty, legacy, prefixless,
/// and malformed IDs at the boundary; they intentionally accept only the
/// canonical chunked `contract:` shape because the native parser is not
/// available. An optional Baid64 mnemonic fragment (`#word-word-word`) is
/// accepted and removed from the returned wire/canonical ID in both modes.
pub fn normalize_contract_id(input: &str) -> ConxianResult<String> {
    if input.is_empty() || input.trim() != input {
        return Err(ConxianError::Rgb(INVALID_CONTRACT_ID.to_string()));
    }

    #[cfg(feature = "rgb-native")]
    {
        let parsed = input
            .parse::<rgb::ContractId>()
            .map_err(|_| ConxianError::Rgb(INVALID_CONTRACT_ID.to_string()))?;
        let canonical = parsed.to_string();
        if !input.starts_with("contract:") || !canonical.starts_with("contract:") {
            return Err(ConxianError::Rgb(INVALID_CONTRACT_ID.to_string()));
        }
        Ok(canonical)
    }

    #[cfg(not(feature = "rgb-native"))]
    {
        normalize_contract_id_without_native(input)
    }
}

/// Compatibility name retained for callers that specifically refer to the
/// native validation boundary. The shared normalizer is the actual boundary
/// used by the adapter, stash, and HTTP paths.
pub fn validate_contract_id_native(input: &str) -> ConxianResult<String> {
    normalize_contract_id(input)
}

#[cfg(not(feature = "rgb-native"))]
fn normalize_contract_id_without_native(input: &str) -> ConxianResult<String> {
    let (base, mnemonic) = input.split_once('#').unwrap_or((input, ""));
    if base.contains('#') || (input.contains('#') && mnemonic.is_empty()) {
        return Err(ConxianError::Rgb(INVALID_CONTRACT_ID.to_string()));
    }

    let payload = base
        .strip_prefix("contract:")
        .filter(|payload| !payload.is_empty())
        .ok_or_else(|| ConxianError::Rgb(INVALID_CONTRACT_ID.to_string()))?;

    let chunks: Vec<&str> = payload.split('-').collect();
    let canonical_chunks = chunks.len() == 6
        && chunks[0].len() == 8
        && chunks[1..].iter().all(|chunk| chunk.len() == 7);
    if !canonical_chunks
        || chunks
            .iter()
            .flat_map(|chunk| chunk.bytes())
            .any(|byte| !matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'~'))
    {
        return Err(ConxianError::Rgb(INVALID_CONTRACT_ID.to_string()));
    }

    if !mnemonic.is_empty()
        && mnemonic
            .split('-')
            .any(|word| word.is_empty() || !word.bytes().all(|byte| byte.is_ascii_lowercase()))
    {
        return Err(ConxianError::Rgb(INVALID_CONTRACT_ID.to_string()));
    }

    Ok(base.to_string())
}

// ── Fallback (feature disabled) ────────────────────────────────────────

#[cfg(not(feature = "rgb-native"))]
pub fn verify_transition_native(
    transition_id: &str,
    _stash: &Option<Arc<StashResolver>>,
) -> ConxianResult<bool> {
    normalize_contract_id(transition_id)?;
    Err(ConxianError::Rgb(
        "rgb-native feature not enabled".to_string(),
    ))
}

#[cfg(not(feature = "rgb-native"))]
pub fn lookup_contract_native(
    contract_id: &str,
    _stash: &Option<Arc<StashResolver>>,
) -> ConxianResult<Option<serde_json::Value>> {
    normalize_contract_id(contract_id)?;
    Err(ConxianError::Rgb(
        "rgb-native feature not enabled".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::ConxianError;

    const VALID_ID: &str = "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg";

    #[cfg(feature = "rgb-native")]
    #[test]
    fn native_requires_a_configured_resolver() {
        let result = verify_transition_native(VALID_ID, &None);
        assert!(result.is_err());
    }

    #[cfg(feature = "rgb-native")]
    #[test]
    fn native_rejects_invalid_contract_ids_before_lookup() {
        let result = lookup_contract_native(
            "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCg!",
            &None,
        );
        assert!(matches!(result, Err(ConxianError::Rgb(_))));
    }

    #[cfg(feature = "rgb-native")]
    #[test]
    fn native_parser_accepts_known_baid64_and_rejects_mutations() {
        assert_eq!(validate_contract_id_native(VALID_ID).unwrap(), VALID_ID);
        assert_eq!(
            validate_contract_id_native(
                "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg#fractal-fashion-capsule"
            )
            .unwrap(),
            VALID_ID
        );
        assert!(validate_contract_id_native(
            "contractx:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg"
        )
        .is_err());
        assert!(validate_contract_id_native(
            "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCg!"
        )
        .is_err());
    }

    #[cfg(not(feature = "rgb-native"))]
    #[test]
    fn fallback_normalizes_canonical_ids_and_mnemonics() {
        assert_eq!(validate_contract_id_native(VALID_ID).unwrap(), VALID_ID);
        assert_eq!(
            validate_contract_id_native(
                "contract:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg#fractal-fashion-capsule"
            )
            .unwrap(),
            VALID_ID
        );
    }

    #[cfg(not(feature = "rgb-native"))]
    #[test]
    fn fallback_rejects_empty_legacy_and_noncanonical_ids() {
        for input in [
            "",
            "rgb:n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg",
            "n4bQgYhM-fWWaL_q-gxVrQFa-O~TxsrC-4Is0V1s-FbDwCgg",
            "contract:",
            "contract:not-a-contract",
        ] {
            assert!(validate_contract_id_native(input).is_err(), "{input:?}");
        }

        let result = verify_transition_native(VALID_ID, &None);
        assert!(matches!(result, Err(ConxianError::Rgb(message)) if message.contains("feature")));
    }
}
