use bitcoin::hex::FromHex;
use bitcoin::secp256k1::{self, ecdsa::Signature, Message, PublicKey, Secp256k1};
use borsh::BorshDeserialize;
use chrono;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use conxian_core::{
    Attestation, AttestationRequest, BitVmAttestation, ConxianError, ConxianResult,
    NormalizedSettlement, SchnorrAttestation, SettlementEnvelope, SettlementIdentifiers,
    SettlementRail, SettlementSource, SettlementStatus, ZkmlProof,
};
use hmac::{Hmac, Mac};
use risc0_zkvm::Receipt;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};
use uuid;

type HmacSha256 = Hmac<Sha256>;

const INGRESS_SIGNATURE_HEX_LEN: usize = 64;
const MAX_ZKML_FIELD_LEN: usize = 4 * 1024 * 1024;
const MAX_INLINE_PUBLIC_INPUTS: usize = 8 * 1024;

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

    pub fn verify_attestation(&self, request: &AttestationRequest) -> ConxianResult<bool> {
        match request {
            AttestationRequest::Ecdsa(a) => self.verify(a),
            AttestationRequest::Schnorr(a) => self.verify_schnorr(a),
            AttestationRequest::Zkml(p) => self.verify_zkml(p),
            AttestationRequest::BitVm(a) => self.verify_bitvm(a),
        }
    }

    pub fn verify(&self, attestation: &Attestation) -> ConxianResult<bool> {
        if !attestation.device_id.starts_with("conxius-") {
            warn!(device_id = %attestation.device_id, "Rejected attestation: missing conxius- prefix");
            return Err(ConxianError::Security(
                "Access denied: invalid device identity".into(),
            ));
        }

        #[cfg(feature = "mock-integrations")]
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

        info!("Verifying ZKML proof for device: {}", proof.device_id);

        let receipt_hash = proof.receipt_hash.trim();
        let receipt_hash = receipt_hash
            .strip_prefix("0x")
            .or_else(|| receipt_hash.strip_prefix("0X"))
            .unwrap_or(receipt_hash);
        if !Self::is_32_byte_hex(receipt_hash) {
            return Err(ConxianError::Compliance(
                "Invalid receipt_hash: expected 32-byte hex string".to_string(),
            ));
        }

        let receipt_str = proof.receipt.trim();
        if receipt_str.is_empty() {
            return Err(ConxianError::Compliance(
                "ZKML verification failed: receipt cannot be empty".to_string(),
            ));
        }
        if receipt_str.len() > MAX_ZKML_FIELD_LEN {
            return Err(ConxianError::Compliance(
                "Invalid receipt: payload too large".to_string(),
            ));
        }

        let public_inputs_str = proof.public_inputs.trim();
        if public_inputs_str.is_empty() {
            return Err(ConxianError::Compliance(
                "ZKML verification failed: public_inputs cannot be empty".to_string(),
            ));
        }
        if public_inputs_str.len() > MAX_ZKML_FIELD_LEN {
            return Err(ConxianError::Compliance(
                "Invalid public_inputs: payload too large".to_string(),
            ));
        }

        let journal_str = proof.journal.trim();
        if journal_str.is_empty() {
            return Err(ConxianError::Compliance(
                "ZKML verification failed: journal cannot be empty".to_string(),
            ));
        }
        if journal_str.len() > MAX_ZKML_FIELD_LEN {
            return Err(ConxianError::Compliance(
                "Invalid journal: payload too large".to_string(),
            ));
        }

        let image_id_hex = proof.image_id.trim();
        let image_id_hex = image_id_hex
            .strip_prefix("0x")
            .or_else(|| image_id_hex.strip_prefix("0X"))
            .unwrap_or(image_id_hex);
        if !Self::is_32_byte_hex(image_id_hex) {
            return Err(ConxianError::Compliance(
                "Invalid image_id: expected 32-byte hex string".to_string(),
            ));
        }

        let receipt_bytes = Self::decode_base64_or_hex("receipt", receipt_str)?;
        let computed_receipt_hash = hex::encode(Sha256::digest(&receipt_bytes));
        if !computed_receipt_hash.eq_ignore_ascii_case(receipt_hash) {
            return Err(ConxianError::Security(
                "ZKML verification failed: receipt_hash mismatch".to_string(),
            ));
        }

        let receipt = Self::decode_risc0_receipt(&receipt_bytes)?;

        let image_id = Self::parse_zkml_image_id(image_id_hex)?;

        receipt.verify(image_id).map_err(|e| {
            warn!(error = %e, "ZKML proof verification failed");
            ConxianError::Security("Cryptographic proof verification failed".to_string())
        })?;

        let receipt_journal = receipt.journal.bytes.as_slice();

        if receipt_journal != journal_str.as_bytes() {
            let decoded_journal_bytes = Self::decode_base64_or_hex("journal", journal_str)?;
            if receipt_journal != decoded_journal_bytes.as_slice() {
                return Err(ConxianError::Security(
                    "ZKML verification failed: journal mismatch".to_string(),
                ));
            }
        }

        let public_inputs_raw = public_inputs_str.as_bytes();
        let public_inputs_raw_hash_hex = hex::encode(Sha256::digest(public_inputs_raw));
        let public_inputs_raw_hash_hex_upper = public_inputs_raw_hash_hex.to_ascii_uppercase();

        let mut public_inputs_ok =
            Self::contains_subslice(receipt_journal, public_inputs_raw_hash_hex.as_bytes())
                || Self::contains_subslice(
                    receipt_journal,
                    public_inputs_raw_hash_hex_upper.as_bytes(),
                );

        if !public_inputs_ok && public_inputs_raw.len() <= MAX_INLINE_PUBLIC_INPUTS {
            public_inputs_ok = Self::contains_subslice(receipt_journal, public_inputs_raw);
        }

        if !public_inputs_ok {
            let public_inputs_decoded =
                Self::decode_base64_or_hex("public_inputs", public_inputs_str).ok();

            if let Some(public_inputs_decoded) = public_inputs_decoded {
                let public_inputs_decoded_hash_hex =
                    hex::encode(Sha256::digest(&public_inputs_decoded));
                let public_inputs_decoded_hash_hex_upper =
                    public_inputs_decoded_hash_hex.to_ascii_uppercase();

                public_inputs_ok = Self::contains_subslice(
                    receipt_journal,
                    public_inputs_decoded_hash_hex.as_bytes(),
                ) || Self::contains_subslice(
                    receipt_journal,
                    public_inputs_decoded_hash_hex_upper.as_bytes(),
                );

                if !public_inputs_ok && public_inputs_decoded.len() <= MAX_INLINE_PUBLIC_INPUTS {
                    public_inputs_ok =
                        Self::contains_subslice(receipt_journal, public_inputs_decoded.as_slice());
                }
            }
        }

        if !public_inputs_ok {
            return Err(ConxianError::Security(
                "ZKML verification failed: journal missing public input commitment".to_string(),
            ));
        }
        Ok(true)
    }

    pub fn verify_bitvm(&self, attestation: &BitVmAttestation) -> ConxianResult<bool> {
        info!(
            "Verifying BitVM attestation for prover: {}",
            attestation.prover_id
        );

        let commitment_hash = attestation.commitment_hash.trim();
        if commitment_hash.is_empty() {
            return Err(ConxianError::Security("BitVM proof missing fields".into()));
        }

        let state_root = attestation.state_root.trim();
        if state_root.is_empty() {
            return Err(ConxianError::Security("BitVM proof missing fields".into()));
        }

        let commitment_hash = commitment_hash
            .strip_prefix("0x")
            .or_else(|| commitment_hash.strip_prefix("0X"))
            .unwrap_or(commitment_hash);

        let commitment_bytes = Vec::from_hex(commitment_hash)
            .map_err(|_| ConxianError::Security("BitVM commitment must be hex".into()))?;
        let commitment_bytes: [u8; 32] = commitment_bytes
            .try_into()
            .map_err(|_| ConxianError::Security("BitVM commitment must be 32 bytes".into()))?;

        let expected: [u8; 32] = Sha256::digest(state_root.as_bytes()).into();
        if commitment_bytes != expected {
            return Err(ConxianError::Security("BitVM commitment mismatch".into()));
        }

        Ok(true)
    }

    fn parse_amount_minor_scale(amount: &str) -> ConxianResult<(u64, u32)> {
        const MAX_SCALE: usize = 18;
        const MAX_LEN: usize = 128;

        if amount.is_empty() {
            return Err(ConxianError::Compliance("Invalid amount".to_string()));
        }
        if amount.len() > MAX_LEN {
            return Err(ConxianError::Compliance(
                "Invalid amount: too long".to_string(),
            ));
        }
        if amount != amount.trim() {
            return Err(ConxianError::Compliance(
                "Invalid amount: must not contain leading/trailing whitespace".to_string(),
            ));
        }
        if amount.starts_with('-') {
            return Err(ConxianError::Compliance(
                "Invalid amount: must be non-negative".to_string(),
            ));
        }
        if amount.starts_with('+') {
            return Err(ConxianError::Compliance(
                "Invalid amount: must not include sign".to_string(),
            ));
        }

        let (int_part_raw, frac_part_raw) = amount.split_once('.').unwrap_or((amount, ""));
        if int_part_raw.is_empty() && frac_part_raw.is_empty() {
            return Err(ConxianError::Compliance(
                "Invalid amount: must contain at least one digit".to_string(),
            ));
        }
        if frac_part_raw.contains('.') {
            return Err(ConxianError::Compliance(
                "Invalid amount: must contain at most one decimal point".to_string(),
            ));
        }

        let scale = frac_part_raw.len();
        if scale > MAX_SCALE {
            return Err(ConxianError::Compliance(
                "Invalid amount: too many decimal places".to_string(),
            ));
        }

        if !int_part_raw.chars().all(|c| c.is_ascii_digit())
            || !frac_part_raw.chars().all(|c| c.is_ascii_digit())
        {
            return Err(ConxianError::Compliance(
                "Invalid amount: must be decimal digits only".to_string(),
            ));
        }

        let int_part = if int_part_raw.is_empty() {
            "0"
        } else {
            int_part_raw
        };

        let minor = int_part
            .chars()
            .chain(frac_part_raw.chars())
            .try_fold(0u64, |acc, c| {
                let digit = c
                    .to_digit(10)
                    .ok_or_else(|| ConxianError::Compliance("Invalid amount".to_string()))?;
                acc.checked_mul(10)
                    .and_then(|v| v.checked_add(digit as u64))
                    .ok_or_else(|| {
                        ConxianError::Compliance("Invalid amount: out of range".to_string())
                    })
            })?;

        Ok((minor, scale as u32))
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

        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(&fields.amount)?;

        Ok(SettlementEnvelope {
            version: conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                source: fields.source,
                transaction_id: fields.transaction_id,
                amount_minor,
                amount_scale,
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
    ) -> ConxianResult<bool> {
        let (device_id, signed_payload) = match attestation {
            AttestationRequest::Ecdsa(a) => (&a.device_id, &a.payload),
            AttestationRequest::Schnorr(a) => (&a.device_id, &a.payload),
            _ => return Ok(false),
        };

        if !device_id.starts_with("conxius-tee-") {
            return Ok(false);
        }

        if signed_payload != payload_hash {
            return Ok(false);
        }

        match self.verify_attestation(attestation) {
            Ok(valid) => Ok(valid),
            Err(ConxianError::Security(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn parse_pacs008_v8(&self, xml: &str) -> ConxianResult<Iso20022Fields> {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;

        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();
        let mut stack: Vec<Vec<u8>> = Vec::new();

        let mut source: Option<SettlementSource> = None;
        let mut transaction_id: Option<String> = None;
        let mut amount: Option<String> = None;
        let mut currency: Option<String> = None;
        let mut sender: Option<String> = None;
        let mut sender_rank: u8 = 0;
        let mut receiver: Option<String> = None;
        let mut receiver_rank: u8 = 0;
        let mut settled_at: Option<u64> = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    if source.is_none() {
                        source = Self::iso20022_source_from_attributes(&e)?;
                    }

                    let name = e.local_name().as_ref().to_vec();
                    if name.as_slice() == b"IntrBkSttlmAmt" && currency.is_none() {
                        for attr in e.attributes() {
                            let attr = attr.map_err(|e| {
                                ConxianError::Compliance(format!("Invalid XML attribute: {e}"))
                            })?;

                            if attr.key.local_name().as_ref() == b"Ccy" {
                                let value = attr.unescape_value().map_err(|e| {
                                    ConxianError::Compliance(format!(
                                        "Invalid XML attribute value: {e}"
                                    ))
                                })?;
                                let ccy = value.trim();
                                if ccy.is_empty() {
                                    return Err(ConxianError::Compliance(
                                        "Empty IntrBkSttlmAmt Ccy attribute".to_string(),
                                    ));
                                }

                                currency = Some(ccy.to_string());
                                break;
                            }
                        }
                    }

                    stack.push(name);
                }
                Ok(Event::Empty(e)) => {
                    if source.is_none() {
                        source = Self::iso20022_source_from_attributes(&e)?;
                    }

                    let name = e.local_name().as_ref().to_vec();
                    if name.as_slice() == b"IntrBkSttlmAmt" && currency.is_none() {
                        for attr in e.attributes() {
                            let attr = attr.map_err(|e| {
                                ConxianError::Compliance(format!("Invalid XML attribute: {e}"))
                            })?;

                            if attr.key.local_name().as_ref() == b"Ccy" {
                                let value = attr.unescape_value().map_err(|e| {
                                    ConxianError::Compliance(format!(
                                        "Invalid XML attribute value: {e}"
                                    ))
                                })?;
                                let ccy = value.trim();
                                if ccy.is_empty() {
                                    return Err(ConxianError::Compliance(
                                        "Empty IntrBkSttlmAmt Ccy attribute".to_string(),
                                    ));
                                }

                                currency = Some(ccy.to_string());
                                break;
                            }
                        }
                    }
                }
                Ok(Event::End(_)) => {
                    stack.pop();
                }
                Ok(Event::Text(e)) => {
                    let text = e
                        .decode()
                        .map_err(|e| ConxianError::Compliance(format!("Invalid XML text: {e}")))?;
                    let text = text.trim();

                    if text.is_empty() {
                        buf.clear();
                        continue;
                    }

                    if transaction_id.is_none() && Self::stack_ends_with(&stack, &[b"MsgId"]) {
                        transaction_id = Some(text.to_string());
                    } else if amount.is_none()
                        && Self::stack_ends_with(&stack, &[b"IntrBkSttlmAmt"])
                    {
                        amount = Some(text.to_string());
                    } else if settled_at.is_none()
                        && (Self::stack_ends_with(&stack, &[b"IntrBkSttlmDtTm"])
                            || Self::stack_ends_with(&stack, &[b"IntrBkSttlmDt"]))
                    {
                        let parsed = Self::parse_iso20022_timestamp(text).ok_or_else(|| {
                            ConxianError::Compliance(format!(
                                "Unparseable ISO 20022 settlement timestamp: {text}"
                            ))
                        })?;

                        settled_at = Some(parsed);
                    }

                    let next_sender_rank =
                        if Self::stack_ends_with(&stack, &[b"DbtrAcct", b"Id", b"Othr", b"Id"])
                            || Self::stack_ends_with(&stack, &[b"DbtrAcct", b"Id", b"IBAN"])
                        {
                            3
                        } else if Self::stack_ends_with(
                            &stack,
                            &[b"DbtrAgt", b"FinInstnId", b"BICFI"],
                        ) || Self::stack_ends_with(
                            &stack,
                            &[b"DbtrAgt", b"FinInstnId", b"BIC"],
                        ) {
                            2
                        } else if Self::stack_ends_with(&stack, &[b"Dbtr", b"Nm"]) {
                            1
                        } else {
                            0
                        };

                    if next_sender_rank > sender_rank {
                        sender = Some(text.to_string());
                        sender_rank = next_sender_rank;
                    }

                    let next_receiver_rank =
                        if Self::stack_ends_with(&stack, &[b"CdtrAcct", b"Id", b"Othr", b"Id"])
                            || Self::stack_ends_with(&stack, &[b"CdtrAcct", b"Id", b"IBAN"])
                        {
                            3
                        } else if Self::stack_ends_with(
                            &stack,
                            &[b"CdtrAgt", b"FinInstnId", b"BICFI"],
                        ) || Self::stack_ends_with(
                            &stack,
                            &[b"CdtrAgt", b"FinInstnId", b"BIC"],
                        ) {
                            2
                        } else if Self::stack_ends_with(&stack, &[b"Cdtr", b"Nm"]) {
                            1
                        } else {
                            0
                        };

                    if next_receiver_rank > receiver_rank {
                        receiver = Some(text.to_string());
                        receiver_rank = next_receiver_rank;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(ConxianError::Compliance(format!("Invalid XML: {e}")));
                }
                _ => (),
            }
            buf.clear();
        }

        Ok(Iso20022Fields {
            source: source.ok_or_else(|| {
                ConxianError::Compliance("Unsupported ISO 20022 message".to_string())
            })?,
            transaction_id: transaction_id
                .ok_or_else(|| ConxianError::Compliance("Missing MsgId".to_string()))?,
            amount: amount
                .ok_or_else(|| ConxianError::Compliance("Missing IntrBkSttlmAmt".to_string()))?,
            currency: currency.ok_or_else(|| {
                ConxianError::Compliance("Missing IntrBkSttlmAmt Ccy attribute".to_string())
            })?,
            sender: sender.ok_or_else(|| {
                ConxianError::Compliance("Missing debtor account identifier".to_string())
            })?,
            receiver: receiver.ok_or_else(|| {
                ConxianError::Compliance("Missing creditor account identifier".to_string())
            })?,
            settled_at,
        })
    }

    fn parse_iso20022_timestamp(value: &str) -> Option<u64> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
            return u64::try_from(dt.with_timezone(&Utc).timestamp()).ok();
        }

        const FIXED_OFFSET_FORMATS: [&str; 2] = ["%Y-%m-%dT%H:%M:%S%.f%z", "%Y-%m-%dT%H:%M:%S%z"];

        for fmt in FIXED_OFFSET_FORMATS {
            if let Ok(dt) = DateTime::parse_from_str(value, fmt) {
                return u64::try_from(dt.with_timezone(&Utc).timestamp()).ok();
            }
        }

        const NAIVE_FORMATS: [&str; 2] = ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S"];

        for fmt in NAIVE_FORMATS {
            if let Ok(dt) = NaiveDateTime::parse_from_str(value, fmt) {
                return u64::try_from(dt.and_utc().timestamp()).ok();
            }
        }

        if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
            return u64::try_from(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp()).ok();
        }

        None
    }

    fn iso20022_source_from_attributes(
        e: &quick_xml::events::BytesStart<'_>,
    ) -> ConxianResult<Option<SettlementSource>> {
        for attr in e.attributes().with_checks(false) {
            let attr = attr
                .map_err(|err| ConxianError::Compliance(format!("Invalid XML attribute: {err}")))?;

            let key = attr.key.as_ref();
            if !Self::is_relevant_iso20022_source_attr_key(key) {
                continue;
            }

            let value = attr.unescape_value().map_err(|err| {
                ConxianError::Compliance(format!("Invalid XML attribute value: {err}"))
            })?;

            if value.contains("pacs.008") {
                return Ok(Some(SettlementSource::Iso20022Pacs008));
            }
            if value.contains("pacs.009") {
                return Ok(Some(SettlementSource::Iso20022Pacs009));
            }
        }

        Ok(None)
    }

    fn is_relevant_iso20022_source_attr_key(key: &[u8]) -> bool {
        let is_xmlns = key == b"xmlns" || key.starts_with(b"xmlns:");
        let is_schema_location = key == b"schemaLocation"
            || key.ends_with(b":schemaLocation")
            || key == b"noNamespaceSchemaLocation"
            || key.ends_with(b":noNamespaceSchemaLocation");

        is_xmlns || is_schema_location
    }

    fn stack_ends_with(stack: &[Vec<u8>], suffix: &[&[u8]]) -> bool {
        if stack.len() < suffix.len() {
            return false;
        }

        stack[stack.len() - suffix.len()..]
            .iter()
            .zip(suffix)
            .all(|(a, b)| a.as_slice() == *b)
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
        self.verify_attestation(&receipt.passkey_attestation)
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

        let job_card_json =
            serde_json::to_string(job_card).map_err(|e| ConxianError::Internal(e.to_string()))?;
        let job_hash = hex::encode(Sha256::digest(job_card_json.as_bytes()));

        let committed = bitvm_attestation
            .state_root
            .split(|c: char| !c.is_ascii_hexdigit())
            .any(|token| token.len() == job_hash.len() && token.eq_ignore_ascii_case(&job_hash));

        if !committed {
            return Err(ConxianError::Security(
                "Job card not committed by BitVM proof".into(),
            ));
        }

        Ok(true)
    }

    #[allow(dead_code)]
    fn decode_base64_or_hex(label: &str, value: &str) -> ConxianResult<Vec<u8>> {
        let value = value.trim();
        if value.starts_with("0x") || value.starts_with("0X") {
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

    fn parse_zkml_image_id(image_id_hex: &str) -> ConxianResult<[u32; 8]> {
        const IMAGE_ID_HEX_LEN: usize = 64;

        let image_id_hex = image_id_hex.trim();
        if image_id_hex.len() != IMAGE_ID_HEX_LEN {
            return Err(ConxianError::Compliance(
                "Invalid proof image format: image_id must be 32 bytes".into(),
            ));
        }

        let mut image_id_bytes = [0u8; 32];
        hex::decode_to_slice(image_id_hex, &mut image_id_bytes)
            .map_err(|_| ConxianError::Compliance("Invalid proof image format".into()))?;

        // Convert into words explicitly so behavior is independent of host endianness.
        let mut image_id = [0u32; 8];
        for (word, chunk) in image_id.iter_mut().zip(image_id_bytes.chunks_exact(4)) {
            *word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        Ok(image_id)
    }

    fn decode_risc0_receipt(bytes: &[u8]) -> ConxianResult<Receipt> {
        if bytes.len() > MAX_ZKML_FIELD_LEN {
            return Err(ConxianError::Compliance(
                "Invalid receipt: payload too large".to_string(),
            ));
        }

        if let Ok(receipt) = Receipt::try_from_slice(bytes) {
            return Ok(receipt);
        }

        let words = Self::bytes_to_words_le(bytes)?;
        risc0_zkvm::serde::from_slice::<Receipt, u32>(&words)
            .map_err(|e| ConxianError::Compliance(format!("Invalid receipt encoding: {e}")))
    }

    fn bytes_to_words_le(bytes: &[u8]) -> ConxianResult<Vec<u32>> {
        if !bytes.len().is_multiple_of(4) {
            return Err(ConxianError::Compliance(
                "Invalid receipt encoding: expected 4-byte word alignment".to_string(),
            ));
        }

        let mut words = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            words.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(words)
    }

    fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() {
            return false;
        }

        memchr::memmem::find(haystack, needle).is_some()
    }

    fn is_even_len_hex(value: &str) -> bool {
        value.len().is_multiple_of(2) && value.as_bytes().iter().all(|b| b.is_ascii_hexdigit())
    }

    fn is_32_byte_hex(value: &str) -> bool {
        value.len() == 64 && Self::is_even_len_hex(value)
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
    fn test_parse_zkml_image_id_maps_words_little_endian() {
        let image_id = ZkcVerifier::parse_zkml_image_id(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .unwrap();

        assert_eq!(
            image_id,
            [
                0x03020100, 0x07060504, 0x0b0a0908, 0x0f0e0d0c, 0x13121110, 0x17161514, 0x1b1a1918,
                0x1f1e1d1c,
            ]
        );
    }

    #[test]
    fn test_parse_zkml_image_id_trims_whitespace() {
        let image_id = ZkcVerifier::parse_zkml_image_id(
            "  000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f  ",
        )
        .unwrap();

        assert_eq!(
            image_id,
            [
                0x03020100, 0x07060504, 0x0b0a0908, 0x0f0e0d0c, 0x13121110, 0x17161514, 0x1b1a1918,
                0x1f1e1d1c,
            ]
        );
    }

    #[test]
    fn test_parse_zkml_image_id_rejects_non_hex_content() {
        let err = ZkcVerifier::parse_zkml_image_id(
            "zz0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .unwrap_err();
        match err {
            ConxianError::Compliance(message) => {
                assert!(message.contains("Invalid proof image format"));
            }
            other => panic!("expected compliance error, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_zkml_image_id_rejects_invalid_length() {
        let err = ZkcVerifier::parse_zkml_image_id("deadbeef").unwrap_err();
        match err {
            ConxianError::Compliance(message) => {
                assert!(message.contains("image_id must be 32 bytes"));
            }
            other => panic!("expected compliance error, got {other:?}"),
        }
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
        assert_eq!(envelope.payload.amount_minor, 50);
        assert_eq!(envelope.payload.amount_scale, 0);
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
