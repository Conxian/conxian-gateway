use blake2::{Blake2s256, Digest};
use conxian_core::ConxianResult;

/// CON-1282: Blake2s-based PRF for Ark V-UTXO derivation.
pub struct ArkPrf;

impl ArkPrf {
    pub fn derive_vutxo(seed: &[u8], index: u32) -> ConxianResult<[u8; 32]> {
        let mut hasher = Blake2s256::new();
        hasher.update(seed);
        hasher.update(&index.to_le_bytes());
        let result = hasher.finalize();
        let mut output = [0u8; 32];
        output.copy_from_slice(&result);
        Ok(output)
    }

    pub fn compute_blake2s(data: &[u8]) -> [u8; 32] {
        let mut hasher = Blake2s256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut output = [0u8; 32];
        output.copy_from_slice(&result);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ark_prf_derivation() {
        let seed = b"ark-seed-test-123";
        let vutxo1 = ArkPrf::derive_vutxo(seed, 0).unwrap();
        let vutxo2 = ArkPrf::derive_vutxo(seed, 1).unwrap();
        assert_ne!(vutxo1, vutxo2);
        assert_eq!(vutxo1.len(), 32);
    }
}
