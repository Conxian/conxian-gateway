//! RGB v0.12 native boundaries, feature-gated behind `rgb-native`.
//!
//! Phase 1.5 deliberately stops at canonical contract-ID parsing and stash
//! metadata presence. Full `ContractVerify`, consignment handling, and
//! signature policy remain Phase 2 work for issue #228.

use conxian_core::ConxianResult;
use std::sync::Arc;

#[cfg(feature = "rgb-native")]
use crate::bitcoin::rgb_stash::StashResolver;
#[cfg(not(feature = "rgb-native"))]
use crate::bitcoin::StashResolver;
#[cfg(not(feature = "rgb-native"))]
use conxian_core::ConxianError;

/// Verifies an RGB transition against the local Phase 1.5 stash boundary.
///
/// The input must be a canonical `contract:` Baid64 ID. A valid but unknown
/// contract is rejected (`Ok(false)`); an invalid ID or missing native resolver
/// is returned as an actionable error.
#[cfg(feature = "rgb-native")]
pub fn verify_transition_native(
    transition_id: &str,
    stash: &Option<Arc<StashResolver>>,
) -> ConxianResult<bool> {
    validate_contract_id_native(transition_id)?;
    let resolver = stash.as_ref().ok_or_else(|| {
        conxian_core::ConxianError::Rgb("RGB native resolver is not configured".to_string())
    })?;
    resolver.verify_transition(transition_id)
}

/// Resolves RGB contract metadata from the local Phase 1.5 stash.
#[cfg(feature = "rgb-native")]
pub fn lookup_contract_native(
    contract_id: &str,
    stash: &Option<Arc<StashResolver>>,
) -> ConxianResult<Option<serde_json::Value>> {
    validate_contract_id_native(contract_id)?;
    let resolver = stash.as_ref().ok_or_else(|| {
        conxian_core::ConxianError::Rgb("RGB native resolver is not configured".to_string())
    })?;

    resolver.lookup_contract(contract_id).map(|meta| {
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

/// Validates and canonicalizes the native contract-ID boundary before any
/// HTTP fallback is attempted by Active mode.
#[cfg(feature = "rgb-native")]
pub fn validate_contract_id_native(input: &str) -> ConxianResult<String> {
    let parsed = input.parse::<rgb::ContractId>().map_err(|_| {
        conxian_core::ConxianError::Rgb(
            "invalid RGB contract ID; expected contract: Baid64".to_string(),
        )
    })?;
    let canonical = parsed.to_string();
    if !input.starts_with("contract:") || !canonical.starts_with("contract:") {
        return Err(conxian_core::ConxianError::Rgb(
            "invalid RGB contract ID; expected contract: Baid64".to_string(),
        ));
    }
    Ok(canonical)
}

// ── Fallback (feature disabled) ────────────────────────────────────────

#[cfg(not(feature = "rgb-native"))]
pub fn verify_transition_native(
    _transition_id: &str,
    _stash: &Option<Arc<StashResolver>>,
) -> ConxianResult<bool> {
    Err(ConxianError::Rgb(
        "rgb-native feature not enabled".to_string(),
    ))
}

#[cfg(not(feature = "rgb-native"))]
pub fn lookup_contract_native(
    _contract_id: &str,
    _stash: &Option<Arc<StashResolver>>,
) -> ConxianResult<Option<serde_json::Value>> {
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
    fn fallback_reports_missing_native_support() {
        let result = verify_transition_native(VALID_ID, &None);
        assert!(matches!(result, Err(ConxianError::Rgb(_))));
    }
}
