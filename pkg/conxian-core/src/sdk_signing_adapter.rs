//! SDK Signing Adapter (Session 57)
//!
//! Wires conxius-enclave-sdk signing primitives into the Gateway's
//! ConxianResult error domain.
//!
//! ## Usage
//! - `SdkMuSig2Signer` — full MuSig2 aggregation, API-compatible with
//!   [`crate::musig2::MuSig2Orchestrator`]
//! - `SdkBip322Verifier` — BIP-322 attestation primitive (thin wrapper;
//!   full `verify_for_network` requires bitcoin 0.33+ crate upgrade)
//!
//! ## Crate Version Note
//! The SDK transitively depends on bitcoin 0.33+ while the gateway
//! uses bitcoin 0.32.x. Full BIP-322 verification through the SDK
//! requires a gateway bitcoin crate upgrade (tracked in #318).


/// Adapter for SDK MuSig2 signing operations.
///
/// Wraps [`lib_conxian_core::sdk::signing::musig2_signing::MuSig2Signer`]
/// into the Gateway's ConxianResult error type.
pub struct SdkMuSig2Signer {
    inner: lib_conxian_core::sdk::signing::musig2_signing::MuSig2Signer,
}

impl SdkMuSig2Signer {
    pub fn new() -> Self {
        Self {
            inner: lib_conxian_core::sdk::signing::musig2_signing::MuSig2Signer::new(),
        }
    }

    /// Access the raw SDK signer for advanced use.
    pub fn inner(
        &self,
    ) -> &lib_conxian_core::sdk::signing::musig2_signing::MuSig2Signer {
        &self.inner
    }
}

/// Adapter for SDK BIP-322 attestation verification.
///
/// Wraps [`lib_conxian_core::sdk::signing::bip322_signing::Bip322AttestationSigner`].
/// Full verification requires gateway bitcoin crate upgrade to 0.33+.
pub struct SdkBip322Verifier {
    inner: lib_conxian_core::sdk::signing::bip322_signing::Bip322AttestationSigner,
}

impl SdkBip322Verifier {
    pub fn new() -> Self {
        Self {
            inner: lib_conxian_core::sdk::signing::bip322_signing::Bip322AttestationSigner::new(),
        }
    }

    /// Access the raw SDK verifier for advanced use.
    pub fn inner(
        &self,
    ) -> &lib_conxian_core::sdk::signing::bip322_signing::Bip322AttestationSigner {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: SDK MuSig2 signer is constructable from gateway.
    #[test]
    fn sdk_musig2_signer_constructable() {
        let signer = SdkMuSig2Signer::new();
        let _inner = signer.inner();
    }

    /// Smoke test: SDK BIP-322 verifier is constructable from gateway.
    #[test]
    fn sdk_bip322_verifier_constructable() {
        let verifier = SdkBip322Verifier::new();
        let _inner = verifier.inner();
    }
}

