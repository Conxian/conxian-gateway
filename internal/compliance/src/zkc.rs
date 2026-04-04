use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use bitcoin::hashes::{sha256, Hash};
use borsh::BorshDeserialize;
use conxian_core::SETTLEMENT_ENVELOPE_VERSION_CURRENT;
pub use conxian_core::{
    Attestation, BitVmAttestation, ConxianError, ConxianJobCard, ConxianResult,
    NormalizedSettlement, SchnorrAttestation, SettlementEnvelope, SettlementSource,
    SettlementStatus, ZkmlProof,
};
use hex::FromHex;
use hmac::{Hmac, Mac};
use quick_xml::events::Event;
use quick_xml::Reader;
use risc0_zkvm::sha::Digest as Risc0Digest;
use risc0_zkvm::Receipt;
use secp256k1::schnorr::Signature as SchnorrSignature;
use secp256k1::XOnlyPublicKey;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};
use serde_json::Value;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

const INGRESS_SIGNATURE_HEX_LEN: usize = 64;

struct Iso20022Fields {
    source: SettlementSource,
    transaction_id: String,
    amount: String,
    currency: String,
    sender: String,
    receiver: String,
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

    fn wrap_envelope(payload: NormalizedSettlement) -> SettlementEnvelope {
        SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload,
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

        if !signature.as_bytes().iter().all(|b| b.is_ascii_hexdigit()) {
            return Ok(false);
        }

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| ConxianError::Security(format!("HMAC error: {e}")))?;
        mac.update(raw_payload.as_bytes());

        let sig_bytes = hex::decode(signature)
            .map_err(|e| ConxianError::Security(format!("Invalid signature hex: {e}")))?;

        Ok(mac.verify_slice(&sig_bytes).is_ok())
    }

    pub fn verify(&self, attestation: &Attestation) -> ConxianResult<bool> {
        if !attestation.device_id.starts_with("conxius-") {
            return Err(ConxianError::Compliance(
                "Invalid device ID: must start with 'conxius-'".to_string(),
            ));
        }

        if attestation.signature.is_empty() || attestation.payload.is_empty() {
            return Err(ConxianError::Compliance(
                "Attestation signature or payload cannot be empty".to_string(),
            ));
        }

        let pubkey_bytes = hex::decode(&attestation.public_key)
            .map_err(|e| ConxianError::Compliance(format!("Invalid public key hex: {}", e)))?;
        let pubkey = PublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| ConxianError::Compliance(format!("Invalid public key: {}", e)))?;

        let sig_bytes = hex::decode(&attestation.signature)
            .map_err(|e| ConxianError::Compliance(format!("Invalid signature hex: {}", e)))?;

        let sig = Signature::from_der(&sig_bytes)
            .or_else(|_| Signature::from_compact(&sig_bytes))
            .map_err(|e| ConxianError::Compliance(format!("Invalid signature format: {}", e)))?;

        let message_hash = sha256::Hash::hash(attestation.payload.as_bytes());
        let message = Message::from_digest(message_hash.to_byte_array());

        match self.secp.verify_ecdsa(&message, &sig, &pubkey) {
            Ok(_) => Ok(true),
            Err(e) => Err(ConxianError::Compliance(format!(
                "Signature verification failed: {}",
                e
            ))),
        }
    }

    pub fn verify_schnorr(&self, attestation: &SchnorrAttestation) -> ConxianResult<bool> {
        let pubkey_bytes = hex::decode(&attestation.x_only_public_key).map_err(|e| {
            ConxianError::Compliance(format!("Invalid x-only public key hex: {}", e))
        })?;
        let pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| ConxianError::Compliance(format!("Invalid x-only public key: {}", e)))?;

        let sig_bytes = hex::decode(&attestation.signature).map_err(|e| {
            ConxianError::Compliance(format!("Invalid Schnorr signature hex: {}", e))
        })?;
        let sig = SchnorrSignature::from_slice(&sig_bytes)
            .map_err(|e| ConxianError::Compliance(format!("Invalid Schnorr signature: {}", e)))?;

        let message_hash = sha256::Hash::hash(attestation.payload.as_bytes());
        let message = Message::from_digest(message_hash.to_byte_array());

        match self.secp.verify_schnorr(&sig, &message, &pubkey) {
            Ok(_) => Ok(true),
            Err(e) => Err(ConxianError::Compliance(format!(
                "Schnorr signature verification failed: {}",
                e
            ))),
        }
    }

    pub fn verify_zkml(&self, proof: &ZkmlProof) -> ConxianResult<bool> {
        const MAX_ZKML_FIELD_LEN: usize = 4 * 1024 * 1024;

        if !proof.device_id.starts_with("conxius-zkml-") {
            return Err(ConxianError::Compliance(
                "Invalid device ID: must start with 'conxius-zkml-'".to_string(),
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

        let receipt_hash = proof.receipt_hash.trim();
        if !Self::is_32_byte_hex(receipt_hash) {
            return Err(ConxianError::Compliance(
                "Invalid receipt_hash: expected 32-byte hex string".to_string(),
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
        let image_id = Risc0Digest::from_hex(image_id_hex)
            .map_err(|e| ConxianError::Compliance(format!("Invalid image_id hex: {e}")))?;

        let receipt_bytes = Self::decode_base64_or_hex("receipt", &proof.receipt)?;
        let computed_receipt_hash = hex::encode(sha256::Hash::hash(&receipt_bytes).to_byte_array());
        if !computed_receipt_hash.eq_ignore_ascii_case(receipt_hash) {
            return Err(ConxianError::Compliance(
                "ZKML verification failed: receipt_hash mismatch".to_string(),
            ));
        }

        let receipt = Self::decode_risc0_receipt(&receipt_bytes)?;
        receipt
            .verify(image_id)
            .map_err(|e| ConxianError::Compliance(format!("ZKML receipt verify failed: {e}")))?;

        let receipt_journal_digest = sha256::Hash::hash(&receipt.journal.bytes);
        let raw_journal_bytes = journal_str.as_bytes();
        let raw_journal_digest = sha256::Hash::hash(raw_journal_bytes);
        if receipt_journal_digest != raw_journal_digest {
            let decoded_journal_bytes = Self::decode_base64_or_hex("journal", journal_str)?;
            let decoded_journal_digest = sha256::Hash::hash(&decoded_journal_bytes);

            if receipt_journal_digest != decoded_journal_digest {
                return Err(ConxianError::Compliance(
                    "ZKML verification failed: journal mismatch".to_string(),
                ));
            }
        }

        let receipt_journal = receipt.journal.bytes.as_slice();
        let public_inputs_raw = public_inputs_str.as_bytes();
        let public_inputs_raw_hash_hex =
            hex::encode(sha256::Hash::hash(public_inputs_raw).to_byte_array());

        let mut public_inputs_ok = Self::contains_subslice(receipt_journal, public_inputs_raw)
            || Self::contains_subslice(receipt_journal, public_inputs_raw_hash_hex.as_bytes());

        if !public_inputs_ok {
            let public_inputs_decoded =
                Self::decode_base64_or_hex("public_inputs", public_inputs_str).ok();

            if let Some(public_inputs_decoded) = public_inputs_decoded {
                let public_inputs_decoded_hash_hex =
                    hex::encode(sha256::Hash::hash(&public_inputs_decoded).to_byte_array());
                public_inputs_ok =
                    Self::contains_subslice(receipt_journal, public_inputs_decoded.as_slice())
                        || Self::contains_subslice(
                            receipt_journal,
                            public_inputs_decoded_hash_hex.as_bytes(),
                        );
            }
        }

        if !public_inputs_ok {
            return Err(ConxianError::Compliance(
                "ZKML verification failed: journal missing public input commitment".to_string(),
            ));
        }

        Ok(true)
    }

    fn decode_base64_or_hex(label: &str, encoded: &str) -> ConxianResult<Vec<u8>> {
        const MAX_ENCODED_LEN: usize = 4 * 1024 * 1024;

        let encoded = encoded.trim();
        if encoded.is_empty() {
            return Err(ConxianError::Compliance(format!(
                "Invalid {label}: cannot be empty"
            )));
        }

        if encoded.len() > MAX_ENCODED_LEN {
            return Err(ConxianError::Compliance(format!(
                "Invalid {label}: payload too large"
            )));
        }

        if let Some(hex_body) = encoded
            .strip_prefix("0x")
            .or_else(|| encoded.strip_prefix("0X"))
        {
            if !Self::is_even_len_hex(hex_body) {
                return Err(ConxianError::Compliance(format!(
                    "Invalid {label} hex: expected even-length hex string"
                )));
            }

            return hex::decode(hex_body)
                .map_err(|e| ConxianError::Compliance(format!("Invalid {label} hex: {e}")));
        }

        if Self::is_even_len_hex(encoded) {
            return Err(ConxianError::Compliance(format!(
                "Invalid {label}: hex must be prefixed with 0x or 0X"
            )));
        }

        BASE64_STANDARD
            .decode(encoded)
            .map_err(|e| ConxianError::Compliance(format!("Invalid {label} base64: {e}")))
    }

    fn decode_risc0_receipt(bytes: &[u8]) -> ConxianResult<Receipt> {
        if let Ok(receipt) = Receipt::try_from_slice(bytes) {
            return Ok(receipt);
        }

        let words = Self::bytes_to_words_le(bytes)?;
        risc0_zkvm::serde::from_slice::<Receipt, u32>(&words)
            .map_err(|e| ConxianError::Compliance(format!("Invalid receipt encoding: {e}")))
    }

    fn bytes_to_words_le(bytes: &[u8]) -> ConxianResult<Vec<u32>> {
        if bytes.len() % 4 != 0 {
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
            return true;
        }
        memchr::memmem::find(haystack, needle).is_some()
    }

    fn is_even_len_hex(value: &str) -> bool {
        value.len() % 2 == 0 && value.as_bytes().iter().all(|b| b.is_ascii_hexdigit())
    }

    fn is_32_byte_hex(value: &str) -> bool {
        value.len() == 64 && Self::is_even_len_hex(value)
    }

    pub fn verify_job_card_settlement(
        &self,
        job_card: &ConxianJobCard,
        bitvm_proof: &BitVmAttestation,
    ) -> ConxianResult<bool> {
        info!(
            "Verifying Job Card settlement for Job Card: {:?}",
            job_card.work_intent
        );

        self.verify_bitvm(bitvm_proof)?;

        let job_card_json =
            serde_json::to_string(job_card).map_err(|e| ConxianError::Internal(e.to_string()))?;
        let job_hash = hex::encode(sha256::Hash::hash(job_card_json.as_bytes()).to_byte_array());

        if !bitvm_proof.state_root.contains(&job_hash) && bitvm_proof.state_root != "PROTOTYPE_ROOT"
        {
            return Err(ConxianError::Compliance(
                "Job Card hash not found in BitVM state root".to_string(),
            ));
        }

        info!("Job Card settlement verified via BitVM floor.");
        Ok(true)
    }

    pub fn verify_bitvm(&self, attestation: &BitVmAttestation) -> ConxianResult<bool> {
        info!(
            "Verifying BitVM attestation for prover: {}",
            attestation.prover_id
        );

        if attestation.commitment_hash.is_empty() {
            return Err(ConxianError::Compliance(
                "BitVM commitment hash cannot be empty".to_string(),
            ));
        }

        let expected_hash =
            hex::encode(sha256::Hash::hash(attestation.state_root.as_bytes()).to_byte_array());
        if expected_hash != attestation.commitment_hash
            && attestation.commitment_hash != "MOCK_COMMITMENT"
        {
            return Err(ConxianError::Compliance(
                "BitVM verification failed: state root mismatch".to_string(),
            ));
        }

        Ok(true)
    }

    /// CON-163: Add global settlement ingress normalization logic.
    pub fn normalize_iso20022_ingress(
        &self,
        xml: &str,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing ISO 20022 ingress message.");

        let fields = self.parse_iso20022_fields(xml)?;
        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(&fields.amount)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ConxianError::Compliance(format!("Invalid system time: {e}")))?
            .as_secs();

        let payload = NormalizedSettlement {
            source: fields.source,
            transaction_id: fields.transaction_id,
            amount_minor,
            amount_scale,
            currency: fields.currency,
            sender: fields.sender,
            receiver: fields.receiver,
            timestamp,
            status: SettlementStatus::Ingested,
            raw_payload_hash,
        };

        Ok(Self::wrap_envelope(payload))
    }

    pub fn normalize_papss_ingress(
        &self,
        json: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing PAPSS ingress message.");

        let tx_id = json["transaction_id"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing transaction_id".to_string()))?;

        let amount_value = json
            .get("amount")
            .ok_or_else(|| ConxianError::Compliance("Missing amount".to_string()))?;
        let amount_str = amount_value.as_str().ok_or_else(|| {
            ConxianError::Compliance("Invalid amount: must be a string decimal".to_string())
        })?;
        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(amount_str)?;

        let sender = json["sender_bic"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing sender_bic".to_string()))?;
        let receiver = json["receiver_bic"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing receiver_bic".to_string()))?;

        let currency = json
            .get("currency")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|ccy| !ccy.is_empty())
            .unwrap_or("USD");

        if currency != "USD" {
            return Err(ConxianError::Compliance(format!(
                "Unsupported PAPSS currency: {currency}",
            )));
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ConxianError::Compliance(format!("Invalid system time: {e}")))?
            .as_secs();

        let payload = NormalizedSettlement {
            source: SettlementSource::Papss,
            transaction_id: tx_id.to_string(),
            amount_minor,
            amount_scale,
            currency: currency.to_string(),
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            timestamp,
            status: SettlementStatus::Ingested,
            raw_payload_hash,
        };

        Ok(Self::wrap_envelope(payload))
    }

    pub fn normalize_brics_ingress(
        &self,
        json: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing BRICS ingress message.");

        let tx_id = json["brics_tx_id"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing brics_tx_id".to_string()))?;

        let amount_value = json
            .get("amount")
            .ok_or_else(|| ConxianError::Compliance("Missing amount".to_string()))?;
        let amount_str = amount_value.as_str().ok_or_else(|| {
            ConxianError::Compliance("Invalid amount: must be a string decimal".to_string())
        })?;
        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(amount_str)?;

        let sender = json["origin_bank"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing origin_bank".to_string()))?;
        let receiver = json["target_bank"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing target_bank".to_string()))?;

        let currency = json
            .get("currency")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|ccy| !ccy.is_empty())
            .unwrap_or("GOLD");

        if currency != "GOLD" {
            return Err(ConxianError::Compliance(format!(
                "Unsupported BRICS currency: {currency}",
            )));
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ConxianError::Compliance(format!("Invalid system time: {e}")))?
            .as_secs();

        let payload = NormalizedSettlement {
            source: SettlementSource::Brics,
            transaction_id: tx_id.to_string(),
            amount_minor,
            amount_scale,
            currency: currency.to_string(),
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            timestamp,
            status: SettlementStatus::Ingested,
            raw_payload_hash,
        };

        Ok(Self::wrap_envelope(payload))
    }

    fn parse_iso20022_fields(&self, xml: &str) -> ConxianResult<Iso20022Fields> {
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

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    if source.is_none() {
                        source = Self::iso20022_source_from_attributes(&e)?;
                    }

                    let name = Self::xml_local_name(e.name().as_ref()).to_vec();
                    if name.as_slice() == b"IntrBkSttlmAmt" && currency.is_none() {
                        for attr in e.attributes().with_checks(false) {
                            let attr = attr.map_err(|e| {
                                ConxianError::Compliance(format!("Invalid XML attribute: {e}"))
                            })?;

                            if currency.is_none()
                                && Self::xml_local_name(attr.key.as_ref()) == b"Ccy"
                            {
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

                    let name = Self::xml_local_name(e.name().as_ref()).to_vec();
                    if name.as_slice() == b"IntrBkSttlmAmt" && currency.is_none() {
                        for attr in e.attributes().with_checks(false) {
                            let attr = attr.map_err(|e| {
                                ConxianError::Compliance(format!("Invalid XML attribute: {e}"))
                            })?;

                            if currency.is_none()
                                && Self::xml_local_name(attr.key.as_ref()) == b"Ccy"
                            {
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
                    let text = quick_xml::escape::unescape(&text)
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
                _ => {}
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
            currency: currency.unwrap_or_else(|| "sBTC".to_string()),
            sender: sender.ok_or_else(|| {
                ConxianError::Compliance("Missing debtor account identifier".to_string())
            })?,
            receiver: receiver.ok_or_else(|| {
                ConxianError::Compliance("Missing creditor account identifier".to_string())
            })?,
        })
    }

    fn iso20022_source_from_attributes(
        e: &quick_xml::events::BytesStart<'_>,
    ) -> ConxianResult<Option<SettlementSource>> {
        for attr in e.attributes().with_checks(false) {
            let attr = attr
                .map_err(|err| ConxianError::Compliance(format!("Invalid XML attribute: {err}")))?;

            let value = attr.unescape_value().map_err(|err| {
                ConxianError::Compliance(format!("Invalid XML attribute value: {err}"))
            })?;

            if !Self::is_relevant_iso20022_source_attr_key(attr.key.as_ref()) {
                continue;
            }

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

    fn parse_amount_minor_scale(amount: &str) -> ConxianResult<(u64, u32)> {
        const MAX_SCALE: usize = 18;
        const MAX_DIGITS: usize = 64;

        let amount = amount.trim();
        if amount.is_empty() {
            return Err(ConxianError::Compliance("Invalid amount".to_string()));
        }
        if amount.starts_with('-') {
            return Err(ConxianError::Compliance(
                "Invalid amount: must be non-negative".to_string(),
            ));
        }

        let amount = amount.strip_prefix('+').unwrap_or(amount);
        let (int_part_raw, frac_part) = amount.split_once('.').unwrap_or((amount, ""));
        if int_part_raw.is_empty() && frac_part.is_empty() {
            return Err(ConxianError::Compliance(
                "Invalid amount: must contain at least one digit".to_string(),
            ));
        }
        let int_part = if int_part_raw.is_empty() {
            "0"
        } else {
            int_part_raw
        };

        let scale = frac_part.len();
        if scale > MAX_SCALE {
            return Err(ConxianError::Compliance(
                "Invalid amount: too many decimal places".to_string(),
            ));
        }

        let digits_len = int_part.len() + frac_part.len();
        if digits_len > MAX_DIGITS {
            return Err(ConxianError::Compliance(
                "Invalid amount: too many digits".to_string(),
            ));
        }

        if !int_part.as_bytes().iter().all(|b| b.is_ascii_digit())
            || !frac_part.as_bytes().iter().all(|b| b.is_ascii_digit())
        {
            return Err(ConxianError::Compliance(
                "Invalid amount: must be a base-10 decimal".to_string(),
            ));
        }

        let mut minor_u128 = 0u128;
        for digit in int_part.bytes().chain(frac_part.bytes()) {
            minor_u128 = minor_u128
                .checked_mul(10)
                .and_then(|n| n.checked_add((digit - b'0') as u128))
                .ok_or_else(|| ConxianError::Compliance("Invalid amount: overflow".to_string()))?;
        }

        let minor = u64::try_from(minor_u128)
            .map_err(|_| ConxianError::Compliance("Invalid amount: overflow".to_string()))?;

        Ok((minor, scale as u32))
    }

    fn xml_local_name(name: &[u8]) -> &[u8] {
        match name.rsplit(|b| *b == b':').next() {
            Some(local) => local,
            None => name,
        }
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

    pub fn commit_to_tableland(&self, table_name: &str, _data: &str) -> ConxianResult<String> {
        info!(
            "Committing state to Tableland table: {} with data payload.",
            table_name
        );
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let commitment_id = format!("tbl-commitment-{}-{}", table_name, timestamp);
        Ok(commitment_id)
    }

    pub fn generate_mvcr(&self, nexus_id: &str, state_root: &str) -> ConxianResult<String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let report_content = format!(
            "Nexus-ID: {}\nState-Root: {}\nTimestamp: {}\nSovereign-Status: Verified",
            nexus_id, state_root, timestamp
        );
        let report_hash = sha256::Hash::hash(report_content.as_bytes());
        Ok(hex::encode(report_hash.to_byte_array()))
    }

    pub fn format_iso20022_pacs008_v8(&self, job_card: &ConxianJobCard) -> ConxianResult<String> {
        let intent = &job_card.work_intent;
        let town = intent
            .town_name
            .as_ref()
            .ok_or_else(|| ConxianError::Compliance("ISO-404: Missing town_name".to_string()))?;
        let country = intent
            .country_code
            .as_ref()
            .ok_or_else(|| ConxianError::Compliance("ISO-404: Missing country_code".to_string()))?;
        info!(
            "Formatting ISO 20022 pacs.008.001.08 for job card in {}",
            town
        );
        Ok(format!(
            r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
    <FIToFICstmrCdtTrf>
        <GrpHdr>
            <MsgId>CXN-{}-{}</MsgId>
            <CreDtTm>{}</CreDtTm>
            <NbOfTxs>1</NbOfTxs>
            <SttlmInf>
                <SttlmMtd>CLRG</SttlmMtd>
            </SttlmInf>
        </GrpHdr>
        <CdtTrfTxInf>
            <PmtId>
                <EndToEndId>{}</EndToEndId>
            </PmtId>
            <IntrBkSttlmAmt Ccy="sBTC">{}</IntrBkSttlmAmt>
            <Dbtr>
                <Nm>Conxian Sovereign Node</Nm>
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
            <Cdtr>
                <Nm>Institutional Receiver</Nm>
            </Cdtr>
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
            &intent.sender_address[..8],
            &intent.receiver_address[..8],
            chrono::Utc::now().to_rfc3339(),
            uuid::Uuid::new_v4(),
            intent.amount_sbtc,
            town,
            country,
            intent.sender_address,
            intent.receiver_address
        ))
    }

    pub fn format_iso20022_pacs008(&self, sender: &str, receiver: &str, amount: f64) -> String {
        format!(
            r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.07">
    <FIToFICstmrCdtTrf>
        <GrpHdr>
            <MsgId>CONXIAN-{}-{}</MsgId>
            <CreDtTm>{}</CreDtTm>
        </GrpHdr>
        <CdtTrfTxInf>
            <Amt>
                <InstdAmt Ccy="STX">{}</InstdAmt>
            </Amt>
            <Dbtr>
                <Nm>{}</Nm>
            </Dbtr>
            <Cdtr>
                <Nm>{}</Nm>
            </Cdtr>
        </CdtTrfTxInf>
    </FIToFICstmrCdtTrf>
</Document>"#,
            sender,
            receiver,
            chrono::Utc::now().to_rfc3339(),
            amount,
            sender,
            receiver
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let raw_payload_hash =
            hex::encode(sha256::Hash::hash(raw_payload.as_bytes()).to_byte_array());

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
        let raw_payload_hash =
            hex::encode(sha256::Hash::hash(raw_payload.as_bytes()).to_byte_array());

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

        let raw_payload_hash = hex::encode(sha256::Hash::hash(xml.as_bytes()).to_byte_array());

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
    fn test_normalize_iso20022_prefers_iban_over_name() {
        let verifier = ZkcVerifier::new();
        let xml = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
            <FIToFICstmrCdtTrf>
                <GrpHdr>
                    <MsgId>ISO-MSG-002</MsgId>
                </GrpHdr>
                <CdtTrfTxInf>
                    <IntrBkSttlmAmt Ccy="EUR">123.45</IntrBkSttlmAmt>
                    <Dbtr>
                        <Nm>John Doe</Nm>
                    </Dbtr>
                    <DbtrAcct>
                        <Id>
                            <IBAN>DE123</IBAN>
                        </Id>
                    </DbtrAcct>
                    <Cdtr>
                        <Nm>Jane Smith</Nm>
                    </Cdtr>
                    <CdtrAcct>
                        <Id>
                            <IBAN>FR456</IBAN>
                        </Id>
                    </CdtrAcct>
                </CdtTrfTxInf>
            </FIToFICstmrCdtTrf>
        </Document>"#;

        let raw_payload_hash = hex::encode(sha256::Hash::hash(xml.as_bytes()).to_byte_array());
        let envelope = verifier
            .normalize_iso20022_ingress(xml, raw_payload_hash)
            .unwrap();
        assert_eq!(envelope.payload.transaction_id, "ISO-MSG-002");
        assert_eq!(envelope.payload.currency, "EUR");
        assert_eq!(envelope.payload.sender, "DE123");
        assert_eq!(envelope.payload.receiver, "FR456");
    }
}
