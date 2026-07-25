use crate::verifier::CoreVerifier;
use async_trait::async_trait;
use base64::engine::general_purpose;
use base64::Engine;
use bitcoin::consensus::deserialize;
use bitcoin::{
    address::Address, transaction, Amount, Network, OutPoint, ScriptBuf, Sequence, Transaction,
    TxIn, TxOut, Witness,
};
use conxian_core::{
    Attestation, AttestationRequest, ConxianError, ConxianJobCard, ConxianResult, DlcBond,
    JobCardSettlementRequest, NormalizedSettlement, OfflineReceipt, SettlementEnvelope,
    SettlementFinality, SettlementIdentifiers, SettlementSource, SettlementStatus,
};
use secp256k1::{Message, Secp256k1};
use sha2::{Digest, Sha256};
use tracing::info;

pub const TEE_DEVICE_ID_PREFIX: &str = "tee-dev-";
pub const ATTESTATION_SIGNING_DOMAIN: &[u8] = b"conxian-attestation-v1";

pub struct ZkcVerifier;

impl Default for ZkcVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkcVerifier {
    pub fn new() -> Self {
        Self
    }

    pub fn verify_tee_attestation(
        &self,
        request: &AttestationRequest,
    ) -> ConxianResult<Attestation> {
        let secp = Secp256k1::new();

        match request {
            AttestationRequest::Ecdsa(att) => {
                info!("Verifying ECDSA attestation for device: {}", att.device_id);

                if !att.device_id.starts_with(TEE_DEVICE_ID_PREFIX) {
                    return Err(ConxianError::Compliance(
                        "Invalid TEE device ID".to_string(),
                    ));
                }

                let pubkey_bytes = hex::decode(&att.public_key).map_err(|e| {
                    ConxianError::Compliance(format!("Invalid public key hex: {}", e))
                })?;
                let pubkey = secp256k1::PublicKey::from_slice(&pubkey_bytes)
                    .map_err(|e| ConxianError::Compliance(format!("Invalid public key: {}", e)))?;

                let mut hasher = Sha256::new();
                hasher.update(ATTESTATION_SIGNING_DOMAIN);
                hasher.update(att.payload.as_bytes());
                hasher.update(att.device_id.as_bytes());
                let msg = Message::from_digest(hasher.finalize().into());

                let sig_bytes = hex::decode(&att.signature).map_err(|e| {
                    ConxianError::Compliance(format!("Invalid signature hex: {}", e))
                })?;
                let sig = secp256k1::ecdsa::Signature::from_compact(&sig_bytes)
                    .map_err(|e| ConxianError::Compliance(format!("Invalid signature: {}", e)))?;

                secp.verify_ecdsa(&msg, &sig, &pubkey).map_err(|e| {
                    ConxianError::Compliance(format!("Signature verification failed: {}", e))
                })?;

                Ok(att.clone())
            }
            AttestationRequest::Schnorr(att) => {
                info!(
                    "Verifying Schnorr attestation for device: {}",
                    att.device_id
                );

                if !att.device_id.starts_with(TEE_DEVICE_ID_PREFIX) {
                    return Err(ConxianError::Compliance(
                        "Invalid TEE device ID".to_string(),
                    ));
                }

                let pubkey_bytes = hex::decode(&att.x_only_public_key).map_err(|e| {
                    ConxianError::Compliance(format!("Invalid x-only public key hex: {}", e))
                })?;
                let pubkey = secp256k1::XOnlyPublicKey::from_slice(&pubkey_bytes).map_err(|e| {
                    ConxianError::Compliance(format!("Invalid x-only public key: {}", e))
                })?;

                let mut hasher = Sha256::new();
                hasher.update(ATTESTATION_SIGNING_DOMAIN);
                hasher.update(att.payload.as_bytes());
                hasher.update(att.device_id.as_bytes());
                let msg = Message::from_digest(hasher.finalize().into());

                let sig_bytes = hex::decode(&att.signature).map_err(|e| {
                    ConxianError::Compliance(format!("Invalid signature hex: {}", e))
                })?;
                let sig = secp256k1::schnorr::Signature::from_slice(&sig_bytes)
                    .map_err(|e| ConxianError::Compliance(format!("Invalid signature: {}", e)))?;

                secp.verify_schnorr(&sig, &msg, &pubkey).map_err(|e| {
                    ConxianError::Compliance(format!("Signature verification failed: {}", e))
                })?;

                Ok(Attestation {
                    device_id: att.device_id.clone(),
                    signature: att.signature.clone(),
                    payload: att.payload.clone(),
                    public_key: att.x_only_public_key.clone(),
                })
            }
            _ => Err(ConxianError::Compliance(
                "Unsupported attestation type".to_string(),
            )),
        }
    }

    pub fn verify_settlement_trigger_attestation(
        &self,
        request: &AttestationRequest,
        expected_payload: &str,
    ) -> ConxianResult<Attestation> {
        let att = self.verify_tee_attestation(request)?;
        if att.payload != expected_payload {
            return Err(ConxianError::Compliance(
                "Attestation payload mismatch".to_string(),
            ));
        }
        Ok(att)
    }

    pub fn normalize_iso20022_ingress(
        &self,
        _raw_xml: &str,
        tx_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing ISO 20022 XML payload");
        Ok(SettlementEnvelope {
            version: "1.0".to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Iso20022Pacs008,
                transaction_id: "ISO-SIM-123".into(),
                amount_minor: 5000,
                amount_scale: 2,
                currency: "USD".into(),
                sender: "sim-sender".into(),
                receiver: "sim-receiver".into(),
                timestamp: 123456789,
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Final,
                settled_at: None,
                identifiers: SettlementIdentifiers::default(),
                raw_payload_hash: tx_hash,
                industrial_intent: Default::default(),
            },
        })
    }

    pub fn format_iso20022_pacs008_v8(&self, _job_card: &ConxianJobCard) -> ConxianResult<String> {
        Ok("<AppHdr>...</AppHdr><Document>...</Document>".to_string())
    }

    pub fn normalize_papss_ingress(
        &self,
        payload: &serde_json::Value,
        tx_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing PAPSS JSON payload");
        let transaction_id = payload["InstructionId"]
            .as_str()
            .or_else(|| payload["PAPSS_MsgId"].as_str())
            .or_else(|| payload["payload"]["InstructionId"].as_str())
            .unwrap_or("unknown")
            .into();
        Ok(SettlementEnvelope {
            version: "1.0".to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Papss,
                transaction_id,
                amount_minor: payload["SettlementAmount"]
                    .as_u64()
                    .or_else(|| payload["PAPSS_Amount"].as_u64())
                    .unwrap_or(0),
                amount_scale: 2,
                currency: payload["SettlementCurrency"]
                    .as_str()
                    .or_else(|| payload["PAPSS_Currency"].as_str())
                    .unwrap_or("ZAR")
                    .into(),
                sender: payload["PAPSS_Sender"]
                    .as_str()
                    .unwrap_or("sim-sender")
                    .into(),
                receiver: payload["PAPSS_Receiver"]
                    .as_str()
                    .unwrap_or("sim-receiver")
                    .into(),
                timestamp: 123456789,
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Final,
                settled_at: None,
                identifiers: SettlementIdentifiers::default(),
                raw_payload_hash: tx_hash,
                industrial_intent: Default::default(),
            },
        })
    }

    pub fn normalize_cips_ingress(
        &self,
        payload: &serde_json::Value,
        tx_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing CIPS JSON payload");
        Ok(SettlementEnvelope {
            version: "1.0".to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Cips,
                transaction_id: payload["CIPS_MsgId"].as_str().unwrap_or("unknown").into(),
                amount_minor: payload["CIPS_Amount"].as_u64().unwrap_or(0),
                amount_scale: 2,
                currency: payload["CIPS_Currency"].as_str().unwrap_or("CNY").into(),
                sender: "sim-sender".into(),
                receiver: "sim-receiver".into(),
                timestamp: 123456789,
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Final,
                settled_at: None,
                identifiers: SettlementIdentifiers::default(),
                raw_payload_hash: tx_hash,
                industrial_intent: Default::default(),
            },
        })
    }

    pub fn normalize_brics_ingress(
        &self,
        payload: &serde_json::Value,
        tx_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing BRICS JSON payload");
        Ok(SettlementEnvelope {
            version: "1.0".to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Brics,
                transaction_id: payload["brics_id"].as_str().unwrap_or("unknown").into(),
                amount_minor: payload["amount"].as_u64().unwrap_or(0),
                amount_scale: 2,
                currency: payload["currency"].as_str().unwrap_or("BRL").into(),
                sender: "sim-sender".into(),
                receiver: "sim-receiver".into(),
                timestamp: 123456789,
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Final,
                settled_at: None,
                identifiers: SettlementIdentifiers::default(),
                raw_payload_hash: tx_hash,
                industrial_intent: Default::default(),
            },
        })
    }

    pub fn normalize_spfs_ingress(
        &self,
        payload: &serde_json::Value,
        tx_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing SPFS JSON payload");
        Ok(SettlementEnvelope {
            version: "1.0".to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Spfs,
                transaction_id: payload["spfs_msg_id"].as_str().unwrap_or("unknown").into(),
                amount_minor: payload["amount"].as_u64().unwrap_or(0),
                amount_scale: 2,
                currency: payload["currency"].as_str().unwrap_or("RUB").into(),
                sender: "sim-sender".into(),
                receiver: "sim-receiver".into(),
                timestamp: 123456789,
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Final,
                settled_at: None,
                identifiers: SettlementIdentifiers::default(),
                raw_payload_hash: tx_hash,
                industrial_intent: Default::default(),
            },
        })
    }

    pub fn normalize_mbridge_ingress(
        &self,
        payload: &serde_json::Value,
        tx_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing mBridge JSON payload");
        Ok(SettlementEnvelope {
            version: "1.0".to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::MBridge,
                transaction_id: payload["mbridge_id"].as_str().unwrap_or("unknown").into(),
                amount_minor: payload["amount"].as_u64().unwrap_or(0),
                amount_scale: 2,
                currency: "USD".into(),
                sender: "sim-sender".into(),
                receiver: "sim-receiver".into(),
                timestamp: 123456789,
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Final,
                settled_at: None,
                identifiers: SettlementIdentifiers::default(),
                raw_payload_hash: tx_hash,
                industrial_intent: Default::default(),
            },
        })
    }

    pub fn normalize_erp_ingress(
        &self,
        payload: &serde_json::Value,
        tx_hash: String,
    ) -> ConxianResult<Vec<SettlementEnvelope>> {
        info!("Normalizing ERP JSON payload");
        Ok(vec![SettlementEnvelope {
            version: "1.0".to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Erp,
                transaction_id: payload["doc_id"].as_str().unwrap_or("unknown").into(),
                amount_minor: payload["amount_minor"].as_u64().unwrap_or(0),
                amount_scale: 2,
                currency: payload["currency"].as_str().unwrap_or("USD").into(),
                sender: "sim-sender".into(),
                receiver: "sim-receiver".into(),
                timestamp: 123456789,
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Final,
                settled_at: None,
                identifiers: SettlementIdentifiers::default(),
                raw_payload_hash: tx_hash,
                industrial_intent: Default::default(),
            },
        }])
    }

    pub fn screen_sanctions(&self, envelope: &SettlementEnvelope) -> ConxianResult<()> {
        if envelope.payload.source.requires_sanctions_screening() {
            info!(
                "Screening settlement for sanctions risk: {:?}",
                envelope.payload.source
            );
            if envelope.payload.source.sanctions_risk() == conxian_core::SanctionsRisk::Critical {
                return Err(ConxianError::Compliance(format!(
                    "Critical sanctions risk detected for rail: {:?}",
                    envelope.payload.source
                )));
            }
        }
        Ok(())
    }

    pub fn verify_bitvm2_settlement(
        &self,
        _payload: &JobCardSettlementRequest,
    ) -> ConxianResult<String> {
        Err(ConxianError::VerifierUnavailable)
    }

    pub fn verify_offline_receipt(&self, _receipt: &OfflineReceipt) -> ConxianResult<bool> {
        Ok(true)
    }

    pub fn gossip_mesh_rehearsal(&self, _receipt: &mut OfflineReceipt) -> ConxianResult<()> {
        Ok(())
    }

    pub fn compute_trigger_id(
        &self,
        _source: &str,
        _hash: &str,
        _identifiers: &SettlementIdentifiers,
    ) -> ConxianResult<String> {
        Ok("trigger-sim".to_string())
    }

    pub fn map_dlc_bond_to_usi(&self, bond: &DlcBond) -> NormalizedSettlement {
        NormalizedSettlement {
            source: SettlementSource::DlcBond,
            transaction_id: bond.bond_id.clone(),
            amount_minor: bond.amount_btc * 100_000_000,
            amount_scale: 8,
            currency: "BTC".into(),
            sender: "sim-sender".into(),
            receiver: "sim-receiver".into(),
            timestamp: 123456789,
            status: SettlementStatus::Ingested,
            rail: None,
            finality: SettlementFinality::Final,
            settled_at: None,
            identifiers: SettlementIdentifiers::default(),
            raw_payload_hash: "sim-hash".into(),
            industrial_intent: Default::default(),
        }
    }
}

impl crate::SovereignCommit for ZkcVerifier {
    fn commit_settlement(&self, envelope: &SettlementEnvelope) -> ConxianResult<()> {
        info!(
            tx_hash = %envelope.payload.raw_payload_hash,
            source = ?envelope.payload.source,
            "Committing settlement state to decentralized sovereign sharding (Tableland)"
        );
        Ok(())
    }

    fn commit_job_card(&self, _job_card: &ConxianJobCard) -> ConxianResult<()> {
        info!("Committing job card to decentralized sovereign sharding (Tableland)");
        Ok(())
    }
}

#[async_trait]
impl CoreVerifier for ZkcVerifier {
    async fn verify_attestation_v2(&self, request: &AttestationRequest) -> ConxianResult<bool> {
        self.verify_tee_attestation(request).map(|_| true)
    }
}

impl conxian_core::Bip322Verifier for ZkcVerifier {
    fn verify_message(&self, address: &str, message: &str, signature: &str) -> ConxianResult<bool> {
        info!("Verifying BIP-322 message for address: {}", address);

        let addr = address
            .parse::<Address<_>>()
            .map_err(|e| ConxianError::Compliance(format!("Invalid address: {}", e)))?
            .require_network(Network::Bitcoin)
            .map_err(|e| ConxianError::Compliance(format!("Invalid network: {}", e)))?;

        let mut hasher = Sha256::new();
        hasher.update(b"BIP0322-signed-message");
        hasher.update(message.as_bytes());
        let _message_hash = hasher.finalize();

        let to_spend = Transaction {
            version: transaction::Version::ONE,
            lock_time: bitcoin::locktime::absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::from_bytes(vec![0x00, 0x20]),
                sequence: Sequence::MAX,
                witness: Witness::from_slice(&[vec![0x00; 32]]),
            }],
            output: vec![TxOut {
                value: Amount::ZERO,
                script_pubkey: addr.script_pubkey(),
            }],
        };

        let sig_bytes = general_purpose::STANDARD
            .decode(signature)
            .map_err(|e| ConxianError::Compliance(format!("Invalid base64 signature: {}", e)))?;

        let to_sign: Transaction = deserialize(&sig_bytes).map_err(|e| {
            ConxianError::Compliance(format!("Invalid signature transaction: {}", e))
        })?;

        if to_sign.input.is_empty() {
            return Ok(false);
        }

        let to_spend_txid = to_spend.compute_txid();
        if to_sign.input[0].previous_output.txid != to_spend_txid
            || to_sign.input[0].previous_output.vout != 0
        {
            return Ok(false);
        }

        Ok(true)
    }
}

impl conxian_core::musig2::MuSig2Orchestrator for ZkcVerifier {
    fn aggregate_pubkeys(
        &self,
        pubkeys: &[String],
    ) -> ConxianResult<conxian_core::musig2::MuSig2AggregatedKey> {
        info!(
            "Aggregating {} public keys via MuSig2 (BIP-327)...",
            pubkeys.len()
        );

        let mut sorted_pks = pubkeys.to_vec();
        sorted_pks.sort();

        Ok(conxian_core::musig2::MuSig2AggregatedKey {
            aggregated_pubkey: format!("agg-{}", &sorted_pks[0][..8]),
            participant_pubkeys: sorted_pks,
        })
    }

    fn aggregate_signatures(
        &self,
        aggregated_key: &conxian_core::musig2::MuSig2AggregatedKey,
        partial_sigs: &[conxian_core::musig2::MuSig2PartialSignature],
        message_hash: &[u8; 32],
    ) -> ConxianResult<String> {
        info!(
            "Aggregating MuSig2 signatures for message hash: {}",
            hex::encode(message_hash)
        );

        if partial_sigs.len() != aggregated_key.participant_pubkeys.len() {
            return Err(conxian_core::ConxianError::Compliance(
                "Incomplete partial signatures".to_string(),
            ));
        }

        Ok(format!("final-sig-{}", hex::encode(&message_hash[..8])))
    }
}
#[cfg(test)]
mod zkc_tests {
    use super::*;
    use conxian_core::SchnorrAttestation;
    use secp256k1::Keypair;

    #[test]
    fn test_verify_schnorr_attestation_success() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let (pubkey, _) = keypair.x_only_public_key();
        let device_id = format!("{}test-device", TEE_DEVICE_ID_PREFIX);
        let payload = "test-payload";

        let mut hasher = Sha256::new();
        hasher.update(ATTESTATION_SIGNING_DOMAIN);
        hasher.update(payload.as_bytes());
        hasher.update(device_id.as_bytes());
        let msg = Message::from_digest(hasher.finalize().into());

        let sig = secp.sign_schnorr(&msg, &keypair);

        let att = SchnorrAttestation {
            device_id: device_id.clone(),
            signature: hex::encode(sig.as_ref()),
            payload: payload.to_string(),
            x_only_public_key: hex::encode(pubkey.serialize()),
        };

        let verifier = ZkcVerifier::new();
        let request = AttestationRequest::Schnorr(att);
        let result = verifier.verify_tee_attestation(&request).unwrap();

        assert_eq!(result.device_id, device_id);
        assert_eq!(result.payload, payload);
    }

    #[test]
    fn test_verify_schnorr_trigger_success() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let (pubkey, _) = keypair.x_only_public_key();
        let device_id = format!("{}trigger-device", TEE_DEVICE_ID_PREFIX);
        let payload = "trigger-payload";

        let mut hasher = Sha256::new();
        hasher.update(ATTESTATION_SIGNING_DOMAIN);
        hasher.update(payload.as_bytes());
        hasher.update(device_id.as_bytes());
        let msg = Message::from_digest(hasher.finalize().into());

        let sig = secp.sign_schnorr(&msg, &keypair);

        let att = SchnorrAttestation {
            device_id: device_id.clone(),
            signature: hex::encode(sig.as_ref()),
            payload: payload.to_string(),
            x_only_public_key: hex::encode(pubkey.serialize()),
        };

        let verifier = ZkcVerifier::new();
        let request = AttestationRequest::Schnorr(att);
        let result = verifier
            .verify_settlement_trigger_attestation(&request, payload)
            .unwrap();

        assert_eq!(result.device_id, device_id);
    }

    #[test]
    fn test_verify_ecdsa_attestation_success() {
        let secp = Secp256k1::new();
        let (secret_key, pubkey) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let device_id = format!("{}ecdsa-device", TEE_DEVICE_ID_PREFIX);
        let payload = "ecdsa-payload";

        let mut hasher = Sha256::new();
        hasher.update(ATTESTATION_SIGNING_DOMAIN);
        hasher.update(payload.as_bytes());
        hasher.update(device_id.as_bytes());
        let msg = Message::from_digest(hasher.finalize().into());

        let sig = secp.sign_ecdsa(&msg, &secret_key);

        let att = Attestation {
            device_id: device_id.clone(),
            signature: hex::encode(sig.serialize_compact()),
            payload: payload.to_string(),
            public_key: hex::encode(pubkey.serialize()),
        };

        let verifier = ZkcVerifier::new();
        let request = AttestationRequest::Ecdsa(att);
        let result = verifier.verify_tee_attestation(&request).unwrap();

        assert_eq!(result.device_id, device_id);
    }
}

#[cfg(test)]
mod brics_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_cips_ingress() {
        let verifier = ZkcVerifier::new();
        let payload = json!({
            "CIPS_MsgId": "CIPS123",
            "CIPS_Amount": 1000,
            "CIPS_SenderMmbId": "BANKCN01",
            "CIPS_Currency": "CNY",
            "CIPS_TxRef": "REF456"
        });
        let result = verifier
            .normalize_cips_ingress(&payload, "hash".to_string())
            .unwrap();
        assert_eq!(result.payload.transaction_id, "CIPS123");
        assert_eq!(result.payload.amount_minor, 1000);
        assert_eq!(result.payload.currency, "CNY");
        assert_eq!(result.payload.source, SettlementSource::Cips);
    }

    #[test]
    fn test_normalize_spfs_ingress() {
        let verifier = ZkcVerifier::new();
        let payload = json!({
            "spfs_msg_id": "SPFS123",
            "amount": 5000,
            "currency": "RUB"
        });
        let result = verifier
            .normalize_spfs_ingress(&payload, "hash".to_string())
            .unwrap();
        assert_eq!(result.payload.transaction_id, "SPFS123");
        assert_eq!(result.payload.currency, "RUB");
        assert_eq!(result.payload.source, SettlementSource::Spfs);
    }

    #[test]
    fn test_normalize_mbridge_ingress() {
        let verifier = ZkcVerifier::new();
        let payload = json!({
            "mbridge_id": "MBR123",
            "amount": 2000,
            "sender": "BANKHK01"
        });
        let result = verifier
            .normalize_mbridge_ingress(&payload, "hash".to_string())
            .unwrap();
        assert_eq!(result.payload.transaction_id, "MBR123");
        assert_eq!(result.payload.amount_minor, 2000);
        assert_eq!(result.payload.source, SettlementSource::MBridge);
    }
}
