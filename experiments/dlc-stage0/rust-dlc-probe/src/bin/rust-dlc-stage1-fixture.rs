//! Deterministic, enumerated-only Stage 1 DLC contract fixture.
//!
//! This fixture is intentionally below the Gateway integration boundary. It
//! uses fixed test-only keys, synthetic previous transactions, and the pinned
//! low-level `rust-dlc` APIs to construct and serialize a complete local
//! offer/accept/sign plus funding/CET/refund artifact set. It does not persist
//! keys, contact a wallet/oracle/node, or claim manager/runtime readiness.

use std::fmt::Write as _;

use bitcoin::absolute::LockTime;
use bitcoin::consensus::serialize;
use bitcoin::hashes::{sha256, Hash};
use bitcoin::sighash::{EcdsaSighashType, SighashCache};
use bitcoin::transaction::Version;
use bitcoin::{
    Amount, OutPoint, Script, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
};
use dlc::{DlcTransactions, OracleInfo, PartyParams, Payout, TxInputInfo};
use dlc_messages::contract_msgs::{
    ContractDescriptor, ContractInfo, ContractInfoInner, ContractOutcome,
    EnumeratedContractDescriptor, SingleContractInfo,
};
use dlc_messages::oracle_msgs::{
    EnumEventDescriptor, EventDescriptor, OracleAnnouncement, OracleAttestation, OracleEvent,
    SingleOracleInfo,
};
use dlc_messages::{
    AcceptDlc, CetAdaptorSignatures, FundingInput, FundingSignature, FundingSignatures, OfferDlc,
    SignDlc, WitnessElement,
};
use lightning::ln::wire::Type;
use lightning::util::ser::{Readable, Writeable};
use secp256k1_zkp::{
    ecdsa::Signature, schnorr::Signature as SchnorrSignature, EcdsaAdaptorSignature, Keypair,
    Message, PublicKey, Secp256k1, SecretKey, XOnlyPublicKey,
};

const EVENT_ID: &str = "stage1-fixture-enum-event";
const OUTCOME_NO: &str = "no";
const OUTCOME_YES: &str = "yes";
const TEMPORARY_CONTRACT_ID: [u8; 32] = [0x11; 32];
const CHAIN_HASH: [u8; 32] = [
    0x06, 0x22, 0x6e, 0x46, 0x11, 0x1a, 0x0b, 0x59, 0xca, 0xaf, 0x12, 0x60, 0x43, 0xeb, 0x5b, 0xbf,
    0x28, 0xc3, 0x4f, 0x3a, 0x5e, 0x33, 0x2a, 0x1f, 0xc7, 0xb2, 0xb7, 0x3c, 0xf1, 0x88, 0x91, 0x0f,
];
const EVENT_MATURITY: u32 = 100;
const CET_LOCKTIME: u32 = 100;
const REFUND_LOCKTIME: u32 = 200;
const FEE_RATE_PER_VB: u64 = 5;
const FUND_LOCKTIME: u32 = 0;
const FUND_OUTPUT_SERIAL_ID: u64 = 5;
const OFFER_INPUT_SERIAL_ID: u64 = 10;
const ACCEPT_INPUT_SERIAL_ID: u64 = 20;
const OFFER_CHANGE_SERIAL_ID: u64 = 11;
const ACCEPT_CHANGE_SERIAL_ID: u64 = 21;
const OFFER_PAYOUT_SERIAL_ID: u64 = 31;
const ACCEPT_PAYOUT_SERIAL_ID: u64 = 41;
const PARTY_INPUT_AMOUNT_SAT: u64 = 100_000_000;
const PARTY_COLLATERAL_SAT: u64 = 50_000_000;

// Filled from the first deterministic `--emit` run and asserted thereafter.
const EXPECTED_OFFER_MESSAGE_SHA256: &str =
    "9f0d2968dfd08ba10a0cbc19e2cf781661cf7be22890070221c4ec1d7071e0dd";
const EXPECTED_ACCEPT_MESSAGE_SHA256: &str =
    "b73821f2f01c874c527cf3efecb250ea167598ba45017b6a51c5c45ea5f54fd6";
const EXPECTED_SIGN_MESSAGE_SHA256: &str =
    "c1cdd23db825f8adb4955872b55be63697fa971fc3486d1617552f49e570c104";
const EXPECTED_FUNDING_TXID: &str =
    "34d0f8da92837a82ce313ef0edbdba6ed7f123f4c045c95555d83b43f6748a88";
const EXPECTED_OFFER_INPUT_TXID: &str =
    "1f104165c17e18a495b8fb914718d6243c0a57f49a0f52a0c936eb63d385bb37";
const EXPECTED_ACCEPT_INPUT_TXID: &str =
    "56cb847e68bb1d7261e7641614a76a20a2bbc6a0d2958abcae6b45cfbffccab0";
const EXPECTED_CET_TXIDS: [&str; 2] = [
    "3ca9d16d505bc7be6104a0e92a0d4b740e5a1aa7427c5d1427af1a98cd2bec2e",
    "f98808178ba0e6ee43c7d7a20529ee132a2ba2534a7ed29ea84cd27d42889361",
];
const EXPECTED_REFUND_TXID: &str =
    "b18f3b6bf9a2b0652d99028ebd0948ebbc80fc8d4692d706274dd67345bdba44";
const EXPECTED_FINAL_CONTRACT_ID: &str =
    "25c1e9cb83926b93df202fe1fcacab7fc6e032e5d154d84444c92a52e7659b99";
const EXPECTED_CANONICAL_DIGEST: &str =
    "9f8ae3bf3098d69ef6dbf986df3348acc929b4df8b9f02169a300766f6f3443a";

struct OracleFixture {
    announcement: OracleAnnouncement,
    nonce_secret: SecretKey,
    oracle_infos: Vec<OracleInfo>,
    outcome_signatures: Vec<SchnorrSignature>,
}

struct PartyFixture {
    params: PartyParams,
    funding_input: FundingInput,
    secret: SecretKey,
}

struct Fixture {
    oracle: OracleFixture,
    offer_party: PartyFixture,
    accept_party: PartyFixture,
    payouts: Vec<Payout>,
    transactions: DlcTransactions,
    offer: OfferDlc,
    accept: AcceptDlc,
    sign: SignDlc,
    final_contract_id: [u8; 32],
    accept_adaptor_signatures: Vec<EcdsaAdaptorSignature>,
    offer_refund_signature: Signature,
    accept_refund_signature: Signature,
    signed_cets_by_offer: Vec<Transaction>,
    signed_cets_by_accept: Vec<Transaction>,
    signed_refund: Transaction,
    offer_message_bytes: Vec<u8>,
    accept_message_bytes: Vec<u8>,
    sign_message_bytes: Vec<u8>,
}

fn fixed_secret(byte: u8) -> SecretKey {
    SecretKey::from_slice(&[byte; 32]).expect("fixed fixture secret is valid")
}

fn outcome_message(outcome: &str) -> Message {
    let digest = sha256::Hash::hash(outcome.as_bytes());
    Message::from_digest(digest.to_byte_array())
}

fn announcement_message(event: &OracleEvent) -> Result<Message, Box<dyn std::error::Error>> {
    let mut encoded = Vec::new();
    event.write(&mut encoded)?;
    let digest = sha256::Hash::hash(&encoded);
    Ok(Message::from_digest(digest.to_byte_array()))
}

fn sign_oracle_outcome(
    secp: &Secp256k1<secp256k1_zkp::All>,
    oracle_secret: &SecretKey,
    nonce_secret: &SecretKey,
    outcome: &str,
) -> SchnorrSignature {
    let keypair = Keypair::from_secret_key(secp, oracle_secret);
    dlc::secp_utils::schnorrsig_sign_with_nonce(
        secp,
        &outcome_message(outcome),
        &keypair,
        &nonce_secret.secret_bytes(),
    )
}

fn build_oracle_fixture() -> Result<OracleFixture, Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let oracle_secret = fixed_secret(7);
    let nonce_secret = fixed_secret(8);
    let oracle_keypair = Keypair::from_secret_key(&secp, &oracle_secret);
    let nonce_keypair = Keypair::from_secret_key(&secp, &nonce_secret);
    let oracle_public_key = XOnlyPublicKey::from_keypair(&oracle_keypair).0;
    let nonce_public_key = XOnlyPublicKey::from_keypair(&nonce_keypair).0;
    let event = OracleEvent {
        oracle_nonces: vec![nonce_public_key],
        event_maturity_epoch: EVENT_MATURITY,
        event_descriptor: EventDescriptor::EnumEvent(EnumEventDescriptor {
            outcomes: vec![OUTCOME_NO.into(), OUTCOME_YES.into()],
        }),
        event_id: EVENT_ID.into(),
    };
    let announcement = OracleAnnouncement {
        announcement_signature: secp
            .sign_schnorr_no_aux_rand(&announcement_message(&event)?, &oracle_keypair),
        oracle_public_key,
        oracle_event: event,
    };
    let outcome_signatures = vec![
        sign_oracle_outcome(&secp, &oracle_secret, &nonce_secret, OUTCOME_NO),
        sign_oracle_outcome(&secp, &oracle_secret, &nonce_secret, OUTCOME_YES),
    ];

    Ok(OracleFixture {
        announcement,
        nonce_secret,
        oracle_infos: vec![OracleInfo {
            public_key: oracle_public_key,
            nonces: vec![nonce_public_key],
        }],
        outcome_signatures,
    })
}

fn previous_transaction(seed: u8) -> Transaction {
    Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::default(),
            script_sig: ScriptBuf::new(),
            sequence: Sequence(0xffff_ffff),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(PARTY_INPUT_AMOUNT_SAT),
            script_pubkey: ScriptBuf::from_bytes(vec![0x51, seed]),
        }],
    }
}

fn build_party(
    secret_byte: u8,
    previous_tx_seed: u8,
    input_serial_id: u64,
    change_serial_id: u64,
    payout_serial_id: u64,
) -> PartyFixture {
    let secp = Secp256k1::new();
    let secret = fixed_secret(secret_byte);
    let previous_tx = previous_transaction(previous_tx_seed);
    let previous_tx_bytes = serialize(&previous_tx);
    let previous_outpoint = OutPoint {
        txid: previous_tx.compute_txid(),
        vout: 0,
    };
    let redeem_script = ScriptBuf::new();
    let funding_input = FundingInput {
        input_serial_id,
        prev_tx: previous_tx_bytes,
        prev_tx_vout: 0,
        sequence: 0xffff_ffff,
        max_witness_len: dlc::P2WPKH_WITNESS_SIZE as u16,
        redeem_script: redeem_script.clone(),
    };
    let params = PartyParams {
        fund_pubkey: PublicKey::from_secret_key(&secp, &secret),
        change_script_pubkey: ScriptBuf::from_bytes(vec![0x52, secret_byte]),
        change_serial_id,
        payout_script_pubkey: ScriptBuf::from_bytes(vec![0x53, secret_byte]),
        payout_serial_id,
        inputs: vec![TxInputInfo {
            outpoint: previous_outpoint,
            max_witness_len: dlc::P2WPKH_WITNESS_SIZE,
            redeem_script,
            serial_id: input_serial_id,
        }],
        input_amount: Amount::from_sat(PARTY_INPUT_AMOUNT_SAT),
        collateral: Amount::from_sat(PARTY_COLLATERAL_SAT),
    };

    PartyFixture {
        params,
        funding_input,
        secret,
    }
}

fn build_contract_info(oracle: &OracleFixture) -> ContractInfo {
    ContractInfo::SingleContractInfo(SingleContractInfo {
        total_collateral: Amount::from_sat(2 * PARTY_COLLATERAL_SAT),
        contract_info: ContractInfoInner {
            contract_descriptor: ContractDescriptor::EnumeratedContractDescriptor(
                EnumeratedContractDescriptor {
                    payouts: vec![
                        ContractOutcome {
                            outcome: OUTCOME_NO.into(),
                            offer_payout: Amount::from_sat(40_000_000),
                        },
                        ContractOutcome {
                            outcome: OUTCOME_YES.into(),
                            offer_payout: Amount::from_sat(60_000_000),
                        },
                    ],
                },
            ),
            oracle_info: dlc_messages::oracle_msgs::OracleInfo::Single(SingleOracleInfo {
                oracle_announcement: oracle.announcement.clone(),
            }),
        },
    })
}

fn payout_vector() -> Vec<Payout> {
    vec![
        Payout {
            offer: Amount::from_sat(40_000_000),
            accept: Amount::from_sat(60_000_000),
        },
        Payout {
            offer: Amount::from_sat(60_000_000),
            accept: Amount::from_sat(40_000_000),
        },
    ]
}

fn tx_sighash(
    tx: &Transaction,
    script_pubkey: &Script,
    value: Amount,
) -> Result<Message, Box<dyn std::error::Error>> {
    let mut cache = SighashCache::new(tx);
    let sighash = cache.p2wsh_signature_hash(0, script_pubkey, value, EcdsaSighashType::All)?;
    Ok(Message::from_digest_slice(sighash.as_ref())?)
}

fn deterministic_adaptor_signature(
    secp: &Secp256k1<secp256k1_zkp::All>,
    cet: &Transaction,
    oracle_infos: &[OracleInfo],
    funding_secret: &SecretKey,
    funding_script_pubkey: &Script,
    fund_output_value: Amount,
    outcome: &str,
) -> Result<EcdsaAdaptorSignature, Box<dyn std::error::Error>> {
    let messages = vec![vec![outcome_message(outcome)]];
    let adaptor_point = dlc::get_adaptor_point_from_oracle_info(secp, oracle_infos, &messages)?;
    let sighash = tx_sighash(cet, funding_script_pubkey, fund_output_value)?;
    // The pinned `dlc` helper uses random auxiliary data under its default
    // `std` feature. The lower-level pinned API supports a no-auxiliary-rand
    // mode, which is required for a byte-stable local fixture.
    Ok(EcdsaAdaptorSignature::encrypt_no_aux_rand(
        secp,
        &sighash,
        funding_secret,
        &adaptor_point,
    ))
}

fn message_bytes<T: Type + Writeable>(message: &T) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut encoded = Vec::new();
    message.type_id().write(&mut encoded)?;
    message.write(&mut encoded)?;
    Ok(encoded)
}

fn final_contract_id(fund_txid: Txid, fund_output_index: u16, temporary_id: &[u8; 32]) -> [u8; 32] {
    // This is the exact `rust-dlc` manager contract-ID formula, reproduced
    // locally so the fixture remains low-level and manager-independent.
    let fund_txid = fund_txid.to_byte_array();
    let mut result = [0u8; 32];
    for index in 0..32 {
        result[index] = fund_txid[31 - index] ^ temporary_id[index];
    }
    result[30] ^= ((fund_output_index >> 8) & 0xff) as u8;
    result[31] ^= (fund_output_index & 0xff) as u8;
    result
}

fn raw_refund_signature(
    secp: &Secp256k1<secp256k1_zkp::All>,
    refund: &Transaction,
    secret: &SecretKey,
    funding_script_pubkey: &Script,
    fund_output_value: Amount,
) -> Result<Signature, Box<dyn std::error::Error>> {
    Ok(dlc::util::get_raw_sig_for_tx_input(
        secp,
        refund,
        0,
        funding_script_pubkey,
        fund_output_value,
        secret,
    )?)
}

fn funding_witness_elements(
    secp: &Secp256k1<secp256k1_zkp::All>,
    funding: &Transaction,
    party: &PartyFixture,
    funding_script_pubkey: &Script,
) -> Result<Vec<WitnessElement>, Box<dyn std::error::Error>> {
    let signature = dlc::util::get_sig_for_tx_input(
        secp,
        funding,
        0,
        party.params.inputs[0].redeem_script.as_script(),
        party.params.input_amount,
        EcdsaSighashType::All,
        &party.secret,
    )?;
    // Keep the fixture's funding witness concrete while retaining the actual
    // funding script in the transaction artifact and message binding checks.
    let _ = funding_script_pubkey;
    Ok(vec![
        WitnessElement { witness: signature },
        WitnessElement {
            witness: party.params.fund_pubkey.serialize().to_vec(),
        },
    ])
}

fn build_fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let oracle = build_oracle_fixture()?;
    let offer_party = build_party(
        1,
        0xa1,
        OFFER_INPUT_SERIAL_ID,
        OFFER_CHANGE_SERIAL_ID,
        OFFER_PAYOUT_SERIAL_ID,
    );
    let accept_party = build_party(
        2,
        0xb2,
        ACCEPT_INPUT_SERIAL_ID,
        ACCEPT_CHANGE_SERIAL_ID,
        ACCEPT_PAYOUT_SERIAL_ID,
    );
    let payouts = payout_vector();
    let transactions = dlc::create_dlc_transactions(
        &offer_party.params,
        &accept_party.params,
        &payouts,
        REFUND_LOCKTIME,
        FEE_RATE_PER_VB,
        FUND_LOCKTIME,
        CET_LOCKTIME,
        FUND_OUTPUT_SERIAL_ID,
    )?;
    let fund_output_index = transactions.get_fund_output_index();
    let final_contract_id = final_contract_id(
        transactions.fund.compute_txid(),
        fund_output_index as u16,
        &TEMPORARY_CONTRACT_ID,
    );
    let contract_info = build_contract_info(&oracle);
    let offer = OfferDlc {
        protocol_version: 1,
        contract_flags: 0,
        chain_hash: CHAIN_HASH,
        temporary_contract_id: TEMPORARY_CONTRACT_ID,
        contract_info,
        funding_pubkey: offer_party.params.fund_pubkey,
        payout_spk: offer_party.params.payout_script_pubkey.clone(),
        payout_serial_id: offer_party.params.payout_serial_id,
        offer_collateral: offer_party.params.collateral,
        funding_inputs: vec![offer_party.funding_input.clone()],
        change_spk: offer_party.params.change_script_pubkey.clone(),
        change_serial_id: offer_party.params.change_serial_id,
        fund_output_serial_id: FUND_OUTPUT_SERIAL_ID,
        fee_rate_per_vb: FEE_RATE_PER_VB,
        cet_locktime: CET_LOCKTIME,
        refund_locktime: REFUND_LOCKTIME,
    };
    let funding_script = transactions.funding_script_pubkey.as_script();
    let fund_output_value = transactions.get_fund_output().value;
    let mut offer_adaptor_signatures = Vec::with_capacity(payouts.len());
    let mut accept_adaptor_signatures = Vec::with_capacity(payouts.len());
    for (index, outcome) in [OUTCOME_NO, OUTCOME_YES].into_iter().enumerate() {
        let offer_signature = deterministic_adaptor_signature(
            &secp,
            &transactions.cets[index],
            &oracle.oracle_infos,
            &offer_party.secret,
            funding_script,
            fund_output_value,
            outcome,
        )?;
        let accept_signature = deterministic_adaptor_signature(
            &secp,
            &transactions.cets[index],
            &oracle.oracle_infos,
            &accept_party.secret,
            funding_script,
            fund_output_value,
            outcome,
        )?;
        dlc::verify_cet_adaptor_sig_from_oracle_info(
            &secp,
            &offer_signature,
            &transactions.cets[index],
            &oracle.oracle_infos,
            &offer_party.params.fund_pubkey,
            funding_script,
            fund_output_value,
            &[vec![outcome_message(outcome)]],
        )?;
        dlc::verify_cet_adaptor_sig_from_oracle_info(
            &secp,
            &accept_signature,
            &transactions.cets[index],
            &oracle.oracle_infos,
            &accept_party.params.fund_pubkey,
            funding_script,
            fund_output_value,
            &[vec![outcome_message(outcome)]],
        )?;
        offer_adaptor_signatures.push(offer_signature);
        accept_adaptor_signatures.push(accept_signature);
    }

    let offer_refund_signature = raw_refund_signature(
        &secp,
        &transactions.refund,
        &offer_party.secret,
        funding_script,
        fund_output_value,
    )?;
    let accept_refund_signature = raw_refund_signature(
        &secp,
        &transactions.refund,
        &accept_party.secret,
        funding_script,
        fund_output_value,
    )?;
    let accept = AcceptDlc {
        protocol_version: 1,
        temporary_contract_id: TEMPORARY_CONTRACT_ID,
        accept_collateral: accept_party.params.collateral,
        funding_pubkey: accept_party.params.fund_pubkey,
        payout_spk: accept_party.params.payout_script_pubkey.clone(),
        payout_serial_id: accept_party.params.payout_serial_id,
        funding_inputs: vec![accept_party.funding_input.clone()],
        change_spk: accept_party.params.change_script_pubkey.clone(),
        change_serial_id: accept_party.params.change_serial_id,
        cet_adaptor_signatures: CetAdaptorSignatures::from(accept_adaptor_signatures.as_slice()),
        refund_signature: accept_refund_signature,
        negotiation_fields: None,
    };
    let offer_witness_elements =
        funding_witness_elements(&secp, &transactions.fund, &offer_party, funding_script)?;
    let sign = SignDlc {
        protocol_version: 1,
        contract_id: final_contract_id,
        cet_adaptor_signatures: CetAdaptorSignatures::from(offer_adaptor_signatures.as_slice()),
        refund_signature: offer_refund_signature,
        funding_signatures: FundingSignatures {
            funding_signatures: vec![FundingSignature {
                witness_elements: offer_witness_elements,
            }],
        },
    };

    let mut signed_cets_by_offer = Vec::with_capacity(transactions.cets.len());
    let mut signed_cets_by_accept = Vec::with_capacity(transactions.cets.len());
    for (index, cet) in transactions.cets.iter().enumerate() {
        let oracle_signature = vec![vec![oracle.outcome_signatures[index]]];
        let mut offer_signed_cet = cet.clone();
        dlc::sign_cet(
            &secp,
            &mut offer_signed_cet,
            &accept_adaptor_signatures[index],
            &oracle_signature,
            &offer_party.secret,
            &accept_party.params.fund_pubkey,
            funding_script,
            fund_output_value,
        )?;
        let mut accept_signed_cet = cet.clone();
        dlc::sign_cet(
            &secp,
            &mut accept_signed_cet,
            &offer_adaptor_signatures[index],
            &oracle_signature,
            &accept_party.secret,
            &offer_party.params.fund_pubkey,
            funding_script,
            fund_output_value,
        )?;
        signed_cets_by_offer.push(offer_signed_cet);
        signed_cets_by_accept.push(accept_signed_cet);
    }
    let mut signed_refund = transactions.refund.clone();
    dlc::util::sign_multi_sig_input(
        &secp,
        &mut signed_refund,
        &offer_refund_signature,
        &offer_party.params.fund_pubkey,
        &accept_party.secret,
        funding_script,
        fund_output_value,
        0,
    )?;

    let offer_message_bytes = message_bytes(&offer)?;
    let accept_message_bytes = message_bytes(&accept)?;
    let sign_message_bytes = message_bytes(&sign)?;

    Ok(Fixture {
        oracle,
        offer_party,
        accept_party,
        payouts,
        transactions,
        offer,
        accept,
        sign,
        final_contract_id,
        accept_adaptor_signatures,
        offer_refund_signature,
        accept_refund_signature,
        signed_cets_by_offer,
        signed_cets_by_accept,
        signed_refund,
        offer_message_bytes,
        accept_message_bytes,
        sign_message_bytes,
    })
}

fn validate_attestation_binding(
    secp: &Secp256k1<secp256k1_zkp::All>,
    attestation: &OracleAttestation,
    announcement: &OracleAnnouncement,
) -> Result<(), dlc::Error> {
    if attestation.event_id != announcement.oracle_event.event_id {
        return Err(dlc::Error::InvalidArgument);
    }
    attestation.validate(secp, announcement)?;
    match &announcement.oracle_event.event_descriptor {
        EventDescriptor::EnumEvent(descriptor) => {
            if attestation
                .outcomes
                .iter()
                .any(|outcome| !descriptor.outcomes.contains(outcome))
            {
                return Err(dlc::Error::InvalidArgument);
            }
        }
        EventDescriptor::DigitDecompositionEvent(_) => {}
    }
    Ok(())
}

fn oracle_attestation(
    fixture: &Fixture,
    outcome: &str,
    signature: SchnorrSignature,
) -> OracleAttestation {
    OracleAttestation {
        event_id: EVENT_ID.into(),
        oracle_public_key: fixture.oracle.announcement.oracle_public_key,
        signatures: vec![signature],
        outcomes: vec![outcome.into()],
    }
}

fn validate_message_binding(
    fixture: &Fixture,
    offer: &OfferDlc,
    accept: &AcceptDlc,
    sign: &SignDlc,
) -> Result<(), &'static str> {
    if offer.temporary_contract_id != TEMPORARY_CONTRACT_ID {
        return Err("offer temporary contract id mismatch");
    }
    if accept.temporary_contract_id != offer.temporary_contract_id {
        return Err("accept temporary contract id mismatch");
    }
    if sign.contract_id != fixture.final_contract_id {
        return Err("sign final contract id mismatch");
    }
    if offer.contract_info != fixture.offer.contract_info {
        return Err("offer contract descriptor/oracle binding mismatch");
    }
    if offer.funding_inputs != vec![fixture.offer_party.funding_input.clone()] {
        return Err("offer funding input mismatch");
    }
    if accept.funding_inputs != vec![fixture.accept_party.funding_input.clone()] {
        return Err("accept funding input/outpoint mismatch");
    }
    if accept.payout_serial_id != fixture.accept.payout_serial_id {
        return Err("accept payout serial id mismatch");
    }
    if accept.cet_adaptor_signatures.ecdsa_adaptor_signatures.len() != fixture.payouts.len() {
        return Err("accept adaptor signature count mismatch");
    }
    if sign.cet_adaptor_signatures.ecdsa_adaptor_signatures.len() != fixture.payouts.len() {
        return Err("sign adaptor signature count mismatch");
    }
    if sign.funding_signatures.funding_signatures.len() != offer.funding_inputs.len() {
        return Err("sign funding signature count mismatch");
    }
    Ok(())
}

fn validate_refund_artifact(fixture: &Fixture, refund: &Transaction) -> Result<(), &'static str> {
    if refund.lock_time.to_consensus_u32() != REFUND_LOCKTIME {
        return Err("refund locktime mismatch");
    }
    if refund.input.len() != 1
        || refund.input[0].previous_output != fixture.transactions.get_fund_outpoint()
    {
        return Err("refund funding outpoint mismatch");
    }
    let expected_outputs = vec![
        TxOut {
            value: fixture.offer_party.params.collateral,
            script_pubkey: fixture.offer_party.params.payout_script_pubkey.clone(),
        },
        TxOut {
            value: fixture.accept_party.params.collateral,
            script_pubkey: fixture.accept_party.params.payout_script_pubkey.clone(),
        },
    ];
    if refund.output != expected_outputs {
        return Err("refund collateral/output mismatch");
    }
    Ok(())
}

fn artifact_hash(bytes: &[u8]) -> String {
    sha256::Hash::hash(bytes).to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            write!(out, "{byte:02x}").expect("writing to String cannot fail");
            out
        })
}

fn canonical_digest(fixture: &Fixture) -> String {
    let mut bytes = Vec::new();
    let mut add = |label: &str, artifact: &[u8]| {
        bytes.extend_from_slice(label.as_bytes());
        bytes.extend_from_slice(&(artifact.len() as u64).to_be_bytes());
        bytes.extend_from_slice(artifact);
    };
    add("offer", &fixture.offer_message_bytes);
    add("accept", &fixture.accept_message_bytes);
    add("sign", &fixture.sign_message_bytes);
    add("fund", &serialize(&fixture.transactions.fund));
    for (index, cet) in fixture.transactions.cets.iter().enumerate() {
        add(&format!("cet-{index}"), &serialize(cet));
    }
    add("refund", &serialize(&fixture.transactions.refund));
    for (index, cet) in fixture.signed_cets_by_offer.iter().enumerate() {
        add(&format!("signed-cet-offer-{index}"), &serialize(cet));
    }
    for (index, cet) in fixture.signed_cets_by_accept.iter().enumerate() {
        add(&format!("signed-cet-accept-{index}"), &serialize(cet));
    }
    add("signed-refund", &serialize(&fixture.signed_refund));
    artifact_hash(&bytes)
}

fn assert_stable_expectations(fixture: &Fixture) {
    let funding_txid = fixture.transactions.fund.compute_txid().to_string();
    let cet_txids: Vec<_> = fixture
        .transactions
        .cets
        .iter()
        .map(|cet| cet.compute_txid().to_string())
        .collect();
    assert_eq!(
        artifact_hash(&fixture.offer_message_bytes),
        EXPECTED_OFFER_MESSAGE_SHA256
    );
    assert_eq!(
        artifact_hash(&fixture.accept_message_bytes),
        EXPECTED_ACCEPT_MESSAGE_SHA256
    );
    assert_eq!(
        artifact_hash(&fixture.sign_message_bytes),
        EXPECTED_SIGN_MESSAGE_SHA256
    );
    assert_eq!(funding_txid, EXPECTED_FUNDING_TXID);
    assert_eq!(
        fixture.offer_party.params.inputs[0]
            .outpoint
            .txid
            .to_string(),
        EXPECTED_OFFER_INPUT_TXID
    );
    assert_eq!(
        fixture.accept_party.params.inputs[0]
            .outpoint
            .txid
            .to_string(),
        EXPECTED_ACCEPT_INPUT_TXID
    );
    assert_eq!(cet_txids.as_slice(), EXPECTED_CET_TXIDS);
    assert_eq!(
        fixture.transactions.refund.compute_txid().to_string(),
        EXPECTED_REFUND_TXID
    );
    assert_eq!(hex(&fixture.final_contract_id), EXPECTED_FINAL_CONTRACT_ID);
    assert_eq!(canonical_digest(fixture), EXPECTED_CANONICAL_DIGEST);
}

fn run_positive_checks(fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    fixture.offer.validate(&secp, 1, 1_000)?;
    validate_message_binding(fixture, &fixture.offer, &fixture.accept, &fixture.sign)
        .map_err(|error| error.to_string())?;
    validate_refund_artifact(fixture, &fixture.transactions.refund)
        .map_err(|error| error.to_string())?;

    assert_eq!(fixture.transactions.cets.len(), 2);
    assert_eq!(fixture.transactions.get_fund_output_index(), 0);
    assert_eq!(
        fixture.transactions.fund.input[0].previous_output,
        fixture.offer_party.params.inputs[0].outpoint
    );
    assert_eq!(
        fixture.transactions.fund.input[1].previous_output,
        fixture.accept_party.params.inputs[0].outpoint
    );
    assert_eq!(
        fixture.transactions.cets[0].lock_time.to_consensus_u32(),
        CET_LOCKTIME
    );
    assert_eq!(
        fixture.transactions.cets[1].lock_time.to_consensus_u32(),
        CET_LOCKTIME
    );
    assert_eq!(
        fixture.transactions.refund.lock_time.to_consensus_u32(),
        REFUND_LOCKTIME
    );

    let total_collateral =
        fixture.offer_party.params.collateral + fixture.accept_party.params.collateral;
    for payout in &fixture.payouts {
        assert_eq!(payout.offer + payout.accept, total_collateral);
    }
    assert_eq!(
        fixture
            .transactions
            .refund
            .output
            .iter()
            .map(|output| output.value)
            .sum::<Amount>(),
        total_collateral
    );
    assert!(fixture.transactions.get_fund_output().value >= total_collateral);
    assert_eq!(fixture.transactions.cets[0].output.len(), 2);
    assert_eq!(fixture.transactions.cets[1].output.len(), 2);
    assert_eq!(
        fixture.transactions.cets[0].output[0].script_pubkey,
        fixture.offer_party.params.payout_script_pubkey
    );
    assert_eq!(
        fixture.transactions.cets[0].output[1].script_pubkey,
        fixture.accept_party.params.payout_script_pubkey
    );

    let cet_input = fixture.transactions.cets[0].input[0].clone();
    let direct_cets = dlc::create_cets(
        &cet_input,
        fixture.offer_party.params.payout_script_pubkey.as_script(),
        fixture.offer_party.params.payout_serial_id,
        fixture.accept_party.params.payout_script_pubkey.as_script(),
        fixture.accept_party.params.payout_serial_id,
        &fixture.payouts,
        CET_LOCKTIME,
    );
    assert_eq!(direct_cets, fixture.transactions.cets);
    let direct_refund = dlc::create_refund_transaction(
        TxOut {
            value: fixture.offer_party.params.collateral,
            script_pubkey: fixture.offer_party.params.payout_script_pubkey.clone(),
        },
        TxOut {
            value: fixture.accept_party.params.collateral,
            script_pubkey: fixture.accept_party.params.payout_script_pubkey.clone(),
        },
        fixture.transactions.refund.input[0].clone(),
        REFUND_LOCKTIME,
    );
    assert_eq!(direct_refund, fixture.transactions.refund);

    assert_eq!(
        fixture.signed_cets_by_offer.len(),
        fixture.transactions.cets.len()
    );
    assert_eq!(
        fixture.signed_cets_by_accept.len(),
        fixture.transactions.cets.len()
    );
    assert!(fixture
        .signed_cets_by_offer
        .iter()
        .all(|cet| !cet.input[0].witness.is_empty()));
    assert!(fixture
        .signed_cets_by_accept
        .iter()
        .all(|cet| !cet.input[0].witness.is_empty()));
    assert!(!fixture.signed_refund.input[0].witness.is_empty());

    dlc::verify_tx_input_sig(
        &secp,
        &fixture.offer_refund_signature,
        &fixture.transactions.refund,
        0,
        fixture.transactions.funding_script_pubkey.as_script(),
        fixture.transactions.get_fund_output().value,
        &fixture.offer_party.params.fund_pubkey,
    )?;
    dlc::verify_tx_input_sig(
        &secp,
        &fixture.accept_refund_signature,
        &fixture.transactions.refund,
        0,
        fixture.transactions.funding_script_pubkey.as_script(),
        fixture.transactions.get_fund_output().value,
        &fixture.accept_party.params.fund_pubkey,
    )?;

    let yes_attestation =
        oracle_attestation(fixture, OUTCOME_YES, fixture.oracle.outcome_signatures[1]);
    validate_attestation_binding(&secp, &yes_attestation, &fixture.oracle.announcement)?;
    Ok(())
}

fn run_rejection_checks(fixture: &Fixture) -> Result<usize, Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let mut changed_temporary_id = fixture.accept.clone();
    changed_temporary_id.temporary_contract_id[0] ^= 1;
    assert!(validate_message_binding(
        fixture,
        &fixture.offer,
        &changed_temporary_id,
        &fixture.sign
    )
    .is_err());

    let mut changed_final_id = fixture.sign.clone();
    changed_final_id.contract_id[0] ^= 1;
    assert!(
        validate_message_binding(fixture, &fixture.offer, &fixture.accept, &changed_final_id)
            .is_err()
    );

    let mut changed_funding_outpoint = fixture.accept.clone();
    changed_funding_outpoint.funding_inputs[0].prev_tx_vout = 1;
    assert!(validate_message_binding(
        fixture,
        &fixture.offer,
        &changed_funding_outpoint,
        &fixture.sign
    )
    .is_err());

    let mut changed_payout = fixture.offer.clone();
    if let ContractInfo::SingleContractInfo(single) = &mut changed_payout.contract_info {
        if let ContractDescriptor::EnumeratedContractDescriptor(descriptor) =
            &mut single.contract_info.contract_descriptor
        {
            descriptor.payouts[0].offer_payout = Amount::from_sat(40_000_001);
        }
    }
    assert!(
        validate_message_binding(fixture, &changed_payout, &fixture.accept, &fixture.sign).is_err()
    );

    let mut changed_serial_id = fixture.accept.clone();
    changed_serial_id.payout_serial_id += 1;
    assert!(
        validate_message_binding(fixture, &fixture.offer, &changed_serial_id, &fixture.sign)
            .is_err()
    );

    let mut wrong_outcome =
        oracle_attestation(fixture, OUTCOME_NO, fixture.oracle.outcome_signatures[1]);
    assert!(
        validate_attestation_binding(&secp, &wrong_outcome, &fixture.oracle.announcement).is_err()
    );

    let alternate_secret = fixed_secret(9);
    wrong_outcome.signatures[0] = sign_oracle_outcome(
        &secp,
        &alternate_secret,
        &fixture.oracle.nonce_secret,
        OUTCOME_YES,
    );
    wrong_outcome.outcomes[0] = OUTCOME_YES.into();
    assert!(
        validate_attestation_binding(&secp, &wrong_outcome, &fixture.oracle.announcement).is_err()
    );

    let wrong_adaptor_outcome = vec![vec![outcome_message("maybe")]];
    assert!(dlc::verify_cet_adaptor_sig_from_oracle_info(
        &secp,
        &fixture.accept_adaptor_signatures[0],
        &fixture.transactions.cets[0],
        &fixture.oracle.oracle_infos,
        &fixture.accept_party.params.fund_pubkey,
        fixture.transactions.funding_script_pubkey.as_script(),
        fixture.transactions.get_fund_output().value,
        &wrong_adaptor_outcome,
    )
    .is_err());

    let mut incomplete_accept = fixture.accept.clone();
    incomplete_accept
        .cet_adaptor_signatures
        .ecdsa_adaptor_signatures
        .clear();
    assert!(
        validate_message_binding(fixture, &fixture.offer, &incomplete_accept, &fixture.sign)
            .is_err()
    );

    let mut incomplete_sign = fixture.sign.clone();
    incomplete_sign
        .funding_signatures
        .funding_signatures
        .clear();
    assert!(
        validate_message_binding(fixture, &fixture.offer, &fixture.accept, &incomplete_sign)
            .is_err()
    );

    let truncated_accept = &fixture.accept_message_bytes[..fixture.accept_message_bytes.len() - 1];
    let mut cursor = lightning::io::Cursor::new(&truncated_accept[2..]);
    assert!(AcceptDlc::read(&mut cursor).is_err());

    let mut bad_refund_locktime = fixture.transactions.refund.clone();
    bad_refund_locktime.lock_time = LockTime::from_consensus(REFUND_LOCKTIME - 1);
    assert!(validate_refund_artifact(fixture, &bad_refund_locktime).is_err());

    let mut bad_refund_collateral = fixture.transactions.refund.clone();
    bad_refund_collateral.output[0].value += Amount::from_sat(1);
    assert!(validate_refund_artifact(fixture, &bad_refund_collateral).is_err());

    Ok(13)
}

fn print_fixture(fixture: &Fixture) {
    let txid = |tx: &Transaction| tx.compute_txid().to_string();
    println!("fixture=stage1-enumerated-single-oracle-two-outcome");
    println!("offer_message_len={}", fixture.offer_message_bytes.len());
    println!(
        "offer_message_sha256={}",
        artifact_hash(&fixture.offer_message_bytes)
    );
    println!("accept_message_len={}", fixture.accept_message_bytes.len());
    println!(
        "accept_message_sha256={}",
        artifact_hash(&fixture.accept_message_bytes)
    );
    println!("sign_message_len={}", fixture.sign_message_bytes.len());
    println!(
        "sign_message_sha256={}",
        artifact_hash(&fixture.sign_message_bytes)
    );
    println!("funding_txid={}", txid(&fixture.transactions.fund));
    for (index, cet) in fixture.transactions.cets.iter().enumerate() {
        println!("cet_{index}_txid={}", txid(cet));
    }
    println!("refund_txid={}", txid(&fixture.transactions.refund));
    println!(
        "offer_input_outpoint={}",
        fixture.offer_party.params.inputs[0].outpoint
    );
    println!(
        "accept_input_outpoint={}",
        fixture.accept_party.params.inputs[0].outpoint
    );
    println!("final_contract_id={}", hex(&fixture.final_contract_id));
    println!(
        "funding_output_index={}",
        fixture.transactions.get_fund_output_index()
    );
    println!("cet_count={}", fixture.transactions.cets.len());
    println!("canonical_digest={}", canonical_digest(fixture));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = build_fixture()?;
    run_positive_checks(&fixture)?;
    let rejection_count = run_rejection_checks(&fixture)?;
    if std::env::args().any(|arg| arg == "--emit") {
        print_fixture(&fixture);
        return Ok(());
    }
    assert_stable_expectations(&fixture);
    println!("stage1_fixture=passed");
    println!(
        "positive_artifacts=offer,accept,sign,funding,cet[2],refund,signed_cet[2],signed_refund"
    );
    println!("rejection_cases={rejection_count}");
    println!("numeric_or_hyperbola_support=false");
    println!("gateway_runtime_or_custody_integration=false");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_fixture_matches_artifact_expectations() {
        let fixture = build_fixture().unwrap();
        run_positive_checks(&fixture).unwrap();
        assert_stable_expectations(&fixture);
    }

    #[test]
    fn deterministic_fixture_rejections_fail_closed() {
        let fixture = build_fixture().unwrap();
        assert_eq!(run_rejection_checks(&fixture).unwrap(), 13);
    }
}
