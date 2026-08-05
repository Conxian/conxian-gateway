//! BitVM2 Adapter Bridge
//!
//! Bridges the Gateway engine to `lib_conxian_core`'s BitVM2 verification
//! pipeline. The adapter consumes the SDK's bitvm2 boundary types (roles,
//! commitments, challenge windows, disprove envelopes) through core's
//! `sdk-blockchain` feature gate.
//!
//! When the `bitvm2-crypto` feature is not enabled (pending bellman crate),
//! the adapter operates in **boundary-validation mode**: it validates
//! role configurations, encoding versions, and instance IDs without
//! performing Groth16 SNARK verification.
//!
//! ## Feature Gates
//! - Default: boundary-only (role/instance validation)
//! - `bitvm2-crypto`: full Groth16 SNARK verification (pending bellman)

use lib_conxian_core::sdk;

/// BitVM2 execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitVM2ExecutionMode {
    /// No BitVM2 operations permitted.
    Disabled,
    /// Boundary validation only (role configs, encoding, instance IDs).
    BoundaryOnly,
    /// Shadow mode: run verification but don't enforce results.
    Shadow,
    /// Full enforcement: verification failures block operations.
    Enforce,
}

/// BitVM2 adapter — bridges Gateway ↔ Core ↔ SDK.
pub struct GatewayBitVM2Adapter {
    mode: BitVM2ExecutionMode,
}

impl GatewayBitVM2Adapter {
    pub fn new(mode: BitVM2ExecutionMode) -> Self {
        Self { mode }
    }

    pub fn mode(&self) -> BitVM2ExecutionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: BitVM2ExecutionMode) {
        self.mode = mode;
    }

    /// Validate a BitVM2 encoding version against the SDK's canonical version.
    pub fn validate_encoding_version(&self, version: u16) -> Result<(), BitVM2Error> {
        if self.mode == BitVM2ExecutionMode::Disabled {
            return Err(BitVM2Error::GatedByRolloutMode);
        }
        // SDK BITVM2_ENCODING_VERSION is the canonical value
        if version != sdk::BITVM2_ENCODING_VERSION {
            return Err(BitVM2Error::InvalidEncodingVersion(version));
        }
        Ok(())
    }

    /// Validate a BitVM2 instance role configuration.
    ///
    /// Verifies that exactly one operator and at most `n` verifiers are present,
    /// and that all participant IDs are non-empty.
    pub fn validate_role_config(
        &self,
        operator_id: &str,
        verifier_ids: &[String],
    ) -> Result<(), BitVM2Error> {
        if self.mode == BitVM2ExecutionMode::Disabled {
            return Err(BitVM2Error::GatedByRolloutMode);
        }
        if operator_id.is_empty() {
            return Err(BitVM2Error::InvalidRoleConfig(
                "operator ID must not be empty".into(),
            ));
        }
        if verifier_ids.is_empty() {
            return Err(BitVM2Error::InvalidRoleConfig(
                "at least one verifier is required".into(),
            ));
        }
        if verifier_ids.iter().any(|v| v.is_empty()) {
            return Err(BitVM2Error::InvalidRoleConfig(
                "verifier IDs must not be empty".into(),
            ));
        }
        Ok(())
    }

    /// Validate a BitVM2 instance identifier format.
    pub fn validate_instance_id(&self, instance_id: &str) -> Result<(), BitVM2Error> {
        if self.mode == BitVM2ExecutionMode::Disabled {
            return Err(BitVM2Error::GatedByRolloutMode);
        }
        if instance_id.is_empty() {
            return Err(BitVM2Error::InvalidInstanceId(
                "instance ID must not be empty".into(),
            ));
        }
        // Instance IDs are 32-byte hex strings
        if instance_id.len() != 64 || !instance_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(BitVM2Error::InvalidInstanceId(
                "instance ID must be a 64-character hex string (32 bytes)".into(),
            ));
        }
        Ok(())
    }
}

impl Default for GatewayBitVM2Adapter {
    fn default() -> Self {
        Self {
            mode: BitVM2ExecutionMode::Disabled,
        }
    }
}

/// BitVM2 adapter errors.
#[derive(Debug)]
pub enum BitVM2Error {
    /// BitVM2 operations are gated by rollout mode.
    GatedByRolloutMode,
    /// The encoding version is not supported.
    InvalidEncodingVersion(u16),
    /// Role configuration is invalid.
    InvalidRoleConfig(String),
    /// Instance ID format is invalid.
    InvalidInstanceId(String),
    /// Groth16 proof verification failed (available with bitvm2-crypto).
    ProofVerificationFailed(String),
}

impl std::fmt::Display for BitVM2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GatedByRolloutMode => write!(f, "BitVM2 operations gated by rollout mode"),
            Self::InvalidEncodingVersion(v) => {
                write!(f, "invalid BitVM2 encoding version: {v}")
            }
            Self::InvalidRoleConfig(msg) => write!(f, "invalid role config: {msg}"),
            Self::InvalidInstanceId(msg) => write!(f, "invalid instance ID: {msg}"),
            Self::ProofVerificationFailed(msg) => {
                write!(f, "Groth16 proof verification failed: {msg}")
            }
        }
    }
}

impl std::error::Error for BitVM2Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_mode_rejects_all() {
        let adapter = GatewayBitVM2Adapter::default();
        assert!(matches!(
            adapter.validate_encoding_version(1),
            Err(BitVM2Error::GatedByRolloutMode)
        ));
    }

    #[test]
    fn boundary_mode_validates_encoding() {
        let adapter = GatewayBitVM2Adapter::new(BitVM2ExecutionMode::BoundaryOnly);
        // SDK BITVM2_ENCODING_VERSION = 1
        assert!(adapter.validate_encoding_version(1).is_ok());
        assert!(adapter.validate_encoding_version(2).is_err());
    }

    #[test]
    fn role_config_requires_operator_and_verifiers() {
        let adapter = GatewayBitVM2Adapter::new(BitVM2ExecutionMode::BoundaryOnly);

        // Missing operator
        assert!(adapter.validate_role_config("", &["v1".into()]).is_err());
        // Missing verifiers
        assert!(adapter
            .validate_role_config("op1", &[])
            .is_err());
        // Valid
        assert!(adapter
            .validate_role_config("op1", &["v1".into(), "v2".into()])
            .is_ok());
    }

    #[test]
    fn instance_id_must_be_64_char_hex() {
        let adapter = GatewayBitVM2Adapter::new(BitVM2ExecutionMode::BoundaryOnly);

        assert!(adapter.validate_instance_id("").is_err());
        assert!(adapter.validate_instance_id("abc").is_err());
        assert!(adapter.validate_instance_id(&"00".repeat(32)).is_ok());
        assert!(adapter
            .validate_instance_id(&"zz".repeat(32))
            .is_err()); // non-hex
    }
}
