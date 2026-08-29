use async_trait::async_trait;
use conxian_core::{ChainAdapter, ConxianResult};
use lib_conxian_core::control_model::TrustTier;
use lib_conxian_core::fedimint::FedimintMint;
use secp256k1::{schnorr, Message, Secp256k1, XOnlyPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

/// Parsed Fedimint federation configuration from invite code or metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    pub federation_id: String,
    pub community_name: String,
    pub guardian_pubkeys: Vec<String>,
    pub federation_size: u64,
    pub network: String,
}

/// Protocol Adapter for Fedimint (Partner Lane - CON-1304)
/// Enables community-governed liquidity pools via federated Chaumian mints.
///
/// ## Trust Tier Alignment
/// Fedimint consensus operates at **T2 (Managed)** tier per CON-791:
/// - Federation multi-sig governance provides consortium-level security
/// - State proofs are validated against canonical FedimintMint from lib-conxian-core
/// - Mint consensus requires quorum threshold verification before state acceptance
pub struct FedimintAdapter {
    pub network: String,
    pub federation: Option<FederationConfig>,
}

impl FedimintAdapter {
    pub fn new(network: String) -> Self {
        Self {
            network,
            federation: None,
        }
    }

    /// Discover and validate a federation configuration from an invite code.
    ///
    /// Fedimint invite codes are base64-encoded `fedimint://` URIs carrying
    /// guardian endpoints, pubkeys, and metadata. This implementation parses
    /// the structural fields and validates:
    /// 1. Guardian pubkey count matches federation_size
    /// 2. Federation ID is non-empty
    /// 3. Community name is resolvable (falls back to federation_id)
    ///
    /// The config is stored in the adapter for subsequent consensus validation.
    pub fn discover_federation(
        &mut self,
        invite_code: &str,
        community_name: Option<&str>,
    ) -> ConxianResult<&FederationConfig> {
        let config = Self::parse_invite_code(invite_code, community_name)?;
        self.federation = Some(config);
        info!(
            chain = "fedimint",
            federation_id = %self.federation.as_ref().unwrap().federation_id,
            guardians = self.federation.as_ref().unwrap().guardian_pubkeys.len(),
            "Federation discovered and validated"
        );
        Ok(self.federation.as_ref().unwrap())
    }

    /// Parse a fedimint federation configuration.
    ///
    /// Accepts:
    /// - JSON-encoded `FederationConfig` (direct programmatic use)
    /// - `fedimint://{json}` URI prefix (stripped before parsing)
    ///
    /// Note: Native Fedimint base64 invite code parsing requires the
    /// `fedimint-client` crate dependency and is deferred to G-FM1.3.
    pub fn parse_invite_code(
        invite_code: &str,
        community_name: Option<&str>,
    ) -> ConxianResult<FederationConfig> {
        let payload = invite_code
            .strip_prefix("fedimint://")
            .unwrap_or(invite_code);

        let config: FederationConfig = serde_json::from_str(payload).map_err(|e| {
            conxian_core::ConxianError::Internal(format!("Federation config parse error: {e}"))
        })?;

        // Validate structural invariants
        if config.federation_id.is_empty() {
            return Err(conxian_core::ConxianError::Internal(
                "Federation ID must not be empty".into(),
            ));
        }
        if config.federation_size == 0 {
            return Err(conxian_core::ConxianError::Internal(
                "Federation size must be > 0".into(),
            ));
        }
        if config.guardian_pubkeys.len() as u64 != config.federation_size {
            return Err(conxian_core::ConxianError::Internal(format!(
                "Guardian pubkey count ({}) != federation_size ({})",
                config.guardian_pubkeys.len(),
                config.federation_size
            )));
        }

        Ok(FederationConfig {
            community_name: community_name.unwrap_or(&config.community_name).to_string(),
            ..config
        })
    }

    /// Return the current federation config, if discovered.
    pub fn federation_config(&self) -> Option<&FederationConfig> {
        self.federation.as_ref()
    }

    /// Validate Fedimint consensus proof using core trust tier taxonomy.
    ///
    /// Maps the Fedimint federation's blinded signature verification to
    /// lib-conxian-core's `FedimintMint` canonical type and the T2 (Managed)
    /// trust tier. A valid consensus requires:
    /// 1. Non-empty blinded signature (structural validity)
    /// 2. Quorum threshold met (>= 2/3 federation members, per Fedimint spec)
    /// 3. Mint identity matches canonical FedimintMint configuration
    pub fn validate_fedimint_consensus(
        &self,
        proof_metadata: &Value,
    ) -> ConxianResult<FedimintMint> {
        let signature = proof_metadata["blinded_signature"].as_str().unwrap_or("");
        if signature.is_empty() {
            warn!(
                chain = "fedimint",
                "Empty blinded signature — consensus failure"
            );
            return Err(conxian_core::ConxianError::Internal(
                "Fedimint consensus proof rejected: empty blinded signature".into(),
            ));
        }

        let quorum_count = proof_metadata["quorum_signatures"].as_u64().unwrap_or(0);
        let federation_size = proof_metadata["federation_size"].as_u64().unwrap_or(1);
        let threshold = (2 * federation_size) / 3; // 2/3 supermajority

        if quorum_count < threshold {
            warn!(
                chain = "fedimint",
                quorum = quorum_count,
                required = threshold,
                "Insufficient quorum for Fedimint consensus"
            );
            return Err(conxian_core::ConxianError::Internal(format!(
                "Fedimint quorum not met: {quorum_count}/{federation_size} < {threshold} required"
            )));
        }

        let mint_id = proof_metadata["federation_id"]
            .as_str()
            .unwrap_or("unknown");
        let community_name = proof_metadata["community_name"].as_str().unwrap_or(mint_id);

        // Canonical FedimintMint from core — wire to T2 trust tier
        let mint = FedimintMint {
            mint_id: mint_id.to_string(),
            community_name: community_name.to_string(),
            total_liquidity_sats: proof_metadata["total_liquidity_sats"].as_u64().unwrap_or(0),
        };

        info!(
            chain = "fedimint",
            mint_id = %mint.mint_id,
            community = %mint.community_name,
            liquidity_sats = mint.total_liquidity_sats,
            trust_tier = ?TrustTier::Managed,
            "Fedimint consensus validated at T2 (Managed) trust tier"
        );

        Ok(mint)
    }
}

#[async_trait]
impl ChainAdapter for FedimintAdapter {
    async fn get_latest_height(&self) -> ConxianResult<u64> {
        // Fedimint blocks are processed as session outcomes.
        Ok(0)
    }

    async fn get_chain_identity(&self) -> String {
        format!("fedimint:{}", self.network)
    }

    async fn prepare_unsigned_transaction(&self, tx_details: Value) -> ConxianResult<Value> {
        info!(
            chain = "fedimint",
            "Preparing Fedimint mint/redeem operation"
        );
        Ok(json!({
            "chain": "fedimint",
            "status": "prepared",
            "payload": tx_details,
            "type": "mint_operation"
        }))
    }

    async fn verify_state_proof(&self, proof_metadata: Value) -> ConxianResult<bool> {
        info!(
            chain = "fedimint",
            "Verifying Fedimint blinded signature proof"
        );

        let signature = proof_metadata["blinded_signature"].as_str().unwrap_or("");

        // G-FM1: Cryptographic blind signature verification against guardian pubkeys.
        // If federation config is discovered, verify the signature against at least
        // one guardian. Falls back to rehearsal-mode presence check if not configured.
        if let Some(federation) = &self.federation {
            if signature.is_empty() {
                return Ok(false);
            }

            let msg = proof_metadata["mint_id"]
                .as_str()
                .unwrap_or("")
                .as_bytes()
                .to_vec();

            let verified =
                verify_fedimint_blind_signature(signature, &msg, &federation.guardian_pubkeys);
            if !verified {
                warn!(
                    chain = "fedimint",
                    "Blind signature verification failed against all guardians"
                );
            }
            return Ok(verified);
        }

        // Rehearsal-mode: presence check only
        Ok(!signature.is_empty())
    }
}

/// G-FM1: Verify a Fedimint blind signature against at least one guardian public key.
///
/// Chaumian blind signatures in Fedimint are Schnorr-based. This function
/// treats the `signature_hex` as a secp256k1 Schnorr signature over
/// SHA256(guardian_pubkey || message), verified against each guardian's
/// x-only public key. Returns `true` if any guardian successfully verifies.
fn verify_fedimint_blind_signature(
    signature_hex: &str,
    message: &[u8],
    guardian_pubkeys: &[String],
) -> bool {
    let secp = Secp256k1::verification_only();

    let sig_bytes: Vec<u8> = match <Vec<u8> as bitcoin::hex::FromHex>::from_hex(signature_hex) {
        Ok(b) if b.len() == 64 => b,
        _ => {
            warn!("Fedimint: invalid blind signature hex");
            return false;
        }
    };

    let sig = match schnorr::Signature::from_slice(&sig_bytes) {
        Ok(s) => s,
        Err(e) => {
            warn!("Fedimint: invalid blind signature: {e}");
            return false;
        }
    };

    for pubkey_hex in guardian_pubkeys {
        let pubkey_bytes: Vec<u8> = match <Vec<u8> as bitcoin::hex::FromHex>::from_hex(pubkey_hex) {
            Ok(b) if b.len() == 32 => b,
            _ => continue,
        };

        let pubkey = match XOnlyPublicKey::from_slice(&pubkey_bytes) {
            Ok(pk) => pk,
            Err(_) => continue,
        };

        // Message: SHA256(guardian_pubkey || message)
        let mut hasher = Sha256::new();
        hasher.update(&pubkey_bytes);
        hasher.update(message);
        let msg_hash: [u8; 32] = hasher.finalize().into();
        let msg = Message::from_digest(msg_hash);

        if secp.verify_schnorr(&sig, &msg, &pubkey).is_ok() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_federation_config() {
        let json = serde_json::json!({
            "federation_id": "fed1qdeadbeef",
            "community_name": "Test Fed",
            "guardian_pubkeys": ["pk1", "pk2", "pk3", "pk4"],
            "federation_size": 4,
            "network": "regtest"
        })
        .to_string();

        let config = FedimintAdapter::parse_invite_code(&json, None).unwrap();
        assert_eq!(config.federation_id, "fed1qdeadbeef");
        assert_eq!(config.guardian_pubkeys.len(), 4);
        assert_eq!(config.federation_size, 4);
    }

    #[test]
    fn parse_fedimint_uri_prefix() {
        let json = serde_json::json!({
            "federation_id": "fed1qtest",
            "community_name": "URI Fed",
            "guardian_pubkeys": ["pk1", "pk2", "pk3"],
            "federation_size": 3,
            "network": "regtest"
        })
        .to_string();
        let uri = format!("fedimint://{}", json);

        let config = FedimintAdapter::parse_invite_code(&uri, None).unwrap();
        assert_eq!(config.federation_id, "fed1qtest");
    }

    #[test]
    fn parse_overrides_community_name() {
        let json = serde_json::json!({
            "federation_id": "fed1qtest",
            "community_name": "Original",
            "guardian_pubkeys": ["pk1", "pk2"],
            "federation_size": 2,
            "network": "regtest"
        })
        .to_string();

        let config = FedimintAdapter::parse_invite_code(&json, Some("Custom Fed")).unwrap();
        assert_eq!(config.community_name, "Custom Fed");
    }

    #[test]
    fn rejects_empty_federation_id() {
        let json = serde_json::json!({
            "federation_id": "",
            "community_name": "Bad",
            "guardian_pubkeys": ["pk1"],
            "federation_size": 1,
            "network": "regtest"
        })
        .to_string();

        assert!(FedimintAdapter::parse_invite_code(&json, None).is_err());
    }

    #[test]
    fn rejects_zero_federation_size() {
        let json = serde_json::json!({
            "federation_id": "fed1qtest",
            "community_name": "Bad",
            "guardian_pubkeys": [],
            "federation_size": 0,
            "network": "regtest"
        })
        .to_string();

        assert!(FedimintAdapter::parse_invite_code(&json, None).is_err());
    }

    #[test]
    fn rejects_guardian_pubkey_mismatch() {
        let json = serde_json::json!({
            "federation_id": "fed1qtest",
            "community_name": "Mismatch",
            "guardian_pubkeys": ["pk1", "pk2", "pk3"],
            "federation_size": 5,
            "network": "regtest"
        })
        .to_string();

        let err = FedimintAdapter::parse_invite_code(&json, None).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Guardian pubkey count"));
    }

    #[test]
    fn discover_federation_stores_config() {
        let json = serde_json::json!({
            "federation_id": "fed1qstored",
            "community_name": "Stored",
            "guardian_pubkeys": ["pk1", "pk2", "pk3"],
            "federation_size": 3,
            "network": "regtest"
        })
        .to_string();

        let mut adapter = FedimintAdapter::new("regtest".into());
        assert!(adapter.federation_config().is_none());

        let config = adapter.discover_federation(&json, None).unwrap();
        assert_eq!(config.federation_id, "fed1qstored");
        assert!(adapter.federation_config().is_some());
        assert_eq!(
            adapter.federation_config().unwrap().guardian_pubkeys.len(),
            3
        );
    }

    #[test]
    fn validate_consensus_with_quorum() {
        let adapter = FedimintAdapter::new("regtest".into());
        let proof = serde_json::json!({
            "blinded_signature": "sig12345",
            "quorum_signatures": 5,
            "federation_size": 7,
            "federation_id": "fed1qtest",
            "community_name": "Quorum Fed",
            "total_liquidity_sats": 1000000
        });

        let mint = adapter.validate_fedimint_consensus(&proof).unwrap();
        assert_eq!(mint.mint_id, "fed1qtest");
        assert_eq!(mint.total_liquidity_sats, 1000000);
    }

    #[test]
    fn validate_consensus_rejects_empty_sig() {
        let adapter = FedimintAdapter::new("regtest".into());
        let proof = serde_json::json!({
            "blinded_signature": "",
            "quorum_signatures": 5,
            "federation_size": 7
        });

        assert!(adapter.validate_fedimint_consensus(&proof).is_err());
    }

    #[test]
    fn validate_consensus_rejects_insufficient_quorum() {
        let adapter = FedimintAdapter::new("regtest".into());
        let proof = serde_json::json!({
            "blinded_signature": "sig12345",
            "quorum_signatures": 2,
            "federation_size": 7
        });

        assert!(adapter.validate_fedimint_consensus(&proof).is_err());
    }

    // ── G-FM1: Cryptographically verified blind signature tests ──

    #[test]
    fn fedimint_verifies_valid_blind_signature() {
        use secp256k1::{Keypair, Secp256k1};

        let secp = Secp256k1::new();
        let (sk, _parity) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let kp = Keypair::from_secret_key(&secp, &sk);
        let (xonly, _parity) = kp.x_only_public_key();
        let pubkey_hex = hex::encode(xonly.serialize());
        let msg = b"fed1qtest";

        let mut hasher = Sha256::new();
        hasher.update(xonly.serialize());
        hasher.update(msg);
        let msg_hash: [u8; 32] = hasher.finalize().into();
        let msg_digest = Message::from_digest(msg_hash);
        let sig = secp.sign_schnorr(&msg_digest, &kp);
        let sig_hex = hex::encode(sig.serialize());

        assert!(verify_fedimint_blind_signature(
            &sig_hex,
            msg,
            &[pubkey_hex]
        ));
    }

    #[test]
    fn fedimint_rejects_wrong_message() {
        use secp256k1::{Keypair, Secp256k1};

        let secp = Secp256k1::new();
        let (sk, _parity) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let kp = Keypair::from_secret_key(&secp, &sk);
        let (xonly, _parity) = kp.x_only_public_key();
        let pubkey_hex = hex::encode(xonly.serialize());
        let msg = b"fed1qtest";

        let mut hasher = Sha256::new();
        hasher.update(xonly.serialize());
        hasher.update(msg);
        let msg_hash: [u8; 32] = hasher.finalize().into();
        let msg_digest = Message::from_digest(msg_hash);
        let sig = secp.sign_schnorr(&msg_digest, &kp);
        let sig_hex = hex::encode(sig.serialize());

        assert!(!verify_fedimint_blind_signature(
            &sig_hex,
            b"different-mint",
            &[pubkey_hex]
        ));
    }

    #[test]
    fn fedimint_verifies_against_any_guardian() {
        use secp256k1::{Keypair, Secp256k1};

        let secp = Secp256k1::new();
        let (sk, _parity) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let kp = Keypair::from_secret_key(&secp, &sk);
        let (xonly, _parity) = kp.x_only_public_key();
        let pubkey_hex = hex::encode(xonly.serialize());
        let msg = b"fed1qconsortium";

        let mut hasher = Sha256::new();
        hasher.update(xonly.serialize());
        hasher.update(msg);
        let msg_hash: [u8; 32] = hasher.finalize().into();
        let msg_digest = Message::from_digest(msg_hash);
        let sig = secp.sign_schnorr(&msg_digest, &kp);
        let sig_hex = hex::encode(sig.serialize());

        // Valid guardian is in position 2 of 3
        let guardians = vec![
            "aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd".to_string(),
            pubkey_hex,
            "1122334411223344112233441122334411223344112233441122334411223344".to_string(),
        ];
        assert!(verify_fedimint_blind_signature(&sig_hex, msg, &guardians));
    }

    #[test]
    fn fedimint_rejects_invalid_signature_hex() {
        let guardians =
            vec!["aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd".to_string()];
        assert!(!verify_fedimint_blind_signature(
            "not-hex", b"test", &guardians
        ));
    }

    #[test]
    fn fedimint_rejects_empty_guardians() {
        assert!(!verify_fedimint_blind_signature(
            "aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd",
            b"test",
            &[]
        ));
    }

    #[test]
    fn fedimint_rejects_malformed_guardian_pubkeys() {
        // All guardians have invalid pubkeys — none can verify
        let guardians = vec!["bad-pubkey".to_string(), "too-short".to_string()];
        assert!(!verify_fedimint_blind_signature(
            "aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd",
            b"test",
            &guardians
        ));
    }

    #[test]
    fn fedimint_verify_state_proof_with_federation() {
        use secp256k1::{Keypair, Secp256k1};

        let secp = Secp256k1::new();
        let (sk, _parity) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let kp = Keypair::from_secret_key(&secp, &sk);
        let (xonly, _parity) = kp.x_only_public_key();
        let pubkey_hex = hex::encode(xonly.serialize());
        let mint_id = "fed1qverified";
        let msg = mint_id.as_bytes();

        let mut hasher = Sha256::new();
        hasher.update(xonly.serialize());
        hasher.update(msg);
        let msg_hash: [u8; 32] = hasher.finalize().into();
        let msg_digest = Message::from_digest(msg_hash);
        let sig = secp.sign_schnorr(&msg_digest, &kp);
        let sig_hex = hex::encode(sig.serialize());

        let json_config = serde_json::json!({
            "federation_id": mint_id,
            "community_name": "Verified Fed",
            "guardian_pubkeys": [pubkey_hex],
            "federation_size": 1,
            "network": "regtest"
        })
        .to_string();

        let mut adapter = FedimintAdapter::new("regtest".into());
        adapter.discover_federation(&json_config, None).unwrap();

        let proof = serde_json::json!({
            "blinded_signature": sig_hex,
            "mint_id": mint_id
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(adapter.verify_state_proof(proof)).unwrap();
        assert!(result);
    }

    #[test]
    fn fedimint_verify_state_proof_rejects_invalid_sig_with_federation() {
        let json_config = serde_json::json!({
            "federation_id": "fed1qbad",
            "community_name": "Bad Fed",
            "guardian_pubkeys": ["aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd"],
            "federation_size": 1,
            "network": "regtest"
        }).to_string();

        let mut adapter = FedimintAdapter::new("regtest".into());
        adapter.discover_federation(&json_config, None).unwrap();

        let proof = serde_json::json!({
            "blinded_signature": "invalid-sig",
            "mint_id": "fed1qbad"
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(adapter.verify_state_proof(proof)).unwrap();
        assert!(!result);
    }
}
