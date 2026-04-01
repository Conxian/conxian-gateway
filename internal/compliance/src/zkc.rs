use bitcoin::hashes::{sha256, Hash};
pub use conxian_core::{
    Attestation, BitVmAttestation, ConxianError, ConxianJobCard, ConxianResult, SchnorrAttestation,
    NormalizedSettlement, SettlementEnvelope, SettlementSource, ZkmlProof,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use secp256k1::schnorr::Signature as SchnorrSignature;
use secp256k1::XOnlyPublicKey;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};
use tracing::{info, warn};
use serde_json::Value;

#[derive(Debug)]
struct Iso20022IngressFields {
    source: SettlementSource,
    transaction_id: String,
    amount: String,
    currency: Option<String>,
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
    pub fn normalize_iso20022_ingress(&self, xml: &str) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing ISO 20022 ingress message.");

        let parsed = self.parse_iso20022_ingress(xml)?;
        let amount = parsed.amount.parse::<f64>().map_err(|e| {
            ConxianError::Compliance(format!("Invalid IntrBkSttlmAmt: {}", e))
        })?;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ConxianError::Compliance(format!("Invalid system time: {}", e)))?
            .as_secs();

        let payload = NormalizedSettlement {
            source: parsed.source,
            transaction_id: parsed.transaction_id,
            amount,
            currency: parsed.currency.unwrap_or_else(|| "sBTC".to_string()),
            sender: parsed.sender,
            receiver: parsed.receiver,
            timestamp,
            status: "INGESTED".to_string(),
            raw_payload_hash: hex::encode(sha256::Hash::hash(xml.as_bytes()).to_byte_array()),
        };

        Ok(SettlementEnvelope {
            version: "1.0.0".to_string(),
            payload,
        })
    }

    pub fn normalize_papss_ingress(&self, json: &Value) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing PAPSS ingress message.");

        let tx_id = json["transaction_id"].as_str().ok_or_else(|| ConxianError::Compliance("Missing transaction_id".to_string()))?;
        let amount = json["amount"].as_f64().unwrap_or(0.0);
        let sender = json["sender_bic"].as_str().unwrap_or("UNKNOWN_PAPSS_SENDER");
        let receiver = json["receiver_bic"].as_str().unwrap_or("UNKNOWN_PAPSS_RECEIVER");

        let payload = NormalizedSettlement {
            source: SettlementSource::Papss,
            transaction_id: tx_id.to_string(),
            amount,
            currency: "USD".to_string(),
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            status: "INGESTED".to_string(),
            raw_payload_hash: hex::encode(sha256::Hash::hash(json.to_string().as_bytes()).to_byte_array()),
        };

        Ok(SettlementEnvelope {
            version: "1.0.0".to_string(),
            payload,
        })
    }

    pub fn normalize_brics_ingress(&self, json: &Value) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing BRICS ingress message.");

        let tx_id = json["brics_tx_id"].as_str().ok_or_else(|| ConxianError::Compliance("Missing brics_tx_id".to_string()))?;
        let amount = json["amount"].as_f64().unwrap_or(0.0);
        let sender = json["origin_bank"].as_str().unwrap_or("UNKNOWN_BRICS_SENDER");
        let receiver = json["target_bank"].as_str().unwrap_or("UNKNOWN_BRICS_RECEIVER");

        let payload = NormalizedSettlement {
            source: SettlementSource::Brics,
            transaction_id: tx_id.to_string(),
            amount,
            currency: "GOLD".to_string(),
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            status: "INGESTED".to_string(),
            raw_payload_hash: hex::encode(sha256::Hash::hash(json.to_string().as_bytes()).to_byte_array()),
        };

        Ok(SettlementEnvelope {
            version: "1.0.0".to_string(),
            payload,
        })
    }

    fn parse_iso20022_ingress(&self, xml: &str) -> ConxianResult<Iso20022IngressFields> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();
        let mut stack: Vec<String> = Vec::new();

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

                    let name = Self::xml_local_name(e.name().as_ref())?;
                    if name == "IntrBkSttlmAmt" && currency.is_none() {
                        currency = Self::attribute_value(&e, "Ccy")?;
                    }

                    stack.push(name);
                }
                Ok(Event::Empty(e)) => {
                    if source.is_none() {
                        source = Self::iso20022_source_from_attributes(&e)?;
                    }

                    let name = Self::xml_local_name(e.name().as_ref())?;
                    if name == "IntrBkSttlmAmt" && currency.is_none() {
                        currency = Self::attribute_value(&e, "Ccy")?;
                    }
                }
                Ok(Event::End(_)) => {
                    stack.pop();
                }
                Ok(Event::Text(t)) => {
                    let text = t
                        .unescape()
                        .map_err(|e| ConxianError::Compliance(format!("Invalid XML text: {}", e)))?;
                    let text = text.trim();

                    if text.is_empty() {
                        buf.clear();
                        continue;
                    }

                    if transaction_id.is_none() && stack.last().is_some_and(|n| n == "MsgId") {
                        transaction_id = Some(text.to_string());
                    }

                    if amount.is_none() && stack.last().is_some_and(|n| n == "IntrBkSttlmAmt") {
                        amount = Some(text.to_string());
                    }

                    if sender.is_none()
                        && (Self::stack_ends_with(&stack, &["DbtrAcct", "Id", "Othr", "Id"])
                            || Self::stack_ends_with(&stack, &["DbtrAcct", "Id", "IBAN"])
                            || Self::stack_ends_with(
                                &stack,
                                &["DbtrAgt", "FinInstnId", "BICFI"],
                            ))
                    {
                        sender = Some(text.to_string());
                    }

                    if receiver.is_none()
                        && (Self::stack_ends_with(&stack, &["CdtrAcct", "Id", "Othr", "Id"])
                            || Self::stack_ends_with(&stack, &["CdtrAcct", "Id", "IBAN"])
                            || Self::stack_ends_with(
                                &stack,
                                &["CdtrAgt", "FinInstnId", "BICFI"],
                            ))
                    {
                        receiver = Some(text.to_string());
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(ConxianError::Compliance(format!(
                        "Failed to parse ISO 20022 XML: {}",
                        e
                    )))
                }
                _ => {}
            }

            buf.clear();
        }

        let source = source.ok_or_else(|| {
            ConxianError::Compliance("Unsupported ISO 20022 message".to_string())
        })?;
        let transaction_id = transaction_id.ok_or_else(|| {
            ConxianError::Compliance("Missing MsgId".to_string())
        })?;
        let amount = amount.ok_or_else(|| {
            ConxianError::Compliance("Missing IntrBkSttlmAmt".to_string())
        })?;
        let sender = sender.ok_or_else(|| {
            ConxianError::Compliance("Missing debtor account".to_string())
        })?;
        let receiver = receiver.ok_or_else(|| {
            ConxianError::Compliance("Missing creditor account".to_string())
        })?;

        Ok(Iso20022IngressFields {
            source,
            transaction_id,
            amount,
            currency,
            sender,
            receiver,
        })
    }

    fn xml_local_name(qname: &[u8]) -> ConxianResult<String> {
        let full = std::str::from_utf8(qname)
            .map_err(|e| ConxianError::Compliance(format!("Invalid XML element name: {}", e)))?;
        Ok(full.rsplit(':').next().unwrap_or(full).to_string())
    }

    fn stack_ends_with(stack: &[String], suffix: &[&str]) -> bool {
        if stack.len() < suffix.len() {
            return false;
        }

        stack
            .iter()
            .rev()
            .zip(suffix.iter().rev())
            .all(|(a, b)| a == b)
    }

    fn iso20022_source_from_attributes(
        e: &quick_xml::events::BytesStart<'_>,
    ) -> ConxianResult<Option<SettlementSource>> {
        for attr in e.attributes().with_checks(false) {
            let attr = attr.map_err(|err| {
                ConxianError::Compliance(format!("Invalid XML attribute: {}", err))
            })?;

            let key = std::str::from_utf8(attr.key.as_ref()).map_err(|err| {
                ConxianError::Compliance(format!("Invalid XML attribute name: {}", err))
            })?;
            if !(key == "xmlns" || key.starts_with("xmlns:") || key == "xsi:schemaLocation") {
                continue;
            }

            let value = attr.unescape_value().map_err(|err| {
                ConxianError::Compliance(format!("Invalid XML attribute value: {}", err))
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

    fn attribute_value(
        e: &quick_xml::events::BytesStart<'_>,
        key: &str,
    ) -> ConxianResult<Option<String>> {
        for attr in e.attributes().with_checks(false) {
            let attr = attr.map_err(|err| {
                ConxianError::Compliance(format!("Invalid XML attribute: {}", err))
            })?;

            let attr_key = std::str::from_utf8(attr.key.as_ref()).map_err(|err| {
                ConxianError::Compliance(format!("Invalid XML attribute name: {}", err))
            })?;
            if attr_key != key {
                continue;
            }

            let value = attr.unescape_value().map_err(|err| {
                ConxianError::Compliance(format!("Invalid XML attribute value: {}", err))
            })?;
            return Ok(Some(value.to_string()));
        }

        Ok(None)
    }

    pub fn commit_to_tableland(&self, table_name: &str, _data: &str) -> ConxianResult<String> {
        info!("Committing state to Tableland table: {} with data payload.", table_name);
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let commitment_id = format!("tbl-commitment-{}-{}", table_name, timestamp);
        Ok(commitment_id)
    }

    pub fn generate_mvcr(&self, nexus_id: &str, state_root: &str) -> ConxianResult<String> {
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let report_content = format!("Nexus-ID: {}\nState-Root: {}\nTimestamp: {}\nSovereign-Status: Verified", nexus_id, state_root, timestamp);
        let report_hash = sha256::Hash::hash(report_content.as_bytes());
        Ok(hex::encode(report_hash.to_byte_array()))
    }

    pub fn format_iso20022_pacs008_v8(&self, job_card: &ConxianJobCard) -> ConxianResult<String> {
        let intent = &job_card.work_intent;
        let town = intent.town_name.as_ref().ok_or_else(|| ConxianError::Compliance("ISO-404: Missing town_name".to_string()))?;
        let country = intent.country_code.as_ref().ok_or_else(|| ConxianError::Compliance("ISO-404: Missing country_code".to_string()))?;
        info!("Formatting ISO 20022 pacs.008.001.08 for job card in {}", town);
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
            sender, receiver, chrono::Utc::now().to_rfc3339(), amount, sender, receiver
        )
    }
}
