Action completed.
#[cfg(test)]
mod crypto_provider_tests {
    #[test]
    fn crypto_provider_initialization_is_idempotent() {
        super::init_crypto_provider().expect("first provider initialization must succeed");
        super::init_crypto_provider().expect("repeated provider initialization must succeed");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
