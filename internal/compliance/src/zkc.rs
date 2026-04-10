use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
pub use conxian_core::{
    Attestation, AttestationRequest, BitVmAttestation, ConxianError, ConxianJobCard, ConxianResult,
    IndustrialIntent, NormalizedSettlement, SchnorrAttestation, SettlementEnvelope,
    SettlementFinality, SettlementIdentifiers, SettlementRail, SettlementRailFamily,
    SettlementSource, SettlementStatus, ZkmlProof, SETTLEMENT_ENVELOPE_VERSION_CURRENT,
};
use hex::FromHex;
use hmac::{Hmac, Mac};
use quick_xml::events::Event;
use quick_xml::Reader;
use risc0_zkvm::Receipt;
use secp256k1::schnorr::Signature as SchnorrSignature;
use secp256k1::XOnlyPublicKey;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::warn;

type HmacSha256 = Hmac<Sha256>;

const INGRESS_SIGNATURE_HEX_LEN: usize = 64;

struct Iso20022Fields {
    source: SettlementSource,
    transaction_id: String,
    amount: String,
    currency: String,
    sender: String,
    receiver: String,
    settled_at: Option<u64>,
}

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
            .map_err(|e| ConxianError::Internal(e.to_string()))?;
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

        let digest = Sha256::digest(attestation.payload.as_bytes());
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
        let pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes).map_err(|_| {
            ConxianError::Security("Identity verification failed: invalid key data".into())
        })?;

        let sig_bytes = Vec::from_hex(&attestation.signature).map_err(|_| {
            ConxianError::Security(
                "Attestation verification failed: invalid signature format".into(),
            )
        })?;
        let signature = SchnorrSignature::from_slice(&sig_bytes).map_err(|_| {
            ConxianError::Security("Attestation verification failed: invalid signature data".into())
        })?;

        let digest = Sha256::digest(attestation.payload.as_bytes());
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

        let receipt_bytes = Self::decode_base64_or_hex("receipt", &proof.receipt)?;
        let receipt: Receipt = bincode::deserialize(&receipt_bytes).map_err(|e| {
            warn!(error = %e, "ZKML receipt deserialization failed");
            ConxianError::Compliance("Internal cryptographic verification error".to_string())
        })?;

        let image_id_bytes = Vec::from_hex(&proof.image_id)
            .map_err(|_| ConxianError::Compliance("Invalid proof image format".into()))?;
        if image_id_bytes.len() != 32 {
            return Err(ConxianError::Compliance(
                "Invalid proof image format: image_id must be 32 bytes".into(),
            ));
        }

        let mut image_id = [0u32; 8];
        for (i, chunk) in image_id_bytes.chunks_exact(4).enumerate() {
            image_id[i] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        receipt.verify(image_id).map_err(|e| {
            warn!(error = %e, "ZKML proof verification failed");
            ConxianError::Security("Cryptographic proof verification failed".to_string())
        })?;

        Ok(true)
    }

    pub fn verify_bitvm(&self, _attestation: &BitVmAttestation) -> ConxianResult<bool> {
        Ok(true)
    }

    pub fn verify_job_card_settlement(
        &self,
        _job_card: &ConxianJobCard,
        bitvm_proof: &BitVmAttestation,
    ) -> ConxianResult<bool> {
        self.verify_bitvm(bitvm_proof)
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
        hasher.update(identifiers.message_id.as_deref().unwrap_or("").as_bytes());
        hasher.update(identifiers.settlement_amount.as_bytes());
        hasher.update(identifiers.settlement_currency.as_bytes());
        hasher.update(identifiers.settlement_date.as_bytes());
        Ok(hex::encode(hasher.finalize()))
    }

    pub fn verify_settlement_trigger_attestation(
        &self,
        attestation: &AttestationRequest,
        payload_hash: &str,
    ) -> ConxianResult<bool> {
        match self.verify_attestation(attestation.clone()) {
            Ok(true) => {
                let signed_payload = match attestation {
                    AttestationRequest::Ecdsa(a) => &a.payload,
                    AttestationRequest::Schnorr(a) => &a.payload,
                    _ => {
                        return Err(ConxianError::Security(
                            "Unsupported attestation type for settlement trigger".into(),
                        ))
                    }
                };
                Ok(signed_payload == payload_hash)
            }
            res => res,
        }
    }

    fn decode_base64_or_hex(field: &str, value: &str) -> ConxianResult<Vec<u8>> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ConxianError::Compliance(format!("{field}: value is empty")));
        }

        if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
            let hex_body = &trimmed[2..];
            if hex_body.is_empty() {
                return Err(ConxianError::Compliance(format!(
                    "{field} hex format: cannot be empty after 0x"
                )));
            }
            Vec::from_hex(hex_body)
                .map_err(|e| ConxianError::Compliance(format!("{field}: invalid hex format: {e}")))
        } else if trimmed.chars().all(|c| c.is_ascii_hexdigit()) && trimmed.len().is_multiple_of(2)
        {
            Err(ConxianError::Compliance(format!(
                "{field}: ambiguous hex-like string must be prefixed with 0x"
            )))
        } else {
            BASE64_STANDARD
                .decode(trimmed)
                .map_err(|e| ConxianError::Compliance(format!("{field}: invalid base64: {e}")))
        }
    }

    pub fn normalize_iso20022_ingress(
        &self,
        xml: &str,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        let fields = self.parse_iso20022_pacs008(xml)?;
        let identifiers = SettlementIdentifiers {
            message_id: Some(fields.transaction_id.clone()),
            settlement_amount: fields.amount.clone(),
            settlement_currency: fields.currency.clone(),
            ..Default::default()
        };

        let amount_f: f64 = fields.amount.parse().map_err(|_| {
            ConxianError::Compliance(format!(
                "Invalid settlement amount format: {}",
                fields.amount
            ))
        })?;

        let (minor, scale) = self.to_minor_units(amount_f);

        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: fields.source,
                rail: Some(SettlementRail {
                    family: SettlementRailFamily::Rtgs,
                    name: "Bitcoin L1".into(),
                    region: "Global".into(),
                }),
                transaction_id: fields.transaction_id,
                amount_minor: minor,
                amount_scale: scale,
                currency: fields.currency,
                sender: fields.sender,
                receiver: fields.receiver,
                timestamp: fields.settled_at.unwrap_or(0),
                settled_at: fields.settled_at,
                status: SettlementStatus::Ingested,
                finality: SettlementFinality::Provisional,
                raw_payload_hash,
                identifiers,
                industrial_intent: IndustrialIntent::default(),
            },
        })
    }

    pub fn normalize_papss_ingress(
        &self,
        payload: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        let tx_id = payload["transaction_id"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required settlement identifier".to_string())
        })?;
        let amount_str = payload["amount"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required settlement amount".to_string())
        })?;
        let currency = payload["currency"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required settlement currency".to_string())
        })?;
        let sender = payload["sender_bic"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required sender identity".to_string())
        })?;
        let receiver = payload["receiver_bic"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required receiver identity".to_string())
        })?;

        let amount_f: f64 = amount_str.parse().map_err(|_| {
            ConxianError::Compliance("Invalid settlement amount format".to_string())
        })?;

        let (minor, scale) = self.to_minor_units(amount_f);
        let identifiers = SettlementIdentifiers {
            message_id: Some(tx_id.to_string()),
            settlement_amount: amount_str.to_string(),
            settlement_currency: currency.to_string(),
            ..Default::default()
        };

        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Papss,
                rail: Some(SettlementRail {
                    family: SettlementRailFamily::Rtgs,
                    name: "Bitcoin L1".into(),
                    region: "Global".into(),
                }),
                transaction_id: tx_id.to_string(),
                amount_minor: minor,
                amount_scale: scale,
                currency: currency.to_string(),
                sender: sender.to_string(),
                receiver: receiver.to_string(),
                timestamp: 0,
                settled_at: None,
                status: SettlementStatus::Ingested,
                finality: SettlementFinality::Provisional,
                raw_payload_hash,
                identifiers,
                industrial_intent: IndustrialIntent::default(),
            },
        })
    }

    pub fn normalize_brics_ingress(
        &self,
        payload: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        let tx_id = payload["brics_tx_id"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required settlement identifier".to_string())
        })?;
        let amount_str = payload["amount"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required settlement amount".to_string())
        })?;
        let currency = payload["currency"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required settlement currency".to_string())
        })?;
        let sender = payload["origin_bank"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required sender identity".to_string())
        })?;
        let receiver = payload["target_bank"].as_str().ok_or_else(|| {
            ConxianError::Compliance("Missing required receiver identity".to_string())
        })?;

        let amount_f: f64 = amount_str.parse().map_err(|_| {
            ConxianError::Compliance("Invalid settlement amount format".to_string())
        })?;

        let (minor, scale) = self.to_minor_units(amount_f);
        let identifiers = SettlementIdentifiers {
            message_id: Some(tx_id.to_string()),
            settlement_amount: amount_str.to_string(),
            settlement_currency: currency.to_string(),
            ..Default::default()
        };

        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: SettlementSource::Brics,
                rail: Some(SettlementRail {
                    family: SettlementRailFamily::Rtgs,
                    name: "Bitcoin L1".into(),
                    region: "Global".into(),
                }),
                transaction_id: tx_id.to_string(),
                amount_minor: minor,
                amount_scale: scale,
                currency: currency.to_string(),
                sender: sender.to_string(),
                receiver: receiver.to_string(),
                timestamp: 0,
                settled_at: None,
                status: SettlementStatus::Ingested,
                finality: SettlementFinality::Provisional,
                raw_payload_hash,
                identifiers,
                industrial_intent: IndustrialIntent::default(),
            },
        })
    }

    fn to_minor_units(&self, amount: f64) -> (u64, u32) {
        let scale = 2;
        let minor = (amount * 100.0).round() as u64;
        (minor, scale)
    }

    fn parse_iso20022_pacs008(&self, xml: &str) -> ConxianResult<Iso20022Fields> {
        let mut reader = Reader::from_str(xml);

        let mut msg_id = String::new();
        let mut amount = String::new();
        let mut currency = String::new();
        let mut dbtr_nm = String::new();
        let mut cdtr_nm = String::new();
        let mut dbtr_iban = String::new();
        let mut cdtr_iban = String::new();

        let mut buf = Vec::new();
        let mut current_tag = String::new();
        let mut in_dbtr = false;
        let mut in_cdtr = false;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    current_tag = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                    match current_tag.as_str() {
                        "Dbtr" | "DbtrAcct" => in_dbtr = true,
                        "Cdtr" | "CdtrAcct" => in_cdtr = true,
                        "IntrBkSttlmAmt" => {
                            for attr in e.attributes() {
                                let attr =
                                    attr.map_err(|e| ConxianError::Compliance(e.to_string()))?;
                                if attr.key.local_name().as_ref() == b"Ccy" {
                                    currency = String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        }
                        _ => (),
                    }
                }
                Ok(Event::Text(e)) => {
                    let text = String::from_utf8_lossy(e.as_ref()).trim().to_string();
                    match current_tag.as_str() {
                        "MsgId" => msg_id = text,
                        "IntrBkSttlmAmt" => amount = text,
                        "Nm" if in_dbtr => dbtr_nm = text,
                        "Nm" if in_cdtr => cdtr_nm = text,
                        "IBAN" if in_dbtr => dbtr_iban = text,
                        "IBAN" if in_cdtr => cdtr_iban = text,
                        _ => (),
                    }
                }
                Ok(Event::End(e)) => {
                    let tag = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                    match tag.as_str() {
                        "Dbtr" | "DbtrAcct" => in_dbtr = false,
                        "Cdtr" | "CdtrAcct" => in_cdtr = false,
                        _ => (),
                    }
                    current_tag.clear();
                }
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

    pub fn format_iso20022_pacs008_v8(&self, job_card: &ConxianJobCard) -> ConxianResult<String> {
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
            <DbtrAcct>
                <Id>
                    <Othr>
                        <Id>{}</Id>
                    </Othr>
                </Id>
            </DbtrAcct>
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
