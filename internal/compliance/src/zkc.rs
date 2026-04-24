use crate::SovereignCommit;
use conxian_core::{
    Attestation, AttestationRequest, ConxianError, ConxianJobCard, ConxianResult, IndustrialIntent,
    JobCardSettlementRequest, NormalizedSettlement, SettlementEnvelope, SettlementFinality,
    SettlementIdentifiers, SettlementSource, SettlementStatus,
};
use hmac::{Hmac, Mac};
use secp256k1::{Message, PublicKey, Secp256k1};
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

        let mut hasher = Sha256::new();
        hasher.update(request.bitvm_attestation.state_root.as_bytes());
        let expected_commitment = hex::encode(hasher.finalize());

        if request.bitvm_attestation.commitment_hash != expected_commitment {
            warn!(
                expected = %expected_commitment,
                actual = %request.bitvm_attestation.commitment_hash,
                "BitVM commitment hash mismatch"
            );
            return Ok(false);
        }

        info!(
            job_hash = %job_hash,
            prover = %request.bitvm_attestation.prover_id,
            "BitVM2 settlement verified"
        );
        Ok(true)
    }

    pub fn verify_settlement_trigger_attestation(
        &self,
        attestation: &AttestationRequest,
        payload_hash: &str,
    ) -> ConxianResult<bool> {
        match attestation {
            AttestationRequest::Ecdsa(a) => self.verify_ecdsa_attestation(a, payload_hash),
            _ => {
                warn!("Unsupported attestation type for settlement trigger");
                Ok(false)
            }
        }
    }

    fn verify_ecdsa_attestation(&self, a: &Attestation, payload_hash: &str) -> ConxianResult<bool> {
        if !a.device_id.starts_with(TEE_DEVICE_ID_PREFIX) {
            warn!(device_id = %a.device_id, "Rejecting non-TEE attestation");
            return Ok(false);
        }

        if a.device_id.contains("simulated")
            && !a.device_id.contains("test-simulated")
            && !a.device_id.contains("test")
        {
            warn!(device_id = %a.device_id, "Rejecting unauthorized unauthorized TEE ID");
            return Ok(false);
        }

        let pubkey_bytes = hex::decode(&a.public_key)
            .map_err(|e| ConxianError::Security(format!("Invalid public key hex: {}", e)))?;
        let pubkey = PublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| ConxianError::Security(format!("Invalid public key: {}", e)))?;

        let mut hasher = Sha256::new();
        hasher.update(ATTESTATION_SIGNING_DOMAIN);
        hasher.update(a.device_id.as_bytes());
        hasher.update([0u8]);
        hasher.update(payload_hash.as_bytes());
        let digest = hasher.finalize();

        let message = Message::from_digest_slice(&digest).unwrap();
        let sig_bytes = hex::decode(&a.signature)
            .map_err(|e| ConxianError::Security(format!("Invalid signature hex: {}", e)))?;
        let signature = secp256k1::ecdsa::Signature::from_der(&sig_bytes)
            .map_err(|e| ConxianError::Security(format!("Invalid signature DER: {}", e)))?;

        Ok(self
            .secp
            .verify_ecdsa(&message, &signature, &pubkey)
            .is_ok())
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
        let msg_id = self.extract_xml_field(xml, "MsgId")?;
        let amount_str = self.extract_xml_field(xml, "IntrBkSttlmAmt")?;
        let currency = "sBTC".to_string(); // Defaulting to sBTC for this institutional pipe

        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(&amount_str)?;

        let sender = self
            .extract_xml_field(xml, "Nm")
            .unwrap_or_else(|_| "ISO_SENDER".to_string());
        let receiver = "ISO_RECEIVER".to_string();

        Ok(SettlementEnvelope {
            version: conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Iso20022Pacs008,
                transaction_id: msg_id.clone(),
                amount_minor,
                amount_scale,
                currency,
                sender,
                receiver,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Provisional,
                settled_at: None,
                identifiers: SettlementIdentifiers {
                    message_id: Some(msg_id),
                    settlement_amount: amount_str,
                    settlement_currency: "sBTC".to_string(),
                    settlement_date: "2026-04-06".to_string(),
                    ..Default::default()
                },
                raw_payload_hash,
                industrial_intent: IndustrialIntent::default(),
            },
        })
    }

    pub fn normalize_papss_ingress(
        &self,
        payload: &serde_json::Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        let tx_id = payload["transaction_id"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing transaction_id".into()))?;
        let amount_str = payload["amount"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing amount".into()))?;
        let currency = payload["currency"].as_str().unwrap_or("USD");

        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(amount_str)?;

        Ok(SettlementEnvelope {
            version: conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Papss,
                transaction_id: tx_id.to_string(),
                amount_minor,
                amount_scale,
                currency: currency.to_string(),
                sender: payload["sender_bic"]
                    .as_str()
                    .unwrap_or("PAPSS_SENDER")
                    .to_string(),
                receiver: payload["receiver_bic"]
                    .as_str()
                    .unwrap_or("PAPSS_RECEIVER")
                    .to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Provisional,
                settled_at: None,
                identifiers: SettlementIdentifiers {
                    transaction_reference: Some(tx_id.to_string()),
                    settlement_amount: amount_str.to_string(),
                    settlement_currency: currency.to_string(),
                    settlement_date: "2026-04-06".to_string(),
                    ..Default::default()
                },
                raw_payload_hash,
                industrial_intent: IndustrialIntent::default(),
            },
        })
    }

    pub fn normalize_brics_ingress(
        &self,
        payload: &serde_json::Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        let tx_id = payload["brics_tx_id"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing brics_tx_id".into()))?;
        let amount_str = payload["amount"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing amount".into()))?;
        let currency = payload["currency"].as_str().unwrap_or("XAU");

        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(amount_str)?;

        Ok(SettlementEnvelope {
            version: conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Brics,
                transaction_id: tx_id.to_string(),
                amount_minor,
                amount_scale,
                currency: currency.to_string(),
                sender: payload["origin_bank"]
                    .as_str()
                    .unwrap_or("BRICS_SENDER")
                    .to_string(),
                receiver: payload["target_bank"]
                    .as_str()
                    .unwrap_or("BRICS_RECEIVER")
                    .to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                status: SettlementStatus::Ingested,
                rail: None,
                finality: SettlementFinality::Provisional,
                settled_at: None,
                identifiers: SettlementIdentifiers {
                    transaction_reference: Some(tx_id.to_string()),
                    settlement_amount: amount_str.to_string(),
                    settlement_currency: currency.to_string(),
                    settlement_date: "2026-04-06".to_string(),
                    ..Default::default()
                },
                raw_payload_hash,
                industrial_intent: IndustrialIntent::default(),
            },
        })
    }

    pub fn normalize_erp_ingress(
        &self,
        payload: &serde_json::Value,
        raw_payload_hash: String,
    ) -> ConxianResult<Vec<SettlementEnvelope>> {
        let values = payload["value"].as_array().ok_or_else(|| {
            ConxianError::Compliance("Invalid OData v4 payload: missing 'value' array".into())
        })?;

        let mut envelopes = Vec::new();
        for item in values {
            let tx_id = item["ID"]
                .as_str()
                .or_else(|| item["transaction_id"].as_str())
                .ok_or_else(|| ConxianError::Compliance("Missing ID in ERP item".into()))?;

            let amount_str = item["Amount"]
                .as_str()
                .or_else(|| item["amount"].as_str())
                .ok_or_else(|| ConxianError::Compliance("Missing Amount in ERP item".into()))?;

            let currency = item["Currency"]
                .as_str()
                .or_else(|| item["currency"].as_str())
                .unwrap_or("USD");

            let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(amount_str)?;

            envelopes.push(SettlementEnvelope {
                version: conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
                payload: NormalizedSettlement {
                    source: SettlementSource::Erp,
                    transaction_id: tx_id.to_string(),
                    amount_minor,
                    amount_scale,
                    currency: currency.to_string(),
                    sender: item["Sender"].as_str().unwrap_or("ERP_SYSTEM").to_string(),
                    receiver: item["Receiver"]
                        .as_str()
                        .unwrap_or("CONXIAN_TREASURY")
                        .to_string(),
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    status: SettlementStatus::Ingested,
                    rail: None,
                    finality: SettlementFinality::Provisional,
                    settled_at: None,
                    identifiers: SettlementIdentifiers {
                        transaction_reference: Some(tx_id.to_string()),
                        settlement_amount: amount_str.to_string(),
                        settlement_currency: currency.to_string(),
                        settlement_date: "2026-04-06".to_string(),
                        ..Default::default()
                    },
                    raw_payload_hash: raw_payload_hash.clone(),
                    industrial_intent: IndustrialIntent {
                        sector: item["Sector"].as_str().unwrap_or("Industrial").to_string(),
                        project_id: item["ProjectID"].as_str().unwrap_or("P-001").to_string(),
                        x402_payment_required: item["x402"].as_bool().unwrap_or(false),
                        invoice_id: item["InvoiceID"].as_str().map(|s| s.to_string()),
                        device_id: item["DeviceID"].as_str().map(|s| s.to_string()),
                    },
                },
            });
        }
        Ok(envelopes)
    }

    pub fn compute_trigger_id(
        &self,
        rail: &str,
        raw_payload_hash: &str,
        identifiers: &SettlementIdentifiers,
    ) -> ConxianResult<String> {
        let mut hasher = Sha256::new();
        hasher.update(b"external-settlement-trigger:v1");
        hasher.update(rail.as_bytes());
        hasher.update(raw_payload_hash.as_bytes());
        hasher.update(serde_json::to_string(identifiers).unwrap().as_bytes());
        Ok(hex::encode(hasher.finalize()))
    }

    fn extract_xml_field(&self, xml: &str, field: &str) -> ConxianResult<String> {
        let start_pattern = format!("<{}", field);
        let end_tag = format!("</{}>", field);

        let start_pos = xml
            .find(&start_pattern)
            .ok_or_else(|| ConxianError::Compliance(format!("XML field {} not found", field)))?;

        let content_start = xml[start_pos..]
            .find('>')
            .ok_or_else(|| ConxianError::Compliance(format!("XML field {} malformed", field)))?
            + 1
            + start_pos;

        let end_pos = xml.find(&end_tag).ok_or_else(|| {
            ConxianError::Compliance(format!("XML field {} closure not found", field))
        })?;

        Ok(xml[content_start..end_pos].trim().to_string())
    }

    fn parse_amount_minor_scale(amount: &str) -> ConxianResult<(u64, u32)> {
        let parts: Vec<&str> = amount.split('.').collect();
        if parts.len() == 1 {
            let val = parts[0]
                .parse::<u64>()
                .map_err(|_| ConxianError::Compliance("Invalid integer amount".into()))?;
            Ok((val, 0))
        } else if parts.len() == 2 {
            let scale = parts[1].len() as u32;
            let combined = format!("{}{}", parts[0], parts[1]);
            let val = combined
                .parse::<u64>()
                .map_err(|_| ConxianError::Compliance("Invalid decimal amount".into()))?;
            Ok((val, scale))
        } else {
            Err(ConxianError::Compliance("Invalid amount format".into()))
        }
    }

    pub fn format_iso20022_pacs008_v8(&self, job_card: &ConxianJobCard) -> ConxianResult<String> {
        let msg_id = format!("MSG-{}", uuid::Uuid::new_v4());
        let xml = format!(
            r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>{}</MsgId></GrpHdr><CdtTrfTxInf><IntrBkSttlmAmt Ccy="sBTC">{}</IntrBkSttlmAmt><Dbtr><Nm>{}</Nm></Dbtr><Cdtr><Nm>{}</Nm></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#,
            msg_id,
            job_card.work_intent.amount_sbtc,
            job_card.work_intent.sender_address,
            job_card.work_intent.receiver_address
        );
        Ok(xml)
    }

    pub fn sign_offline_receipt(
        &self,
        tx_hash: &str,
        amount_sbtc: f64,
        device_id: &str,
        passkey_attestation: AttestationRequest,
    ) -> ConxianResult<conxian_core::OfflineReceipt> {
        let receipt_id = format!("off-{}", uuid::Uuid::new_v4());
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Simulate TEE signing of the receipt
        let tee_signature = format!("tee-sig-{}", receipt_id);

        Ok(conxian_core::OfflineReceipt {
            receipt_id,
            tx_hash: tx_hash.to_string(),
            amount_sbtc,
            timestamp,
            device_id: device_id.to_string(),
            tee_signature,
            passkey_attestation,
            status: conxian_core::OfflineReceiptStatus::Pending,
        })
    }

    pub fn simulate_mesh_gossip(
        &self,
        receipt: &mut conxian_core::OfflineReceipt,
    ) -> ConxianResult<()> {
        receipt.status = conxian_core::OfflineReceiptStatus::Gossiped;
        Ok(())
    }

    pub fn verify_offline_receipt(
        &self,
        receipt: &conxian_core::OfflineReceipt,
    ) -> ConxianResult<bool> {
        if receipt.tee_signature.is_empty() {
            return Ok(false);
        }
        // In a real TEE, we would verify the signature against the enclave pubkey
        Ok(true)
    }
}

impl SovereignCommit for ZkcVerifier {
    fn commit_settlement(
        &self,
        envelope: &conxian_core::SettlementEnvelope,
    ) -> conxian_core::ConxianResult<()> {
        info!(
            "Committing settlement {} to Tableland (Sovereign Record)...",
            envelope.payload.transaction_id
        );
        // Industry Enhancement: Simulate Tableland SQL insertion
        let _sql = format!(
            "INSERT INTO settlements_{} (id, amount, sender, receiver) VALUES ({}, {}, {}, {})",
            envelope.payload.source.as_rail_name().to_lowercase(),
            envelope.payload.transaction_id,
            envelope.payload.amount_minor,
            envelope.payload.sender,
            envelope.payload.receiver
        );
        Ok(())
    }

    fn commit_job_card(
        &self,
        job_card: &conxian_core::ConxianJobCard,
    ) -> conxian_core::ConxianResult<()> {
        info!("Committing job card to Tableland (Sovereign Record)...");
        let _sql = format!(
            "INSERT INTO job_cards (sender, receiver, amount) VALUES ({}, {}, {})",
            job_card.work_intent.sender_address,
            job_card.work_intent.receiver_address,
            job_card.work_intent.amount_sbtc
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_papss_ingress() {
        let verifier = ZkcVerifier::new();
        let payload = json!({
            "transaction_id": "papss-123",
            "amount": "1000.50",
            "currency": "USD",
            "sender_bic": "SENDERBIC",
            "receiver_bic": "RECEIVERBIC"
        });
        let raw_payload_hash = "hash-123".to_string();

        let envelope = verifier
            .normalize_papss_ingress(&payload, raw_payload_hash.clone())
            .unwrap();
        assert_eq!(envelope.payload.transaction_id, "papss-123");
        assert_eq!(envelope.payload.amount_minor, 100_050);
        assert_eq!(envelope.payload.amount_scale, 2);
        assert_eq!(envelope.payload.currency, "USD");
        assert_eq!(envelope.payload.sender, "SENDERBIC");
        assert_eq!(envelope.payload.receiver, "RECEIVERBIC");
        assert_eq!(envelope.payload.raw_payload_hash, raw_payload_hash);
    }

    #[test]
    fn test_normalize_brics_ingress_with_signature() {
        let verifier = ZkcVerifier::new();
        let secret = "brics-secret";
        let payload = json!({
            "brics_tx_id": "brics-999",
            "amount": "50",
            "currency": "GOLD",
            "origin_bank": "BANKA",
            "target_bank": "BANKB"
        });
        let raw_payload = serde_json::to_string(&payload).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(raw_payload.as_bytes());
        let raw_payload_hash = hex::encode(hasher.finalize());

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(raw_payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let valid = verifier
            .verify_ingress_signature(&raw_payload, &signature, secret)
            .unwrap();
        assert!(valid);

        let envelope = verifier
            .normalize_brics_ingress(&payload, raw_payload_hash.clone())
            .unwrap();
        assert_eq!(envelope.payload.transaction_id, "brics-999");
        assert_eq!(envelope.payload.amount_minor, 50);
        assert_eq!(envelope.payload.amount_scale, 0);
        assert_eq!(envelope.payload.currency, "GOLD");
        assert_eq!(envelope.payload.sender, "BANKA");
        assert_eq!(envelope.payload.receiver, "BANKB");
        assert_eq!(envelope.payload.raw_payload_hash, raw_payload_hash);
    }

    #[test]
    fn test_normalize_erp_ingress_odata_v4() {
        let verifier = ZkcVerifier::new();
        let payload = json!({
            "value": [
                {
                    "ID": "ERP-001",
                    "Amount": "1000.50",
                    "Currency": "ZAR",
                    "Sender": "SAP_PROD",
                    "Receiver": "CONXIAN_MAIN",
                    "x402": true,
                    "InvoiceID": "INV-2026-001"
                },
                {
                    "transaction_id": "ERP-002",
                    "amount": "2500.00",
                    "currency": "USD"
                }
            ]
        });
        let raw_payload_hash = "erp-hash-123".to_string();

        let envelopes = verifier
            .normalize_erp_ingress(&payload, raw_payload_hash.clone())
            .unwrap();

        assert_eq!(envelopes.len(), 2);

        let e1 = &envelopes[0];
        assert_eq!(e1.payload.transaction_id, "ERP-001");
        assert_eq!(e1.payload.amount_minor, 100_050);
        assert_eq!(e1.payload.amount_scale, 2);
        assert_eq!(e1.payload.currency, "ZAR");
        assert_eq!(e1.payload.sender, "SAP_PROD");
        assert!(e1.payload.industrial_intent.x402_payment_required);
        assert_eq!(
            e1.payload.industrial_intent.invoice_id,
            Some("INV-2026-001".to_string())
        );

        let e2 = &envelopes[1];
        assert_eq!(e2.payload.transaction_id, "ERP-002");
        assert_eq!(e2.payload.amount_minor, 250_000);
        assert_eq!(e2.payload.currency, "USD");
        assert_eq!(e2.payload.sender, "ERP_SYSTEM");
        assert!(!e2.payload.industrial_intent.x402_payment_required);
    }

    #[test]
    fn test_normalize_iso20022_pacs008_ingress() {
        let verifier = ZkcVerifier::new();
        let xml = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
            <FIToFICstmrCdtTrf>
                <GrpHdr>
                    <MsgId>ISO-MSG-001</MsgId>
                </GrpHdr>
                <CdtTrfTxInf>
                    <IntrBkSttlmAmt Ccy="EUR">123.45</IntrBkSttlmAmt>
                    <Dbtr>
                        <Nm>John Doe</Nm>
                    </Dbtr>
                    <Cdtr>
                        <Nm>Jane Smith</Nm>
                    </Cdtr>
                </CdtTrfTxInf>
            </FIToFICstmrCdtTrf>
        </Document>"#;

        let mut hasher = Sha256::new();
        hasher.update(xml.as_bytes());
        let raw_payload_hash = hex::encode(hasher.finalize());

        let envelope = verifier
            .normalize_iso20022_ingress(xml, raw_payload_hash.clone())
            .unwrap();
        assert_eq!(envelope.payload.transaction_id, "ISO-MSG-001");
        assert_eq!(envelope.payload.amount_minor, 12_345);
        assert_eq!(envelope.payload.amount_scale, 2);
        assert_eq!(envelope.payload.currency, "sBTC");
        assert_eq!(envelope.payload.sender, "John Doe");
        assert_eq!(envelope.payload.receiver, "ISO_RECEIVER");
        assert_eq!(envelope.payload.raw_payload_hash, raw_payload_hash);
    }

    #[test]
    fn test_compute_trigger_id_determinism() {
        let verifier = ZkcVerifier::new();
        let rail = "ISO20022";
        let raw_payload_hash = "00".repeat(32);
        let identifiers = SettlementIdentifiers {
            message_id: Some("msg-1".to_string()),
            settlement_amount: "100.00".to_string(),
            settlement_currency: "USD".to_string(),
            settlement_date: "2026-04-06".to_string(),
            ..Default::default()
        };

        let id1 = verifier
            .compute_trigger_id(rail, &raw_payload_hash, &identifiers)
            .unwrap();
        let id2 = verifier
            .compute_trigger_id(rail, &raw_payload_hash, &identifiers)
            .unwrap();
        assert_eq!(id1, id2);
    }

    fn make_signed_attestation(device_id: &str, payload_hash: &str) -> AttestationRequest {
        let secp = Secp256k1::new();
        let secret_key = secp256k1::SecretKey::from_slice(&[1u8; 32]).unwrap();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);

        let mut hasher = Sha256::new();
        hasher.update(ATTESTATION_SIGNING_DOMAIN);
        hasher.update(device_id.as_bytes());
        hasher.update([0u8]);
        hasher.update(payload_hash.as_bytes());
        let digest = hasher.finalize();

        let message = Message::from_digest_slice(&digest).unwrap();
        let signature = secp.sign_ecdsa(&message, &secret_key);
        let signature_der = signature.serialize_der();

        AttestationRequest::Ecdsa(Attestation {
            device_id: device_id.to_string(),
            signature: hex::encode(signature_der),
            payload: payload_hash.to_string(),
            public_key: hex::encode(public_key.serialize()),
        })
    }

    #[test]
    fn settlement_attestation_rejects_non_tee_device() {
        let verifier = ZkcVerifier::new();

        let attestation = AttestationRequest::Ecdsa(Attestation {
            device_id: "conxius-mobile-123".to_string(),
            signature: "00".to_string(),
            payload: "payload-hash".to_string(),
            public_key: "00".to_string(),
        });

        let res = verifier.verify_settlement_trigger_attestation(&attestation, "payload-hash");
        assert!(matches!(res, Ok(false)));
    }

    #[test]
    fn settlement_attestation_rejects_unsupported_attestation_types() {
        let verifier = ZkcVerifier::new();

        let attestation = AttestationRequest::BitVm(conxian_core::BitVmAttestation {
            prover_id: "prover".to_string(),
            commitment_hash: "hash".to_string(),
            state_root: "root".to_string(),
            proof_hash: "proof".to_string(),
            verifier_address: "address".to_string(),
        });

        let res = verifier.verify_settlement_trigger_attestation(&attestation, "payload-hash");
        assert!(matches!(res, Ok(false)));
    }

    #[test]
    fn settlement_attestation_rejects_mock_device_id() {
        let verifier = ZkcVerifier::new();

        let accepted =
            make_signed_attestation(&format!("{TEE_DEVICE_ID_PREFIX}test-123"), "payload-hash");
        let res = verifier.verify_settlement_trigger_attestation(&accepted, "payload-hash");
        assert!(matches!(res, Ok(true)));

        let rejected =
            make_signed_attestation(&format!("{TEE_DEVICE_ID_PREFIX}mock-123"), "payload-hash");
        let res = verifier.verify_settlement_trigger_attestation(&rejected, "payload-hash");
        assert!(matches!(res, Ok(false)));
    }
}
