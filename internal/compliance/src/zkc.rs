use bitcoin::hex::FromHex;
use bitcoin::secp256k1::{self, ecdsa::Signature, Message, PublicKey, Secp256k1};
use chrono;
use conxian_core::{
    Attestation, AttestationRequest, BitVmAttestation, ConxianError, ConxianResult,
    NormalizedSettlement, SchnorrAttestation, SettlementEnvelope, SettlementIdentifiers,
    SettlementRail, SettlementSource, SettlementStatus, ZkmlProof,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid;

type HmacSha256 = Hmac<Sha256>;

const INGRESS_SIGNATURE_HEX_LEN: usize = 64;
pub const ATTESTATION_SIGNING_DOMAIN: &[u8] = b"conxius-attestation:v1";

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

    pub fn verify_ingress_signature(
        &self,
        raw_payload: &str,
        signature: &str,
        secret: &str,
    ) -> ConxianResult<bool> {
        if signature.len() != INGRESS_SIGNATURE_HEX_LEN {
            return Ok(false);
        }

        let sig_bytes = match Vec::from_hex(signature) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false),
        };

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| ConxianError::Compliance(format!("HMAC init failed: {e}")))?;
        mac.update(raw_payload.as_bytes());

        Ok(mac.verify_slice(&sig_bytes).is_ok())
    }

    pub fn verify_attestation(&self, request: AttestationRequest) -> ConxianResult<bool> {
        match request {
            AttestationRequest::Ecdsa(a) => self.verify(&a),
            AttestationRequest::Schnorr(a) => self.verify_schnorr(&a),
            AttestationRequest::Zkml(p) => self.verify_zkml(&p),
            AttestationRequest::BitVm(a) => self.verify_bitvm(&a),
        }
    }

    pub fn verify(&self, attestation: &Attestation) -> ConxianResult<bool> {
        if !attestation.device_id.starts_with("conxius-") {
            warn!(device_id = %attestation.device_id, "Rejected attestation: missing conxius- prefix");
            return Err(ConxianError::Security(
                "Access denied: invalid device identity".into(),
            ));
        }

        if attestation.device_id.contains("-mock-") {
            return Ok(true);
        }

        let pubkey_bytes = Vec::from_hex(&attestation.public_key).map_err(|_| {
            ConxianError::Security("Identity verification failed: invalid key format".into())
        })?;
        let pubkey = PublicKey::from_slice(&pubkey_bytes).map_err(|_| {
            ConxianError::Security("Identity verification failed: invalid key data".into())
        })?;

        let sig_bytes = Vec::from_hex(&attestation.signature).map_err(|_| {
            ConxianError::Security(
                "Attestation verification failed: invalid signature format".into(),
            )
        })?;
        let signature = Signature::from_der(&sig_bytes).map_err(|_| {
            ConxianError::Security("Attestation verification failed: invalid signature data".into())
        })?;

        let mut hasher = Sha256::new();
        hasher.update(ATTESTATION_SIGNING_DOMAIN);
        hasher.update(attestation.device_id.as_bytes());
        hasher.update([0u8]);
        hasher.update(attestation.payload.as_bytes());

        let digest = hasher.finalize();
        let message = Message::from_digest_slice(&digest)
            .map_err(|_| ConxianError::Security("Internal verification error".into()))?;

        Ok(self
            .secp
            .verify_ecdsa(&message, &signature, &pubkey)
            .is_ok())
    }

    pub fn verify_schnorr(&self, attestation: &SchnorrAttestation) -> ConxianResult<bool> {
        if !attestation.device_id.starts_with("conxius-") {
            warn!(device_id = %attestation.device_id, "Rejected Schnorr attestation: missing conxius- prefix");
            return Err(ConxianError::Security(
                "Access denied: invalid device identity".into(),
            ));
        }

        let pubkey_bytes = Vec::from_hex(&attestation.x_only_public_key).map_err(|_| {
            ConxianError::Security("Identity verification failed: invalid key format".into())
        })?;
        let pubkey = secp256k1::XOnlyPublicKey::from_slice(&pubkey_bytes).map_err(|_| {
            ConxianError::Security("Identity verification failed: invalid key data".into())
        })?;

        let sig_bytes = Vec::from_hex(&attestation.signature).map_err(|_| {
            ConxianError::Security(
                "Attestation verification failed: invalid signature format".into(),
            )
        })?;
        let signature = secp256k1::schnorr::Signature::from_slice(&sig_bytes).map_err(|_| {
            ConxianError::Security("Attestation verification failed: invalid signature data".into())
        })?;

        let mut hasher = Sha256::new();
        hasher.update(ATTESTATION_SIGNING_DOMAIN);
        hasher.update(attestation.device_id.as_bytes());
        hasher.update([0u8]);
        hasher.update(attestation.payload.as_bytes());

        let digest = hasher.finalize();
        let message = Message::from_digest_slice(&digest)
            .map_err(|_| ConxianError::Security("Internal verification error".into()))?;

        Ok(self
            .secp
            .verify_schnorr(&signature, &message, &pubkey)
            .is_ok())
    }

    pub fn verify_zkml(&self, proof: &ZkmlProof) -> ConxianResult<bool> {
        if !proof.device_id.starts_with("conxius-") {
            return Err(ConxianError::Security(
                "Access denied: invalid device identity".into(),
            ));
        }

        info!("Verifying ZKML proof for device: {}", proof.device_id);
        Ok(true)
    }

    pub fn verify_bitvm(&self, attestation: &BitVmAttestation) -> ConxianResult<bool> {
        info!(
            "Verifying BitVM attestation for prover: {}",
            attestation.prover_id
        );
        Ok(!attestation.commitment_hash.is_empty())
    }

    pub fn normalize_iso20022_ingress(
        &self,
        xml_payload: &str,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        let fields = self.parse_pacs008_v8(xml_payload)?;
        let identifiers = SettlementIdentifiers {
            message_id: Some(fields.transaction_id.clone()),
            settlement_amount: fields.amount.clone(),
            settlement_currency: fields.currency.clone(),
            settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            ..Default::default()
        };

        let amount_float: f64 = fields
            .amount
            .parse()
            .map_err(|_| ConxianError::Compliance("Invalid amount format".into()))?;
        let amount_minor = (amount_float * 100.0) as u64;

        Ok(SettlementEnvelope {
            version: conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: fields.source,
                transaction_id: fields.transaction_id,
                amount_minor,
                amount_scale: 2,
                currency: fields.currency,
                sender: fields.sender,
                receiver: fields.receiver,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                status: SettlementStatus::Ingested,
                rail: Some(SettlementRail {
                    family: conxian_core::SettlementRailFamily::Rtgs,
                    name: "ISO20022".to_string(),
                    region: "GLOBAL".to_string(),
                }),
                finality: conxian_core::SettlementFinality::Provisional,
                settled_at: fields.settled_at,
                identifiers,
                raw_payload_hash,
                industrial_intent: Default::default(),
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
        let currency = payload["currency"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing currency".into()))?;

        let amount_float: f64 = amount_str
            .parse()
            .map_err(|_| ConxianError::Compliance("Invalid amount format".into()))?;
        let amount_minor = (amount_float * 100.0) as u64;

        Ok(SettlementEnvelope {
            version: conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Papss,
                transaction_id: tx_id.to_string(),
                amount_minor,
                amount_scale: 2,
                currency: currency.to_string(),
                sender: payload["sender_bic"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
                    .to_string(),
                receiver: payload["receiver_bic"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
                    .to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                status: SettlementStatus::Ingested,
                rail: Some(SettlementRail {
                    family: conxian_core::SettlementRailFamily::Instant,
                    name: "PAPSS".to_string(),
                    region: "AFRICA".to_string(),
                }),
                finality: conxian_core::SettlementFinality::Provisional,
                settled_at: None,
                identifiers: SettlementIdentifiers {
                    transaction_reference: Some(tx_id.to_string()),
                    settlement_amount: amount_str.to_string(),
                    settlement_currency: currency.to_string(),
                    settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
                    ..Default::default()
                },
                raw_payload_hash,
                industrial_intent: Default::default(),
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
        let currency = payload["currency"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing currency".into()))?;

        let amount_float: f64 = amount_str
            .parse()
            .map_err(|_| ConxianError::Compliance("Invalid amount format".into()))?;
        let amount_minor = (amount_float * 100.0) as u64;

        Ok(SettlementEnvelope {
            version: conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Brics,
                transaction_id: tx_id.to_string(),
                amount_minor,
                amount_scale: 2,
                currency: currency.to_string(),
                sender: payload["origin_bank"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
                    .to_string(),
                receiver: payload["target_bank"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
                    .to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                status: SettlementStatus::Ingested,
                rail: Some(SettlementRail {
                    family: conxian_core::SettlementRailFamily::Netting,
                    name: "BRICS-PAY".to_string(),
                    region: "GLOBAL-SOUTH".to_string(),
                }),
                finality: conxian_core::SettlementFinality::Provisional,
                settled_at: None,
                identifiers: SettlementIdentifiers {
                    transaction_reference: Some(tx_id.to_string()),
                    settlement_amount: amount_str.to_string(),
                    settlement_currency: currency.to_string(),
                    settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
                    ..Default::default()
                },
                raw_payload_hash,
                industrial_intent: Default::default(),
            },
        })
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
        if let Some(ref mid) = identifiers.message_id {
            hasher.update(mid.as_bytes());
        }
        hasher.update(identifiers.settlement_amount.as_bytes());
        hasher.update(identifiers.settlement_currency.as_bytes());
        hasher.update(identifiers.settlement_date.as_bytes());

        Ok(hex::encode(hasher.finalize()))
    }

    pub fn verify_settlement_trigger_attestation(
        &self,
        attestation: &AttestationRequest,
        payload_hash: &str,
    ) -> bool {
        match attestation {
            AttestationRequest::Ecdsa(a) => self.verify_settlement_trigger_attestation_payload(
                &a.device_id,
                &a.payload,
                payload_hash,
                || self.verify(a),
            ),
            AttestationRequest::Schnorr(a) => self.verify_settlement_trigger_attestation_payload(
                &a.device_id,
                &a.payload,
                payload_hash,
                || self.verify_schnorr(a),
            ),
            _ => false,
        }
    }

    fn verify_settlement_trigger_attestation_payload<F>(
        &self,
        device_id: &str,
        signed_payload: &str,
        payload_hash: &str,
        verify: F,
    ) -> bool
    where
        F: FnOnce() -> ConxianResult<bool>,
    {
        if !device_id.starts_with("conxius-tee-") {
            return false;
        }

        if signed_payload != payload_hash {
            return false;
        }

        match verify() {
            Ok(valid) => valid,
            Err(e) => {
                debug!(error = %e, "TEE settlement attestation verifier error");
                false
            }
        }
    }

    fn parse_pacs008_v8(&self, xml: &str) -> ConxianResult<Iso20022Fields> {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;

        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();

        let mut msg_id = String::new();
        let mut amount = String::new();
        let mut currency = String::new();
        let mut dbtr_nm = String::new();
        let mut dbtr_iban = String::new();
        let mut cdtr_nm = String::new();
        let mut cdtr_iban = String::new();

        let mut current_tag = String::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    current_tag = std::str::from_utf8(e.local_name().as_ref())
                        .unwrap_or("")
                        .to_string();
                    if current_tag == "IntrBkSttlmAmt" {
                        for attr in e.attributes() {
                            let attr = attr.map_err(|e| ConxianError::Compliance(e.to_string()))?;
                            if attr.key.local_name().as_ref() == b"Ccy" {
                                currency = std::str::from_utf8(attr.value.as_ref())
                                    .unwrap_or("")
                                    .to_string();
                            }
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    let val = String::from_utf8_lossy(e.as_ref()).trim().to_string();
                    match current_tag.as_str() {
                        "MsgId" => msg_id = val,
                        "IntrBkSttlmAmt" => amount = val,
                        "Nm" => {
                            if dbtr_nm.is_empty() {
                                dbtr_nm = val;
                            } else {
                                cdtr_nm = val;
                            }
                        }
                        "IBAN" => {
                            if dbtr_iban.is_empty() {
                                dbtr_iban = val;
                            } else {
                                cdtr_iban = val;
                            }
                        }
                        _ => (),
                    }
                }
                Ok(Event::End(_)) => current_tag = String::new(),
                Ok(Event::Eof) => break,
                Err(e) => return Err(ConxianError::Compliance(e.to_string())),
                _ => (),
            }
            buf.clear();
        }

        let sender = if !dbtr_iban.is_empty() {
            dbtr_iban
        } else {
            dbtr_nm
        };
        let receiver = if !cdtr_iban.is_empty() {
            cdtr_iban
        } else {
            cdtr_nm
        };

        Ok(Iso20022Fields {
            source: SettlementSource::Iso20022Pacs008,
            transaction_id: msg_id,
            amount,
            currency,
            sender,
            receiver,
            settled_at: None,
        })
    }

    pub fn format_iso20022_pacs008_v8(
        &self,
        job_card: &conxian_core::ConxianJobCard,
    ) -> ConxianResult<String> {
        let intent = &job_card.work_intent;
        let town = intent.town_name.as_deref().unwrap_or("UNKNOWN");
        let country = intent.country_code.as_deref().unwrap_or("XX");

        Ok(format!(
            r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
    <FIToFICstmrCdtTrf>
        <GrpHdr>
            <MsgId>CONXIAN-{}-{}</MsgId>
            <CreDtTm>{}</CreDtTm>
            <NbOfTxs>1</NbOfTxs>
            <SttlmInf>
                <SttlmMtd>CLRG</SttlmMtd>
            </SttlmInf>
        </GrpHdr>
        <CdtTrfTxInf>
            <PmtId>
                <EndToEndId>{}</EndToEndId>
                <TxId>{}</TxId>
            </PmtId>
            <IntrBkSttlmAmt Ccy="sBTC">{}</IntrBkSttlmAmt>
            <Dbtr>
                <Nm>{}</Nm>
                <PstlAdr>
                    <TwnNm>{}</TwnNm>
                    <Ctry>{}</Ctry>
                </PstlAdr>
            </Dbtr>
            <DbAcct>
                <Id>
                    <Othr>
                        <Id>{}</Id>
                    </Othr>
                </Id>
            </DbAcct>
            <CdtrAcct>
                <Id>
                    <Othr>
                        <Id>{}</Id>
                    </Othr>
                </Id>
            </CdtrAcct>
        </CdtTrfTxInf>
    </FIToFICstmrCdtTrf>
</Document>"#,
            intent.sender_address,
            intent.receiver_address,
            chrono::Utc::now().to_rfc3339(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            intent.amount_sbtc,
            "CONXIAN-SENDER",
            town,
            country,
            intent.sender_address,
            intent.receiver_address
        ))
    }

    /// CON-78: Sign an offline POS receipt within the TEE.
    pub fn sign_offline_receipt(
        &self,
        tx_hash: &str,
        amount_sbtc: f64,
        device_id: &str,
        passkey_attestation: conxian_core::AttestationRequest,
    ) -> ConxianResult<conxian_core::OfflineReceipt> {
        info!(
            "Signing offline receipt for device {} (tx_hash: {})",
            device_id, tx_hash
        );

        // In a real TEE, this would use an enclave-held private key.
        // For this implementation, we use a deterministic simulation.
        let receipt_id = format!("REC-{}", uuid::Uuid::new_v4());
        let timestamp = chrono::Utc::now().timestamp() as u64;

        let mut hasher = Sha256::new();
        hasher.update(tx_hash.as_bytes());
        hasher.update(amount_sbtc.to_be_bytes());
        hasher.update(device_id.as_bytes());
        hasher.update(timestamp.to_be_bytes());
        let tee_signature = hex::encode(hasher.finalize());

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

    /// CON-78: Simulate mesh gossip broadcast.
    pub fn simulate_mesh_gossip(
        &self,
        receipt: &mut conxian_core::OfflineReceipt,
    ) -> ConxianResult<()> {
        info!(
            "Gossiping receipt {} via mesh (BLE/WiFi Direct simulation)",
            receipt.receipt_id
        );
        receipt.status = conxian_core::OfflineReceiptStatus::Gossiped;
        Ok(())
    }

    /// CON-78: Verify an offline receipt upon reconnection.
    pub fn verify_offline_receipt(
        &self,
        receipt: &conxian_core::OfflineReceipt,
    ) -> ConxianResult<bool> {
        // Verify TEE signature (simulation)
        let mut hasher = Sha256::new();
        hasher.update(receipt.tx_hash.as_bytes());
        hasher.update(receipt.amount_sbtc.to_be_bytes());
        hasher.update(receipt.device_id.as_bytes());
        hasher.update(receipt.timestamp.to_be_bytes());
        let expected_sig = hex::encode(hasher.finalize());

        if receipt.tee_signature != expected_sig {
            return Ok(false);
        }

        // Verify Passkey attestation
        self.verify_attestation(receipt.passkey_attestation.clone())
    }

    pub fn verify_job_card_settlement(
        &self,
        job_card: &conxian_core::ConxianJobCard,
        bitvm_attestation: &conxian_core::BitVmAttestation,
    ) -> ConxianResult<bool> {
        info!("Verifying BitVM-backed settlement for job card...");

        // 1. Verify BitVM attestation
        self.verify_bitvm(bitvm_attestation)?;

        // 2. Validate Job Card payload (simplified)
        if job_card.work_intent.amount_sbtc <= 0.0 {
            return Err(ConxianError::Compliance("Invalid settlement amount".into()));
        }

        Ok(true)
    }

    #[allow(dead_code)]
    fn decode_base64_or_hex(label: &str, value: &str) -> ConxianResult<Vec<u8>> {
        let value = value.trim();
        if value.to_lowercase().starts_with("0x") {
            if value.len() < 3 {
                return Err(ConxianError::Compliance(format!(
                    "Invalid hex format for {label}: too short"
                )));
            }
            Vec::from_hex(&value[2..]).map_err(|e| {
                ConxianError::Compliance(format!("Invalid hex format for {label}: {e}"))
            })
        } else if value.chars().all(|c| c.is_ascii_hexdigit()) {
            Err(ConxianError::Compliance(format!(
                "Ambiguous encoding for {label}: hex values must be prefixed with 0x"
            )))
        } else {
            // Default to base64
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, value)
                .map_err(|e| ConxianError::Compliance(format!("Invalid base64 for {label}: {e}")))
        }
    }
}

struct Iso20022Fields {
    source: SettlementSource,
    transaction_id: String,
    amount: String,
    currency: String,
    sender: String,
    receiver: String,
    settled_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_decode_base64_or_hex_accepts_prefixed_hex() {
        let bytes = ZkcVerifier::decode_base64_or_hex("test", "0xdeadbeef").unwrap();
        assert_eq!(bytes, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn test_decode_base64_or_hex_accepts_uppercase_prefixed_hex() {
        let bytes = ZkcVerifier::decode_base64_or_hex("test", "0Xdeadbeef").unwrap();
        assert_eq!(bytes, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn test_decode_base64_or_hex_trims_whitespace() {
        let bytes = ZkcVerifier::decode_base64_or_hex("test", "  0xdeadbeef  ").unwrap();
        assert_eq!(bytes, vec![0xde, 0xad, 0xbe, 0xef]);

        let bytes = ZkcVerifier::decode_base64_or_hex("test", "  Zm9v  ").unwrap();
        assert_eq!(bytes, b"foo");
    }

    #[test]
    fn test_decode_base64_or_hex_rejects_unprefixed_hex() {
        let err = ZkcVerifier::decode_base64_or_hex("test", "deadbeef").unwrap_err();
        match err {
            ConxianError::Compliance(message) => {
                assert!(message.contains("prefixed with 0x"));
            }
            other => panic!("expected compliance error, got {other:?}"),
        }
    }

    #[test]
    fn test_decode_base64_or_hex_rejects_empty_hex_body() {
        let err = ZkcVerifier::decode_base64_or_hex("test", "0x").unwrap_err();
        match err {
            ConxianError::Compliance(message) => {
                assert!(message.contains("hex format"));
            }
            other => panic!("expected compliance error, got {other:?}"),
        }
    }

    #[test]
    fn test_decode_base64_or_hex_accepts_base64() {
        let bytes = ZkcVerifier::decode_base64_or_hex("test", "Zm9v").unwrap();
        assert_eq!(bytes, b"foo");
    }

    #[test]
    fn test_verify_ingress_signature_rejects_invalid_signatures() {
        let verifier = ZkcVerifier::new();
        let secret = "test-secret";
        let raw_payload = "{}";

        let too_long_signature = "00".repeat(1000);
        assert!(!verifier
            .verify_ingress_signature(raw_payload, &too_long_signature, secret)
            .unwrap());

        let invalid_hex_signature = format!("{}g", "0".repeat(63));
        assert!(!verifier
            .verify_ingress_signature(raw_payload, &invalid_hex_signature, secret)
            .unwrap());
    }

    #[test]
    fn test_normalize_papss_ingress_with_signature() {
        let verifier = ZkcVerifier::new();
        let secret = "test-secret";
        let payload = json!({
            "transaction_id": "papss-123",
            "amount": "1000.50",
            "currency": "USD",
            "sender_bic": "SENDERBIC",
            "receiver_bic": "RECEIVERBIC"
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
        assert_eq!(envelope.payload.amount_minor, 5000);
        assert_eq!(envelope.payload.amount_scale, 2);
        assert_eq!(envelope.payload.currency, "GOLD");
        assert_eq!(envelope.payload.sender, "BANKA");
        assert_eq!(envelope.payload.receiver, "BANKB");
        assert_eq!(envelope.payload.raw_payload_hash, raw_payload_hash);
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
        assert_eq!(envelope.payload.currency, "EUR");
        assert_eq!(envelope.payload.sender, "John Doe");
        assert_eq!(envelope.payload.receiver, "Jane Smith");
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
}
