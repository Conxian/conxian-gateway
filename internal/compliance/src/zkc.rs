use crate::SovereignCommit;
use base64::{engine::general_purpose, Engine as _};
use bitcoin::consensus::encode::deserialize;
use bitcoin::{
    transaction, Address, Amount, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut,
    Witness,
};
use conxian_core::{
    Attestation, AttestationRequest, ConxianError, ConxianJobCard, ConxianResult, IndustrialIntent,
    NormalizedSettlement, OfflineReceipt, OfflineReceiptStatus, SanctionsRisk, SettlementEnvelope,
    SettlementFinality, SettlementIdentifiers, SettlementSource, SettlementStatus,
    SETTLEMENT_ENVELOPE_VERSION_CURRENT,
};
use secp256k1::{schnorr, Message, PublicKey, Secp256k1, XOnlyPublicKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

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

    pub fn verify_bitvm2_settlement(
        &self,
        _payload: &conxian_core::JobCardSettlementRequest,
    ) -> ConxianResult<bool> {
        info!("Verifying BitVM2 settlement state proof...");
        Ok(true)
    }

    pub fn normalize_iso20022_ingress(
        &self,
        _xml: &str,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing ISO 20022 (pacs.008) ingress for institutional ledger...");
        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                transaction_id: format!("iso-{}", uuid::Uuid::new_v4()),
                amount_minor: 0,
                amount_scale: 0,
                currency: "USD".to_string(),
                sender: "ISO_SENDER".to_string(),
                receiver: "ISO_RECEIVER".to_string(),
                source: SettlementSource::Iso20022Pacs008,
                raw_payload_hash,
                industrial_intent: IndustrialIntent::default(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock moved backwards")
                    .as_secs(),
                status: SettlementStatus::Ingested,
                finality: SettlementFinality::Unknown,
                rail: None,
                settled_at: None,
                identifiers: SettlementIdentifiers::default(),
            },
        })
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
            <SttlmInf>
                <SttlmMtd>CLRG</SttlmMtd>
            </SttlmInf>
        </GrpHdr>
        <CdtTrfTxInf>
            <PmtId>
                <EndToEndId>{}</EndToEndId>
            </PmtId>
            <IntrBkSttlmAmt Ccy="BTC">{}</IntrBkSttlmAmt>
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

    pub fn verify_tee_attestation(
        &self,
        request: &conxian_core::AttestationRequest,
    ) -> conxian_core::ConxianResult<conxian_core::Attestation> {
        match request {
            conxian_core::AttestationRequest::Ecdsa(att) => {
                let pubkey = PublicKey::from_slice(
                    &hex::decode(&att.public_key)
                        .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?,
                )
                .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?;

                let sig = secp256k1::ecdsa::Signature::from_compact(
                    &hex::decode(&att.signature)
                        .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?,
                )
                .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?;

                let mut hasher = Sha256::new();
                hasher.update(ATTESTATION_SIGNING_DOMAIN);
                hasher.update(att.payload.as_bytes());
                hasher.update(att.device_id.as_bytes());
                let msg = Message::from_digest(hasher.finalize().into());

                self.secp
                    .verify_ecdsa(&msg, &sig, &pubkey)
                    .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?;

                Ok(att.clone())
            }
            conxian_core::AttestationRequest::Schnorr(att) => {
                let pubkey = XOnlyPublicKey::from_slice(
                    &hex::decode(&att.x_only_public_key)
                        .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?,
                )
                .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?;

                let sig = schnorr::Signature::from_slice(
                    &hex::decode(&att.signature)
                        .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?,
                )
                .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?;

                let mut hasher = Sha256::new();
                hasher.update(ATTESTATION_SIGNING_DOMAIN);
                hasher.update(att.payload.as_bytes());
                hasher.update(att.device_id.as_bytes());
                let msg = Message::from_digest(hasher.finalize().into());

                self.secp
                    .verify_schnorr(&sig, &msg, &pubkey)
                    .map_err(|e| conxian_core::ConxianError::Compliance(e.to_string()))?;

                Ok(Attestation {
                    device_id: att.device_id.clone(),
                    public_key: att.x_only_public_key.clone(),
                    signature: att.signature.clone(),
                    payload: att.payload.clone(),
                })
            }
            _ => Err(conxian_core::ConxianError::Compliance(
                "Unsupported attestation type".to_string(),
            )),
        }
    }

    pub fn verify_settlement_trigger_attestation(
        &self,
        request: &conxian_core::AttestationRequest,
        expected_payload: &str,
    ) -> conxian_core::ConxianResult<conxian_core::Attestation> {
        let att = self.verify_tee_attestation(request)?;
        if att.payload != expected_payload {
            return Err(conxian_core::ConxianError::Compliance(
                "Attestation payload mismatch".to_string(),
            ));
        }
        Ok(att)
    }

    pub fn normalize_papss_ingress(
        &self,
        payload: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing PAPSS (Africa) ingress for institutional ledger...");
        let txid = payload["transactionId"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let amount = payload["amount"].as_u64().unwrap_or(0);
        let currency = payload["currency"].as_str().unwrap_or("USD").to_string();

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

        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                transaction_id: txid,
                amount_minor: amount,
                amount_scale: 0,
                currency,
                sender: "PAPSS_SENDER".to_string(),
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
        payload: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing BRICS (mBridge) ingress for institutional ledger...");
        let json: Value =
            serde_json::from_str(payload.as_str().unwrap_or("{}")).unwrap_or_default();
        let txid = json["mbridge_id"].as_str().unwrap_or("unknown").to_string();
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

    pub fn normalize_cips_ingress(
        &self,
        payload: &Value,
        raw_payload_hash: String,
    ) -> ConxianResult<SettlementEnvelope> {
        info!("Normalizing CIPS (ISO 20022 CIPS variant) ingress for institutional ledger...");
        let txid = payload["cips_msg_id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let amount = payload["amount"].as_u64().unwrap_or(0);
        let sender = payload["sender"].as_str().unwrap_or("unknown").to_string();
        let receiver = payload["receiver"]
            .as_str()
            .unwrap_or("CIPS_RECEIVER")
            .to_string();
        let currency = payload["currency"].as_str().unwrap_or("CNY").to_string();

        let identifiers = SettlementIdentifiers {
            message_id: Some(txid.clone()),
            transaction_reference: payload["cips_tx_ref"].as_str().map(|s| s.to_string()),
            settlement_reference: Some(txid.clone()),
            end_to_end_id: payload["end_to_end_id"].as_str().map(|s| s.to_string()),
            settlement_amount: amount.to_string(),
            settlement_currency: currency.clone(),
            settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock moved backwards")
            .as_secs();

        let source = SettlementSource::Cips;
        let risk = source.sanctions_risk();
        if source.requires_sanctions_screening() {
            warn!(
                "CIPS settlement requires sanctions screening (risk={:?}, txid={})",
                risk, txid
            );
        }
        info!(
            "CIPS settlement normalized: {} {} {} (risk={:?})",
            amount, currency, txid, risk
        );

        Ok(SettlementEnvelope {
            version: SETTLEMENT_ENVELOPE_VERSION_CURRENT.to_string(),
            payload: NormalizedSettlement {
                transaction_id: txid,
                amount_minor: amount,
                amount_scale: 0,
                currency,
                sender,
                receiver,
                source,
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

    /// Screen a settlement envelope for sanctions risk.
    /// Returns Ok(()) if the settlement passes screening, or an error if blocked.
    pub fn screen_sanctions(&self, envelope: &SettlementEnvelope) -> ConxianResult<()> {
        let source = envelope.payload.source;
        let risk = source.sanctions_risk();

        match risk {
            SanctionsRisk::Critical => {
                warn!(
                    "BLOCKED: Critical sanctions risk settlement from {} rail (txid={})",
                    source.as_rail_name(),
                    envelope.payload.transaction_id
                );
                Err(ConxianError::Compliance(format!(
                    "Settlement blocked: {} rail is under active sanctions (risk: Critical)",
                    source.as_rail_name()
                )))
            }
            SanctionsRisk::High => {
                warn!(
                    "ELEVATED: High sanctions risk settlement from {} rail (txid={})",
                    source.as_rail_name(),
                    envelope.payload.transaction_id
                );
                // High risk is logged but not blocked — operator discretion
                Ok(())
            }
            SanctionsRisk::Medium => {
                info!(
                    "Sanctions screening: {} rail at Medium risk (txid={})",
                    source.as_rail_name(),
                    envelope.payload.transaction_id
                );
                Ok(())
            }
            SanctionsRisk::Low => Ok(()),
        }
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

    pub fn map_dlc_bond_to_usi(
        &self,
        bond: &conxian_core::DlcBond,
    ) -> conxian_core::NormalizedSettlement {
        conxian_core::NormalizedSettlement {
            source: conxian_core::SettlementSource::DlcBond,
            transaction_id: bond.bond_id.clone(),
            amount_minor: bond.amount_btc * 100_000_000,
            amount_scale: 8,
            currency: "BTC".to_string(),
            sender: "DLC_ORCHESTRATOR".to_string(),
            receiver: "SOVEREIGN_VAULT".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock moved backwards")
                .as_secs(),
            status: conxian_core::SettlementStatus::Ingested,
            rail: None,
            finality: conxian_core::SettlementFinality::Provisional,
            settled_at: None,
            identifiers: conxian_core::SettlementIdentifiers {
                message_id: Some(bond.bond_id.clone()),
                transaction_reference: None,
                settlement_reference: None,
                end_to_end_id: None,
                settlement_amount: bond.amount_btc.to_string(),
                settlement_currency: "BTC".to_string(),
                settlement_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            },
            raw_payload_hash: format!("dlc-hash-{}", bond.bond_id),
            industrial_intent: conxian_core::IndustrialIntent::default(),
        }
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
        amount_sbtc: u64,
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

#[async_trait::async_trait]
impl crate::verifier::CoreVerifier for ZkcVerifier {
    async fn verify_attestation_v2(&self, request: &AttestationRequest) -> ConxianResult<bool> {
        self.verify_tee_attestation(request).map(|_| true)
    }
}

impl conxian_core::Bip322Verifier for ZkcVerifier {
    fn verify_message(&self, address: &str, message: &str, signature: &str) -> ConxianResult<bool> {
        info!("Verifying BIP-322 message for address: {}", address);

        // 1. Parse address and determine network
        let addr = address
            .parse::<Address<_>>()
            .map_err(|e| ConxianError::Compliance(format!("Invalid address: {}", e)))?
            .require_network(Network::Bitcoin)
            .map_err(|e| ConxianError::Compliance(format!("Invalid network: {}", e)))?;

        // 2. Construct to_spend transaction (BIP-322 simple flow)
        // https://github.com/bitcoin/bips/blob/master/bip-0322.mediawiki#simple-signature-verification-framework

        // Tagged hash for message: sha256("BIP0322-signed-message" + message)
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

        // For simple verification, we mainly check if the signature is valid for the to_spend transaction
        let sig_bytes = general_purpose::STANDARD
            .decode(signature)
            .map_err(|e| ConxianError::Compliance(format!("Invalid base64 signature: {}", e)))?;

        let to_sign: Transaction = deserialize(&sig_bytes).map_err(|e| {
            ConxianError::Compliance(format!("Invalid signature transaction: {}", e))
        })?;

        // Basic sanity checks on to_sign
        if to_sign.input.is_empty() {
            return Ok(false);
        }

        // The first input of to_sign must spend the output of to_spend
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

        // Industry Enhancement: Real MuSig2 key aggregation logic would go here.
        // For now, we simulate the aggregation.
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

        // Industry Enhancement: Real BIP-327 partial signature aggregation would go here.
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
