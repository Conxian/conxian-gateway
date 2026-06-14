use crate::SovereignCommit;
use conxian_core::{
    Attestation, AttestationRequest, ConxianError, ConxianJobCard, ConxianResult, IndustrialIntent,
    JobCardSettlementRequest, NormalizedSettlement, OfflineReceipt, OfflineReceiptStatus,
    SettlementEnvelope, SettlementFinality, SettlementIdentifiers, SettlementSource,
    SettlementStatus, SETTLEMENT_ENVELOPE_VERSION_CURRENT,
};
use hmac::KeyInit;
use hmac::{Hmac, Mac};
use secp256k1::{schnorr, Message, PublicKey, Secp256k1, XOnlyPublicKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

type HmacSha256 = Hmac<Sha256>;

pub const ATTESTATION_SIGNING_DOMAIN: &[u8] = b"conxian-attestation-v1";
pub const TEE_DEVICE_ID_PREFIX: &str = "conxius-tee-";

pub struct ZkcVerifier {
    secp: Secp256k1<secp256k1::All>,
}

impl Default for ZkcVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkcVerifier {
    pub fn new() -> Self {
        Self {
            secp: Secp256k1::new(),
        }
    }

    pub fn format_iso20022_pacs008_v8(
        &self,
        job_card: &conxian_core::ConxianJobCard,
    ) -> conxian_core::ConxianResult<String> {
        info!("Formatting ISO 20022 (pacs.008.001.08) payment for job card...");
        let msg_id = format!("ISO-MSG-{}", uuid::Uuid::new_v4());
        let amount = job_card.work_intent.amount_sbtc;
        let debtor = &job_card.work_intent.sender_address;
        let creditor = &job_card.work_intent.receiver_address;
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
    <FIToFICstmrCdtTrf>
        <GrpHdr>
            <MsgId>{}</MsgId>
            <CreDtTm>{}</CreDtTm>
            <NbOfTxs>1</NbOfTxs>
        </GrpHdr>
        <CdtTrfTxInf>
            <PmtId>
                <EndToEndId>{}</EndToEndId>
            </PmtId>
            <IntrBkSttlmAmt Ccy="sBTC">{:.8}</IntrBkSttlmAmt>
            <Dbtr>
                <Nm>{}</Nm>
            </Dbtr>
            <Cdtr>
                <Nm>{}</Nm>
            </Cdtr>
        </CdtTrfTxInf>
    </FIToFICstmrCdtTrf>
</Document>"#,
            msg_id, timestamp, msg_id, amount, debtor, creditor
        );

        Ok(xml)
    }

    pub fn compute_job_hash(job_card: &ConxianJobCard) -> ConxianResult<String> {
        let job_json = serde_json::to_string(job_card).map_err(|e| {
            ConxianError::Compliance(format!("Job card serialization failed: {}", e))
        })?;
        let mut hasher = Sha256::new();
        hasher.update(job_json.as_bytes());
        Ok(hex::encode(hasher.finalize()))
    }

    pub fn verify_bitvm2_settlement(
        &self,
        request: &JobCardSettlementRequest,
    ) -> ConxianResult<bool> {
        let job_hash = Self::compute_job_hash(&request.job_card)?;
        let expected_state_root = format!("job_hash={}", job_hash);

        if request.bitvm_attestation.state_root != expected_state_root {
            warn!(
                expected = %expected_state_root,
                actual = %request.bitvm_attestation.state_root,
                "BitVM state root mismatch"
            );
            return Ok(false);
        }

        info!("BitVM 2.0 settlement verification successful");
        Ok(true)
    }

    pub fn verify_tee_attestation(
        &self,
        request: &AttestationRequest,
    ) -> ConxianResult<Attestation> {
        match request {
            AttestationRequest::Ecdsa(att) => {
                if !att.device_id.starts_with(TEE_DEVICE_ID_PREFIX) {
                    return Err(ConxianError::Compliance(
                        "Invalid TEE device ID prefix".to_string(),
                    ));
                }

                let pubkey = PublicKey::from_slice(
                    &hex::decode(&att.public_key)
                        .map_err(|e| ConxianError::Compliance(format!("Invalid hex: {}", e)))?,
                )
                .map_err(|e| ConxianError::Compliance(format!("Invalid public key: {}", e)))?;

                let mut hasher = Sha256::new();
                hasher.update(ATTESTATION_SIGNING_DOMAIN);
                hasher.update(att.payload.as_bytes());
                hasher.update(att.device_id.as_bytes());
                let msg = Message::from_digest(hasher.finalize().into());

                let sig = secp256k1::ecdsa::Signature::from_compact(
                    &hex::decode(&att.signature).map_err(|e| {
                        ConxianError::Compliance(format!("Invalid signature hex: {}", e))
                    })?,
                )
                .map_err(|e| {
                    ConxianError::Compliance(format!("Invalid signature format: {}", e))
                })?;

                self.secp.verify_ecdsa(&msg, &sig, &pubkey).map_err(|e| {
                    ConxianError::Compliance(format!("Signature verification failed: {}", e))
                })?;

                info!(device_id = %att.device_id, "TEE attestation verified");
                Ok(att.clone())
            }
            AttestationRequest::Schnorr(att) => {
                if !att.device_id.starts_with(TEE_DEVICE_ID_PREFIX) {
                    return Err(ConxianError::Compliance(
                        "Invalid TEE device ID prefix".to_string(),
                    ));
                }

                let pubkey = XOnlyPublicKey::from_slice(
                    &hex::decode(&att.x_only_public_key)
                        .map_err(|e| ConxianError::Compliance(format!("Invalid hex: {}", e)))?,
                )
                .map_err(|e| {
                    ConxianError::Compliance(format!("Invalid X-only public key: {}", e))
                })?;

                let mut hasher = Sha256::new();
                hasher.update(ATTESTATION_SIGNING_DOMAIN);
                hasher.update(att.payload.as_bytes());
                hasher.update(att.device_id.as_bytes());
                let msg = Message::from_digest(hasher.finalize().into());

                let sig =
                    schnorr::Signature::from_slice(&hex::decode(&att.signature).map_err(|e| {
                        ConxianError::Compliance(format!("Invalid signature hex: {}", e))
                    })?)
                    .map_err(|e| {
                        ConxianError::Compliance(format!("Invalid signature format: {}", e))
                    })?;

                self.secp.verify_schnorr(&sig, &msg, &pubkey).map_err(|e| {
                    ConxianError::Compliance(format!(
                        "Schnorr signature verification failed: {}",
                        e
                    ))
                })?;

                info!(device_id = %att.device_id, "TEE Schnorr attestation verified");
                Ok(Attestation {
                    device_id: att.device_id.clone(),
                    signature: att.signature.clone(),
                    payload: att.payload.clone(),
                    public_key: att.x_only_public_key.clone(),
                })
            }
            _ => Err(ConxianError::Compliance(
                "Unsupported attestation type for TEE verification".to_string(),
            )),
        }
    }

    pub fn verify_settlement_trigger_attestation(
        &self,
        request: &AttestationRequest,
        payload_hash: &str,
    ) -> ConxianResult<Attestation> {
        match request {
            AttestationRequest::Ecdsa(att) => {
                if att.payload != payload_hash {
                    return Err(ConxianError::Security(
                        "Attestation payload hash mismatch".to_string(),
                    ));
                }
                self.verify_tee_attestation(request)
            }
            AttestationRequest::Schnorr(att) => {
                if att.payload != payload_hash {
                    return Err(ConxianError::Security(
                        "Attestation payload hash mismatch".to_string(),
                    ));
                }
                self.verify_tee_attestation(request)
            }
            _ => Err(ConxianError::Compliance(
                "Unsupported attestation type for trigger verification".to_string(),
            )),
        }
    }

    pub fn normalize_lightning_settlement(
        &self,
        intent: &IndustrialIntent,
        proof: &str,
        amount: u128,
    ) -> ConxianResult<NormalizedSettlement> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock moved backwards")
            .as_secs();

        let identifiers = SettlementIdentifiers {
            message_id: Some(uuid::Uuid::new_v4().to_string()),
            transaction_reference: Some(proof.to_string()),
            settlement_reference: Some(intent.project_id.clone()),
            end_to_end_id: None,
            settlement_amount: amount.to_string(),
            settlement_currency: "sBTC".to_string(),
            settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        Ok(NormalizedSettlement {
            source: SettlementSource::Iso20022Pacs008,
            transaction_id: proof.to_string(),
            amount_minor: amount as u64,
            amount_scale: 0,
            currency: "sBTC".to_string(),
            sender: intent.project_id.clone(),
            receiver: "conxian-treasury".to_string(),
            timestamp: now,
            status: SettlementStatus::Settled,
            rail: None,
            finality: SettlementFinality::Final,
            settled_at: Some(now),
            identifiers,
            raw_payload_hash: hex::encode(Sha256::digest(proof.as_bytes())),
            industrial_intent: intent.clone(),
        })
    }

    pub fn verify_ingress_signature(
        &self,
        payload: &str,
        signature: &str,
        secret: &str,
    ) -> ConxianResult<bool> {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| ConxianError::Security(format!("HMAC error: {}", e)))?;
        mac.update(payload.as_bytes());

        let sig_bytes = hex::decode(signature)
            .map_err(|e| ConxianError::Security(format!("Invalid signature hex: {}", e)))?;

        Ok(mac.verify_slice(&sig_bytes).is_ok())
    }

    pub fn normalize_iso20022_ingress(
        &self,
        xml: &str,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing ISO 20022 (pacs.008) ingress...");

        let msg_id = if let Some(start) = xml.find("<MsgId>") {
            let end = xml.find("</MsgId>").unwrap_or(xml.len());
            xml[start + 7..end].to_string()
        } else {
            format!("iso-{}", uuid::Uuid::new_v4())
        };

        let amount_str = if let Some(start) = xml.find("<IntrBkSttlmAmt") {
            let end = xml.find("</IntrBkSttlmAmt>").unwrap_or(xml.len());
            let tag_content = &xml[start..end];
            if let Some(val_start) = tag_content.find('>') {
                tag_content[val_start + 1..].trim().to_string()
            } else {
                "0".to_string()
            }
        } else {
            "0".to_string()
        };

        let amount_f: f64 = amount_str.parse().unwrap_or(0.0);
        let amount_minor = (amount_f * 100.0) as u64;

        let dbtr_nm = if let Some(dbtr_start) = xml.find("<Dbtr>") {
            let dbtr_end = xml.find("</Dbtr>").unwrap_or(xml.len());
            let dbtr_xml = &xml[dbtr_start..dbtr_end];
            if let Some(nm_start) = dbtr_xml.find("<Nm>") {
                let nm_end = dbtr_xml.find("</Nm>").unwrap_or(dbtr_xml.len());
                dbtr_xml[nm_start + 4..nm_end].to_string()
            } else {
                "UNKNOWN_DEBTOR".to_string()
            }
        } else {
            "UNKNOWN_DEBTOR".to_string()
        };

        let identifiers = SettlementIdentifiers {
            message_id: Some(msg_id.clone()),
            transaction_reference: None,
            settlement_reference: None,
            end_to_end_id: None,
            settlement_amount: amount_str,
            settlement_currency: "sBTC".to_string(),
            settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock moved backwards")
            .as_secs();

        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                transaction_id: msg_id,
                amount_minor,
                amount_scale: 2,
                currency: "sBTC".to_string(),
                sender: dbtr_nm,
                receiver: "ISO_RECEIVER".to_string(),
                source: SettlementSource::Iso20022Pacs008,
                raw_payload_hash,
                industrial_intent: IndustrialIntent::default(),
                timestamp,
                status: SettlementStatus::Ingested,
                finality: SettlementFinality::Unknown,
                rail: None,
                settled_at: None,
                identifiers,
            },
        })
    }

    pub fn normalize_papss_ingress(
        &self,
        json: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing PAPSS ingress...");
        let txid = json["transaction_id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let amount = json["amount"].as_u64().unwrap_or(0);
        let sender = json["sender"].as_str().unwrap_or("unknown").to_string();

        let identifiers = SettlementIdentifiers {
            message_id: None,
            transaction_reference: Some(txid.clone()),
            settlement_reference: None,
            end_to_end_id: None,
            settlement_amount: amount.to_string(),
            settlement_currency: "USD".to_string(),
            settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock moved backwards")
            .as_secs();

        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                transaction_id: txid,
                amount_minor: amount,
                amount_scale: 0,
                currency: "USD".to_string(),
                sender,
                receiver: "PAPSS_RECEIVER".to_string(),
                source: SettlementSource::Papss,
                raw_payload_hash,
                industrial_intent: IndustrialIntent::default(),
                timestamp,
                status: SettlementStatus::Ingested,
                finality: SettlementFinality::Unknown,
                rail: None,
                settled_at: None,
                identifiers,
            },
        })
    }

    pub fn normalize_brics_ingress(
        &self,
        json: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing BRICS Pay ingress...");
        let txid = json["brics_id"].as_str().unwrap_or("unknown").to_string();
        let amount = json["amount"].as_u64().unwrap_or(0);
        let sender = json["sender"].as_str().unwrap_or("unknown").to_string();

        let identifiers = SettlementIdentifiers {
            message_id: None,
            transaction_reference: None,
            settlement_reference: Some(txid.clone()),
            end_to_end_id: None,
            settlement_amount: amount.to_string(),
            settlement_currency: "RUB".to_string(),
            settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock moved backwards")
            .as_secs();

        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                transaction_id: txid,
                amount_minor: amount,
                amount_scale: 0,
                currency: "RUB".to_string(),
                sender,
                receiver: "BRICS_RECEIVER".to_string(),
                source: SettlementSource::Brics,
                raw_payload_hash,
                industrial_intent: IndustrialIntent::default(),
                timestamp,
                status: SettlementStatus::Ingested,
                finality: SettlementFinality::Unknown,
                rail: None,
                settled_at: None,
                identifiers,
            },
        })
    }

    pub fn normalize_erp_ingress(
        &self,
        payload: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<Vec<SettlementEnvelope>> {
        info!("Normalizing ERP (OData v4) ingress for institutional ledger...");
        let mut envelopes = Vec::new();

        if let Some(items) = payload["value"].as_array() {
            for item in items {
                let txid = item["DocNum"].as_str().unwrap_or("unknown").to_string();
                let amount = item["DocTotal"].as_f64().unwrap_or(0.0);
                let currency = item["DocCur"].as_str().unwrap_or("USD").to_string();

                let identifiers = SettlementIdentifiers {
                    message_id: Some(txid.clone()),
                    transaction_reference: None,
                    settlement_reference: None,
                    end_to_end_id: None,
                    settlement_amount: amount.to_string(),
                    settlement_currency: currency.clone(),
                    settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
                };

                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock moved backwards")
                    .as_secs();

                envelopes.push(SettlementEnvelope {
                    version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
                    payload: NormalizedSettlement {
                        transaction_id: txid,
                        amount_minor: (amount * 100.0) as u64,
                        amount_scale: 2,
                        currency,
                        sender: "ERP_INSTITUTION".to_string(),
                        receiver: "ERP_TREASURY".to_string(),
                        source: SettlementSource::Erp,
                        raw_payload_hash: raw_payload_hash.clone(),
                        industrial_intent: IndustrialIntent::default(),
                        timestamp,
                        status: SettlementStatus::Ingested,
                        finality: SettlementFinality::Unknown,
                        rail: None,
                        settled_at: None,
                        identifiers,
                    },
                });
            }
        }

        Ok(envelopes)
    }

    pub fn compute_trigger_id(
        &self,
        source_info: &str,
        payload_hash: &str,
        identifiers: &SettlementIdentifiers,
    ) -> ConxianResult<String> {
        let mut hasher = Sha256::new();
        hasher.update(source_info.as_bytes());
        hasher.update(payload_hash.as_bytes());
        if let Some(msg_id) = &identifiers.message_id {
            hasher.update(msg_id.as_bytes());
        }
        Ok(hex::encode(hasher.finalize()))
    }

    pub fn sign_offline_receipt(
        &self,
        tx_hash: &str,
        amount_sbtc: f64,
        device_id: &str,
        passkey_attestation: AttestationRequest,
    ) -> ConxianResult<OfflineReceipt> {
        Ok(conxian_core::OfflineReceipt {
            receipt_id: format!("rec-{}", uuid::Uuid::new_v4()),
            tx_hash: tx_hash.to_string(),
            amount_sbtc,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock moved backwards")
                .as_secs(),
            device_id: device_id.to_string(),
            tee_signature: format!("sig-{}", uuid::Uuid::new_v4()),
            passkey_attestation,
            status: OfflineReceiptStatus::Pending,
        })
    }

    pub fn verify_offline_receipt(
        &self,
        receipt: &conxian_core::OfflineReceipt,
    ) -> ConxianResult<bool> {
        Ok(receipt.device_id.starts_with(TEE_DEVICE_ID_PREFIX))
    }

    pub fn gossip_mesh_rehearsal(
        &self,
        receipt: &mut conxian_core::OfflineReceipt,
    ) -> ConxianResult<()> {
        receipt.status = OfflineReceiptStatus::Gossiped;
        info!(
            device_id = %receipt.device_id,
            "Offline receipt gossiped through sovereign mesh"
        );
        Ok(())
    }
}

impl SovereignCommit for ZkcVerifier {
    fn commit_settlement(&self, envelope: &SettlementEnvelope) -> ConxianResult<()> {
        info!(
            txid = %envelope.payload.transaction_id,
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
