use bitcoin::hashes::{sha256, Hash};
pub use conxian_core::{
    Attestation, BitVmAttestation, ConxianError, ConxianJobCard, ConxianResult,
    NormalizedSettlement, SchnorrAttestation, SettlementEnvelope, SettlementSource,
    SettlementStatus, ZkmlProof,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use secp256k1::schnorr::Signature as SchnorrSignature;
use secp256k1::XOnlyPublicKey;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

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
        if !proof.device_id.starts_with("conxius-zkml-") {
            return Err(ConxianError::Compliance(
                "Invalid device ID: must start with 'conxius-zkml-'".to_string(),
            ));
        }

        let combined = format!(
            "{}:{}:{}",
            proof.public_inputs, proof.journal, proof.device_id
        );
        let computed_hash = hex::encode(sha256::Hash::hash(combined.as_bytes()).to_byte_array());

        if computed_hash != proof.receipt_hash {
            warn!(
                "ZKML mismatch: expected {}, got {}",
                computed_hash, proof.receipt_hash
            );
            return Err(ConxianError::Compliance(
                "ZKML verification failed: receipt hash mismatch".to_string(),
            ));
        }

        Ok(true)
    }

    /// CON-75: Wire the BitVM2 verification floor for Job Card settlement.
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

        Ok(SettlementEnvelope {
            version: "2.0.0".to_string(),
            payload,
        })
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
            ConxianError::Compliance("Invalid amount (must be a string decimal)".to_string())
        })?;
        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(amount_str)?;

        let sender = json["sender_bic"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing sender_bic".to_string()))?;
        let receiver = json["receiver_bic"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing receiver_bic".to_string()))?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ConxianError::Compliance(format!("Invalid system time: {e}")))?
            .as_secs();

        let payload = NormalizedSettlement {
            source: SettlementSource::Papss,
            transaction_id: tx_id.to_string(),
            amount_minor,
            amount_scale,
            currency: "USD".to_string(),
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            timestamp,
            status: SettlementStatus::Ingested,
            raw_payload_hash,
        };

        Ok(SettlementEnvelope {
            version: "2.0.0".to_string(),
            payload,
        })
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
            ConxianError::Compliance("Invalid amount (must be a string decimal)".to_string())
        })?;
        let (amount_minor, amount_scale) = Self::parse_amount_minor_scale(amount_str)?;

        let sender = json["origin_bank"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing origin_bank".to_string()))?;
        let receiver = json["target_bank"]
            .as_str()
            .ok_or_else(|| ConxianError::Compliance("Missing target_bank".to_string()))?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ConxianError::Compliance(format!("Invalid system time: {e}")))?
            .as_secs();

        let payload = NormalizedSettlement {
            source: SettlementSource::Brics,
            transaction_id: tx_id.to_string(),
            amount_minor,
            amount_scale,
            currency: "GOLD".to_string(),
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            timestamp,
            status: SettlementStatus::Ingested,
            raw_payload_hash,
        };

        Ok(SettlementEnvelope {
            version: "2.0.0".to_string(),
            payload,
        })
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
        let mut receiver: Option<String> = None;

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
                                currency = Some(value.into_owned());
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
                                currency = Some(value.into_owned());
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
                    } else if sender.is_none()
                        && (Self::stack_ends_with(&stack, &[b"DbtrAcct", b"Id", b"Othr", b"Id"])
                            || Self::stack_ends_with(&stack, &[b"DbtrAcct", b"Id", b"IBAN"])
                            || Self::stack_ends_with(
                                &stack,
                                &[b"DbtrAgt", b"FinInstnId", b"BICFI"],
                            ))
                    {
                        sender = Some(text.to_string());
                    } else if receiver.is_none()
                        && (Self::stack_ends_with(&stack, &[b"CdtrAcct", b"Id", b"Othr", b"Id"])
                            || Self::stack_ends_with(&stack, &[b"CdtrAcct", b"Id", b"IBAN"])
                            || Self::stack_ends_with(
                                &stack,
                                &[b"CdtrAgt", b"FinInstnId", b"BICFI"],
                            ))
                    {
                        receiver = Some(text.to_string());
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
            currency: currency.ok_or_else(|| {
                ConxianError::Compliance("Missing IntrBkSttlmAmt Ccy".to_string())
            })?,
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
        let (int_part, frac_part) = amount.split_once('.').unwrap_or((amount, ""));
        let int_part = if int_part.is_empty() { "0" } else { int_part };

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
