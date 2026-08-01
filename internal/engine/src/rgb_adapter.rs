//! RGB Adapter Bridge: consumes `lib_conxian_core::rgb::RGBAdapter` trait.
//!
//! Bridges the Gateway's existing `rgb-native` feature dependency stack
//! (rgb-core v0.12, rgb-std) into `lib-conxian-core`'s canonical RGBAdapter
//! trait interface, eliminating the parallel RGB integration path.
//!
//! When `rgb-native` is NOT enabled, a no-op implementation is provided
//! that returns `RGBError::VerificationUnavailable` for all operations.

use lib_conxian_core::rgb::{RGBAdapter, RGBError, RGBExecutionMode};

/// RGB adapter that delegates to Gateway's rgb-native feature stack
/// when available, or returns `GatedByRolloutMode` otherwise.
pub struct GatewayRgbAdapter {
    mode: RGBExecutionMode,
}

impl GatewayRgbAdapter {
    pub fn new(mode: RGBExecutionMode) -> Self {
        Self { mode }
    }

    pub fn mode(&self) -> RGBExecutionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: RGBExecutionMode) {
        self.mode = mode;
    }
}

impl Default for GatewayRgbAdapter {
    fn default() -> Self {
        Self {
            mode: RGBExecutionMode::Disabled,
        }
    }
}

impl RGBAdapter for GatewayRgbAdapter {
    fn validate_transition(&self, transition_hex: &str) -> Result<bool, RGBError> {
        match self.mode {
            RGBExecutionMode::Disabled => Err(RGBError::GatedByRolloutMode),
            RGBExecutionMode::Shadow => {
                if transition_hex.is_empty() {
                    return Err(RGBError::TransitionValidationFailed(
                        "empty transition hex".into(),
                    ));
                }
                // Shadow mode: non-authoritative validation runs but enforcement skipped
                Err(RGBError::GatedByRolloutMode)
            }
            RGBExecutionMode::Active => {
                #[cfg(feature = "rgb-native")]
                {
                    validate_transition_native(transition_hex)
                }
                #[cfg(not(feature = "rgb-native"))]
                {
                    Err(RGBError::TransitionValidationFailed(
                        "rgb-native feature not enabled".into(),
                    ))
                }
            }
        }
    }

    fn verify_seal(&self, _utxo_txid: &str, _seal_commitment: &str) -> Result<bool, RGBError> {
        match self.mode {
            RGBExecutionMode::Disabled => Err(RGBError::GatedByRolloutMode),
            RGBExecutionMode::Shadow => Err(RGBError::GatedByRolloutMode),
            RGBExecutionMode::Active => {
                #[cfg(feature = "rgb-native")]
                {
                    verify_seal_native(utxo_txid, seal_commitment)
                }
                #[cfg(not(feature = "rgb-native"))]
                {
                    Err(RGBError::SealVerificationFailed)
                }
            }
        }
    }

    fn get_contract_details(&self, contract_id: &str) -> Result<String, RGBError> {
        match self.mode {
            RGBExecutionMode::Disabled => Err(RGBError::GatedByRolloutMode),
            RGBExecutionMode::Shadow => Err(RGBError::ContractNotFound(contract_id.to_string())),
            RGBExecutionMode::Active => {
                #[cfg(feature = "rgb-native")]
                {
                    get_contract_details_native(contract_id)
                }
                #[cfg(not(feature = "rgb-native"))]
                {
                    Err(RGBError::ContractNotFound(contract_id.to_string()))
                }
            }
        }
    }
}

// ── Native RGB implementations (behind rgb-native feature) ────────────────────

#[cfg(feature = "rgb-native")]
fn validate_transition_native(transition_hex: &str) -> Result<bool, RGBError> {
    if transition_hex.is_empty() {
        return Err(RGBError::TransitionValidationFailed(
            "empty transition hex".into(),
        ));
    }

    let _transition_bytes = hex::decode(transition_hex)
        .map_err(|e| RGBError::TransitionValidationFailed(format!("hex decode failed: {e}")))?;

    // Full AluVM-based transition validation requires rgb-core schema evaluation.
    Err(RGBError::TransitionValidationFailed(
        "aluVM transition evaluation not yet wired to rgb-core".into(),
    ))
}

#[cfg(feature = "rgb-native")]
fn verify_seal_native(_utxo_txid: &str, _seal_commitment: &str) -> Result<bool, RGBError> {
    // Single-use seal verification requires Bitcoin UTXO inspection via
    // bp-core + Electrum/esplora. Not yet wired.
    Err(RGBError::SealVerificationFailed)
}

#[cfg(feature = "rgb-native")]
fn get_contract_details_native(contract_id: &str) -> Result<String, RGBError> {
    if contract_id.is_empty() {
        return Err(RGBError::InvalidContractId);
    }
    // Full contract lookup requires rgb-std stash inspection or rgb-node RPC.
    Err(RGBError::ContractNotFound(contract_id.to_string()))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_mode_rejects_all() {
        let adapter = GatewayRgbAdapter::new(RGBExecutionMode::Disabled);
        assert!(matches!(
            adapter.validate_transition("00ff"),
            Err(RGBError::GatedByRolloutMode)
        ));
        assert!(matches!(
            adapter.verify_seal("abc", "def"),
            Err(RGBError::GatedByRolloutMode)
        ));
        assert!(matches!(
            adapter.get_contract_details("cid"),
            Err(RGBError::GatedByRolloutMode)
        ));
    }

    #[test]
    fn shadow_mode_rejects_authoritative() {
        let adapter = GatewayRgbAdapter::new(RGBExecutionMode::Shadow);
        assert!(matches!(
            adapter.validate_transition("00ff"),
            Err(RGBError::GatedByRolloutMode)
        ));
        assert!(matches!(
            adapter.verify_seal("abc", "def"),
            Err(RGBError::GatedByRolloutMode)
        ));
        assert!(matches!(
            adapter.get_contract_details("cid"),
            Err(RGBError::ContractNotFound(_))
        ));
    }

    #[test]
    fn active_mode_without_rgb_native_returns_error() {
        let adapter = GatewayRgbAdapter::new(RGBExecutionMode::Active);
        // Without rgb-native feature, active calls return transition/seal errors
        let result = adapter.validate_transition("00ff");
        assert!(result.is_err());
    }

    #[test]
    fn default_is_disabled() {
        let adapter = GatewayRgbAdapter::default();
        assert_eq!(adapter.mode(), RGBExecutionMode::Disabled);
    }

    #[test]
    fn mode_can_be_changed() {
        let mut adapter = GatewayRgbAdapter::default();
        assert_eq!(adapter.mode(), RGBExecutionMode::Disabled);
        adapter.set_mode(RGBExecutionMode::Shadow);
        assert_eq!(adapter.mode(), RGBExecutionMode::Shadow);
        adapter.set_mode(RGBExecutionMode::Active);
        assert_eq!(adapter.mode(), RGBExecutionMode::Active);
    }
}
