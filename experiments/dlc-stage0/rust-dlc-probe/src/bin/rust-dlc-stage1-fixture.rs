//! Deterministic, enumerated-only Stage 1 DLC contract fixture.
//!
//! This fixture is intentionally below the Gateway integration boundary. It
//! uses fixed test-only keys, coherent native-P2WPKH synthetic previous
//! transactions, and the pinned low-level `rust-dlc` APIs to construct and serialize a complete local
//! offer/accept/sign plus funding/CET/refund artifact set. It does not persist
//! keys, contact a wallet/oracle/node, or claim manager/runtime readiness.

use std::fmt::{self, Write as _};

use bitcoin::absolute::LockTime;
use bitcoin::consensus::{serialize, Decodable};
use bitcoin::hashes::{sha256, Hash};
use bitcoin::sighash::{EcdsaSighashType, SighashCache};
use bitcoin::transaction::Version;
use bitcoin::{
    Address, Amount, CompressedPublicKey, Network, OutPoint, Script, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut, Txid, Witness,
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
    "02429a798cf33c6a15bb8cd738c55ad6d581303fbd2370a2d93c79d1c36b1e4c";
const EXPECTED_ACCEPT_MESSAGE_SHA256: &str =
    "2ecb9087438980c0dc8749bce7da9b34be644038fbce20af667da92c2d00984a";
const EXPECTED_SIGN_MESSAGE_SHA256: &str =
    "2d4bfb0d31aafc3aa57693f58830dd008f8637891dbd3ba625067dfbc72d6c91";
const EXPECTED_FUNDING_TXID: &str =
    "f4f0d66c02a0491307f545692d7cbeef9aca095b43a63191bddcaebca08a3334";
const EXPECTED_OFFER_INPUT_TXID: &str =
    "3e2b1dad8e66e6cba1e762711786a9ee2d9e96dc890b87251eee22821781e69e";
const EXPECTED_ACCEPT_INPUT_TXID: &str =
    "5f420a1f4b9b7e5f9b39c8d1c54a8aa7ba651cd32030792a78bb2417bd0d9de0";
const EXPECTED_CET_TXIDS: [&str; 2] = [
    "7f724daedb20461ac379dd0784eaad7acbc11099818b2162a94cbb3b756e2a97",
    "6d3042cc2050c7fc91889c1e34efd940c15f04875e56323296242178ef499ff1",
];
const EXPECTED_REFUND_TXID: &str =
    "43d205a919923fb600c96e777f65e234cfaee8a84b1d48b1d1dc695b82762199";
const EXPECTED_FINAL_CONTRACT_ID: &str =
    "e5e1c77d13b1580216e454783c6daffe8bdb184a52b72080accdbfadb19b2225";
const EXPECTED_CANONICAL_DIGEST: &str =
    "bf13afe7352577f1cd3e28ca92098cd247e5e607948ed1283b1fd9e66ead1f40";

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
    previous_output: TxOut,
    funding_script_pubkey: ScriptBuf,
    funding_script_code: ScriptBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixtureValidationError {
    ContractId,
    ContractInfo,
    ChainHash,
    Collateral,
    FundingPubkey,
    PayoutScript,
    PayoutSerialId,
    ChangeScript,
    ChangeSerialId,
    FundOutputSerialId,
    FeeRate,
    Locktime,
    FundingInputCount,
    FundingInputPrevTx,
    FundingInputOutpoint,
    FundingInputMetadata,
    FundingInputValue,
    FundingInputScript,
    SignatureCardinality,
    SignatureContents,
    FundingWitness,
    CetWitness,
    RefundWitness,
    RefundOutpoint,
    RefundOutputs,
    MessageTypeId,
    MessageRoundTrip,
    ProtocolVersion,
}

impl fmt::Display for FixtureValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FixtureValidationError {}

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
    offer_adaptor_signatures: Vec<EcdsaAdaptorSignature>,
    accept_adaptor_signatures: Vec<EcdsaAdaptorSignature>,
    offer_funding_witness_elements: Vec<WitnessElement>,
    accept_funding_witness_elements: Vec<WitnessElement>,
    offer_refund_signature: Signature,
    accept_refund_signature: Signature,
    signed_funding: Transaction,
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

fn previous_transaction(seed: u8, script_pubkey: ScriptBuf) -> Transaction {
    Transaction {
        version: Version::TWO,
        // Keep the two synthetic transactions distinct without changing the
        // referenced output's value or spending semantics.
        lock_time: LockTime::from_consensus(u32::from(seed)),
        input: vec![TxIn {
            previous_output: OutPoint::default(),
            script_sig: ScriptBuf::new(),
            sequence: Sequence(0xffff_ffff),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(PARTY_INPUT_AMOUNT_SAT),
            script_pubkey,
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
    let fund_pubkey = PublicKey::from_secret_key(&secp, &secret);
    let compressed_fund_pubkey = CompressedPublicKey::from_slice(&fund_pubkey.serialize())
        .expect("fixture funding key must be a compressed public key");
    let funding_script_pubkey = ScriptBuf::new_p2wpkh(&compressed_fund_pubkey.wpubkey_hash());
    let funding_script_code = compressed_fund_pubkey.p2wpkh_script_code();
    let previous_tx = previous_transaction(previous_tx_seed, funding_script_pubkey.clone());
    let previous_tx_bytes = serialize(&previous_tx);
    let previous_outpoint = OutPoint {
        txid: previous_tx.compute_txid(),
        vout: 0,
    };
    let previous_output = previous_tx.output[0].clone();
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
        fund_pubkey,
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
        previous_output,
        funding_script_pubkey,
        funding_script_code,
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
    input_index: usize,
    party: &PartyFixture,
) -> Result<Vec<WitnessElement>, Box<dyn std::error::Error>> {
    let (previous_tx, previous_output) = decode_funding_prevout(&party.funding_input)?;
    let expected_outpoint = OutPoint {
        txid: previous_tx.compute_txid(),
        vout: party.funding_input.prev_tx_vout,
    };
    if funding
        .input
        .get(input_index)
        .map(|input| input.previous_output)
        != Some(expected_outpoint)
    {
        return Err(FixtureValidationError::FundingInputOutpoint.into());
    }
    if previous_output != party.previous_output {
        return Err(FixtureValidationError::FundingInputValue.into());
    }
    let signature = dlc::util::get_sig_for_tx_input(
        secp,
        funding,
        input_index,
        party.funding_script_code.as_script(),
        previous_output.value,
        EcdsaSighashType::All,
        &party.secret,
    )?;
    let witness = Witness::from_slice(&[signature, party.params.fund_pubkey.serialize().to_vec()]);
    validate_funding_witness_stack(secp, funding, input_index, party, &witness)
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
    Ok(witness
        .to_vec()
        .into_iter()
        .map(|witness| WitnessElement { witness })
        .collect())
}

fn decode_funding_prevout(
    funding_input: &FundingInput,
) -> Result<(Transaction, TxOut), FixtureValidationError> {
    let mut encoded = funding_input.prev_tx.as_slice();
    let previous_tx = Transaction::consensus_decode(&mut encoded)
        .map_err(|_| FixtureValidationError::FundingInputPrevTx)?;
    if !encoded.is_empty() {
        return Err(FixtureValidationError::FundingInputPrevTx);
    }
    let previous_output = previous_tx
        .output
        .get(funding_input.prev_tx_vout as usize)
        .cloned()
        .ok_or(FixtureValidationError::FundingInputOutpoint)?;
    Ok((previous_tx, previous_output))
}

fn validate_prevout_script(
    previous_output: &TxOut,
    party: &PartyFixture,
) -> Result<(), FixtureValidationError> {
    if !previous_output.script_pubkey.is_p2wpkh()
        || previous_output.script_pubkey != party.funding_script_pubkey
    {
        return Err(FixtureValidationError::FundingInputScript);
    }
    let compressed_fund_pubkey =
        CompressedPublicKey::from_slice(&party.params.fund_pubkey.serialize())
            .map_err(|_| FixtureValidationError::FundingInputScript)?;
    if party.funding_script_code != compressed_fund_pubkey.p2wpkh_script_code() {
        return Err(FixtureValidationError::FundingInputScript);
    }
    let expected_address = Address::p2wpkh(&compressed_fund_pubkey, Network::Regtest);
    let actual_address = Address::from_script(&previous_output.script_pubkey, Network::Regtest)
        .map_err(|_| FixtureValidationError::FundingInputScript)?;
    if actual_address != expected_address
        || actual_address.script_pubkey() != previous_output.script_pubkey
    {
        return Err(FixtureValidationError::FundingInputScript);
    }
    Ok(())
}

fn validate_funding_input_binding(
    message_input: &FundingInput,
    params_input: &TxInputInfo,
    funding_tx_input: &TxIn,
    party: &PartyFixture,
) -> Result<(), FixtureValidationError> {
    let (previous_tx, previous_output) = decode_funding_prevout(message_input)?;
    let parsed_outpoint = OutPoint {
        txid: previous_tx.compute_txid(),
        vout: message_input.prev_tx_vout,
    };
    if serialize(&previous_tx) != message_input.prev_tx {
        return Err(FixtureValidationError::FundingInputPrevTx);
    }
    if parsed_outpoint != params_input.outpoint
        || message_input.prev_tx_vout != params_input.outpoint.vout
    {
        return Err(FixtureValidationError::FundingInputOutpoint);
    }
    if message_input.input_serial_id != params_input.serial_id
        || message_input.max_witness_len as usize != params_input.max_witness_len
        || message_input.redeem_script != params_input.redeem_script
        || message_input.redeem_script != party.funding_input.redeem_script
    {
        return Err(FixtureValidationError::FundingInputMetadata);
    }
    if message_input.sequence != party.funding_input.sequence
        || funding_tx_input.sequence.to_consensus_u32() != message_input.sequence
    {
        return Err(FixtureValidationError::FundingInputMetadata);
    }
    if funding_tx_input.previous_output != parsed_outpoint
        || !funding_tx_input.script_sig.is_empty()
    {
        return Err(FixtureValidationError::FundingInputOutpoint);
    }
    if previous_output.value != party.params.input_amount
        || previous_output.value != party.previous_output.value
    {
        return Err(FixtureValidationError::FundingInputValue);
    }
    validate_prevout_script(&previous_output, party)?;
    Ok(())
}

fn parse_finalized_signature(
    bytes: &[u8],
    error: FixtureValidationError,
) -> Result<Signature, FixtureValidationError> {
    if bytes.len() < 2 || bytes.last().copied() != Some(EcdsaSighashType::All.to_u32() as u8) {
        return Err(error);
    }
    Signature::from_der(&bytes[..bytes.len() - 1]).map_err(|_| error)
}

fn validate_funding_witness_stack(
    secp: &Secp256k1<secp256k1_zkp::All>,
    funding: &Transaction,
    input_index: usize,
    party: &PartyFixture,
    witness: &Witness,
) -> Result<(), FixtureValidationError> {
    let input = funding
        .input
        .get(input_index)
        .ok_or(FixtureValidationError::FundingWitness)?;
    let (previous_tx, previous_output) = decode_funding_prevout(&party.funding_input)?;
    if input.previous_output
        != (OutPoint {
            txid: previous_tx.compute_txid(),
            vout: party.funding_input.prev_tx_vout,
        })
        || previous_output != party.previous_output
    {
        return Err(FixtureValidationError::FundingWitness);
    }
    validate_prevout_script(&previous_output, party)?;
    let elements = witness.to_vec();
    if witness.len() != 2
        || witness.size() > party.funding_input.max_witness_len as usize
        || elements[1] != party.params.fund_pubkey.serialize().to_vec()
    {
        return Err(FixtureValidationError::FundingWitness);
    }
    let signature =
        parse_finalized_signature(&elements[0], FixtureValidationError::FundingWitness)?;
    dlc::verify_tx_input_sig(
        secp,
        &signature,
        funding,
        input_index,
        party.funding_script_code.as_script(),
        previous_output.value,
        &party.params.fund_pubkey,
    )
    .map_err(|_| FixtureValidationError::FundingWitness)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_multisig_witness(
    secp: &Secp256k1<secp256k1_zkp::All>,
    transaction: &Transaction,
    input_index: usize,
    funding_script: &Script,
    fund_output_value: Amount,
    offer_pubkey: &PublicKey,
    accept_pubkey: &PublicKey,
    error: FixtureValidationError,
) -> Result<(), FixtureValidationError> {
    let witness = transaction
        .input
        .get(input_index)
        .ok_or(error)?
        .witness
        .to_vec();
    if witness.len() != 4 || !witness[0].is_empty() || witness[3] != funding_script.to_bytes() {
        return Err(error);
    }
    let first_signature = parse_finalized_signature(&witness[1], error)?;
    let second_signature = parse_finalized_signature(&witness[2], error)?;
    let (first_pubkey, second_pubkey) = if offer_pubkey < accept_pubkey {
        (offer_pubkey, accept_pubkey)
    } else {
        (accept_pubkey, offer_pubkey)
    };
    dlc::verify_tx_input_sig(
        secp,
        &first_signature,
        transaction,
        input_index,
        funding_script,
        fund_output_value,
        first_pubkey,
    )
    .map_err(|_| error)?;
    dlc::verify_tx_input_sig(
        secp,
        &second_signature,
        transaction,
        input_index,
        funding_script,
        fund_output_value,
        second_pubkey,
    )
    .map_err(|_| error)?;
    Ok(())
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
    let offer_funding_witness_elements =
        funding_witness_elements(&secp, &transactions.fund, 0, &offer_party)?;
    let accept_funding_witness_elements =
        funding_witness_elements(&secp, &transactions.fund, 1, &accept_party)?;
    let mut signed_funding = transactions.fund.clone();
    signed_funding.input[0].witness = Witness::from_slice(
        &offer_funding_witness_elements
            .iter()
            .map(|element| element.witness.clone())
            .collect::<Vec<_>>(),
    );
    signed_funding.input[1].witness = Witness::from_slice(
        &accept_funding_witness_elements
            .iter()
            .map(|element| element.witness.clone())
            .collect::<Vec<_>>(),
    );
    let sign = SignDlc {
        protocol_version: 1,
        contract_id: final_contract_id,
        cet_adaptor_signatures: CetAdaptorSignatures::from(offer_adaptor_signatures.as_slice()),
        refund_signature: offer_refund_signature,
        funding_signatures: FundingSignatures {
            funding_signatures: vec![FundingSignature {
                witness_elements: offer_funding_witness_elements.clone(),
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
        offer_adaptor_signatures,
        accept_adaptor_signatures,
        offer_funding_witness_elements,
        accept_funding_witness_elements,
        offer_refund_signature,
        accept_refund_signature,
        signed_funding,
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
) -> Result<(), FixtureValidationError> {
    if offer.protocol_version != 1 || accept.protocol_version != 1 || sign.protocol_version != 1 {
        return Err(FixtureValidationError::ProtocolVersion);
    }
    if offer.contract_flags != 0 {
        return Err(FixtureValidationError::ContractInfo);
    }
    if offer.temporary_contract_id != TEMPORARY_CONTRACT_ID
        || accept.temporary_contract_id != offer.temporary_contract_id
        || sign.contract_id != fixture.final_contract_id
    {
        return Err(FixtureValidationError::ContractId);
    }
    if offer.chain_hash != CHAIN_HASH {
        return Err(FixtureValidationError::ChainHash);
    }
    if offer.contract_info != fixture.offer.contract_info {
        return Err(FixtureValidationError::ContractInfo);
    }
    if offer.offer_collateral != fixture.offer_party.params.collateral
        || accept.accept_collateral != fixture.accept_party.params.collateral
    {
        return Err(FixtureValidationError::Collateral);
    }
    if offer.funding_pubkey != fixture.offer_party.params.fund_pubkey
        || accept.funding_pubkey != fixture.accept_party.params.fund_pubkey
    {
        return Err(FixtureValidationError::FundingPubkey);
    }
    if offer.payout_spk != fixture.offer_party.params.payout_script_pubkey
        || accept.payout_spk != fixture.accept_party.params.payout_script_pubkey
    {
        return Err(FixtureValidationError::PayoutScript);
    }
    if offer.payout_serial_id != fixture.offer_party.params.payout_serial_id
        || accept.payout_serial_id != fixture.accept_party.params.payout_serial_id
    {
        return Err(FixtureValidationError::PayoutSerialId);
    }
    if offer.change_spk != fixture.offer_party.params.change_script_pubkey
        || accept.change_spk != fixture.accept_party.params.change_script_pubkey
    {
        return Err(FixtureValidationError::ChangeScript);
    }
    if offer.change_serial_id != fixture.offer_party.params.change_serial_id
        || accept.change_serial_id != fixture.accept_party.params.change_serial_id
    {
        return Err(FixtureValidationError::ChangeSerialId);
    }
    if offer.fund_output_serial_id != FUND_OUTPUT_SERIAL_ID {
        return Err(FixtureValidationError::FundOutputSerialId);
    }
    if offer.fee_rate_per_vb != FEE_RATE_PER_VB {
        return Err(FixtureValidationError::FeeRate);
    }
    if offer.cet_locktime != CET_LOCKTIME || offer.refund_locktime != REFUND_LOCKTIME {
        return Err(FixtureValidationError::Locktime);
    }
    if offer.funding_inputs.len() != 1 || accept.funding_inputs.len() != 1 {
        return Err(FixtureValidationError::FundingInputCount);
    }
    validate_funding_input_binding(
        &offer.funding_inputs[0],
        &fixture.offer_party.params.inputs[0],
        &fixture.transactions.fund.input[0],
        &fixture.offer_party,
    )?;
    validate_funding_input_binding(
        &accept.funding_inputs[0],
        &fixture.accept_party.params.inputs[0],
        &fixture.transactions.fund.input[1],
        &fixture.accept_party,
    )?;

    let accept_signatures = &accept.cet_adaptor_signatures.ecdsa_adaptor_signatures;
    let sign_signatures = &sign.cet_adaptor_signatures.ecdsa_adaptor_signatures;
    if accept_signatures.len() != fixture.payouts.len()
        || sign_signatures.len() != fixture.payouts.len()
    {
        return Err(FixtureValidationError::SignatureCardinality);
    }
    if accept_signatures
        .iter()
        .zip(&fixture.accept_adaptor_signatures)
        .any(|(actual, expected)| actual.signature != *expected)
        || sign_signatures
            .iter()
            .zip(&fixture.offer_adaptor_signatures)
            .any(|(actual, expected)| actual.signature != *expected)
    {
        return Err(FixtureValidationError::SignatureContents);
    }
    if accept.refund_signature != fixture.accept_refund_signature
        || sign.refund_signature != fixture.offer_refund_signature
    {
        return Err(FixtureValidationError::SignatureContents);
    }
    if sign.funding_signatures.funding_signatures.len() != offer.funding_inputs.len() {
        return Err(FixtureValidationError::SignatureCardinality);
    }
    if sign.funding_signatures.funding_signatures[0].witness_elements
        != fixture.offer_funding_witness_elements
    {
        return Err(FixtureValidationError::FundingWitness);
    }
    validate_funding_witness_stack(
        &Secp256k1::new(),
        &fixture.signed_funding,
        0,
        &fixture.offer_party,
        &fixture.signed_funding.input[0].witness,
    )?;

    let secp = Secp256k1::new();
    let funding_script = fixture.transactions.funding_script_pubkey.as_script();
    let fund_output_value = fixture.transactions.get_fund_output().value;
    for (index, outcome) in [OUTCOME_NO, OUTCOME_YES].into_iter().enumerate() {
        let messages = &[vec![outcome_message(outcome)]];
        dlc::verify_cet_adaptor_sig_from_oracle_info(
            &secp,
            &accept_signatures[index].signature,
            &fixture.transactions.cets[index],
            &fixture.oracle.oracle_infos,
            &accept.funding_pubkey,
            funding_script,
            fund_output_value,
            messages,
        )
        .map_err(|_| FixtureValidationError::SignatureContents)?;
        dlc::verify_cet_adaptor_sig_from_oracle_info(
            &secp,
            &sign_signatures[index].signature,
            &fixture.transactions.cets[index],
            &fixture.oracle.oracle_infos,
            &offer.funding_pubkey,
            funding_script,
            fund_output_value,
            messages,
        )
        .map_err(|_| FixtureValidationError::SignatureContents)?;
    }
    Ok(())
}

fn validate_refund_artifact(
    fixture: &Fixture,
    refund: &Transaction,
) -> Result<(), FixtureValidationError> {
    if refund.lock_time.to_consensus_u32() != REFUND_LOCKTIME {
        return Err(FixtureValidationError::Locktime);
    }
    if refund.input.len() != 1
        || refund.input[0].previous_output != fixture.transactions.get_fund_outpoint()
    {
        return Err(FixtureValidationError::RefundOutpoint);
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
        return Err(FixtureValidationError::RefundOutputs);
    }
    Ok(())
}

fn round_trip_message<T, F>(
    message: &T,
    expected_type_id: u16,
    decoder: F,
) -> Result<T, FixtureValidationError>
where
    T: Type + Writeable + PartialEq + fmt::Debug,
    F: FnOnce(&mut lightning::io::Cursor<&[u8]>) -> Result<T, lightning::ln::msgs::DecodeError>,
{
    let encoded = message_bytes(message).map_err(|_| FixtureValidationError::MessageRoundTrip)?;
    let mut cursor = lightning::io::Cursor::new(encoded.as_slice());
    let type_id = u16::read(&mut cursor).map_err(|_| FixtureValidationError::MessageTypeId)?;
    if type_id != expected_type_id || message.type_id() != expected_type_id {
        return Err(FixtureValidationError::MessageTypeId);
    }
    let decoded = decoder(&mut cursor).map_err(|_| FixtureValidationError::MessageRoundTrip)?;
    if cursor.position() != encoded.len() as u64 {
        return Err(FixtureValidationError::MessageRoundTrip);
    }
    Ok(decoded)
}

fn expect_fixture_error<T>(
    result: Result<T, FixtureValidationError>,
    expected: FixtureValidationError,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match result {
        Err(actual) if actual == expected => Ok(()),
        Err(actual) => Err(format!("{label}: expected {expected:?}, received {actual:?}").into()),
        Ok(_) => Err(format!("{label}: expected {expected:?}, received success").into()),
    }
}

fn expect_dlc_error_category<T, F>(
    result: Result<T, dlc::Error>,
    label: &str,
    category: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(&dlc::Error) -> bool,
{
    match result {
        Err(error) if category(&error) => Ok(()),
        Err(error) => Err(format!("{label}: unexpected DLC error {error:?}").into()),
        Ok(_) => Err(format!("{label}: expected a DLC error").into()),
    }
}

fn is_invalid_argument(error: &dlc::Error) -> bool {
    matches!(error, dlc::Error::InvalidArgument)
}

fn is_secp256k1_error(error: &dlc::Error) -> bool {
    matches!(error, dlc::Error::Secp256k1(_))
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
    add("signed-fund", &serialize(&fixture.signed_funding));
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

    let round_trip_offer = round_trip_message(&fixture.offer, dlc_messages::OFFER_TYPE, |reader| {
        OfferDlc::read(reader)
    })
    .map_err(|error| error.to_string())?;
    let round_trip_accept =
        round_trip_message(&fixture.accept, dlc_messages::ACCEPT_TYPE, |reader| {
            AcceptDlc::read(reader)
        })
        .map_err(|error| error.to_string())?;
    let round_trip_sign = round_trip_message(&fixture.sign, dlc_messages::SIGN_TYPE, |reader| {
        SignDlc::read(reader)
    })
    .map_err(|error| error.to_string())?;
    assert_eq!(round_trip_offer, fixture.offer);
    assert_eq!(round_trip_accept, fixture.accept);
    assert_eq!(round_trip_sign, fixture.sign);

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
    assert_eq!(
        fixture.transactions.get_fund_output().script_pubkey,
        fixture.transactions.funding_script_pubkey.to_p2wsh()
    );

    validate_funding_input_binding(
        &fixture.offer.funding_inputs[0],
        &fixture.offer_party.params.inputs[0],
        &fixture.signed_funding.input[0],
        &fixture.offer_party,
    )
    .map_err(|error| error.to_string())?;
    validate_funding_input_binding(
        &fixture.accept.funding_inputs[0],
        &fixture.accept_party.params.inputs[0],
        &fixture.signed_funding.input[1],
        &fixture.accept_party,
    )
    .map_err(|error| error.to_string())?;
    validate_funding_witness_stack(
        &secp,
        &fixture.signed_funding,
        0,
        &fixture.offer_party,
        &fixture.signed_funding.input[0].witness,
    )
    .map_err(|error| error.to_string())?;
    validate_funding_witness_stack(
        &secp,
        &fixture.signed_funding,
        1,
        &fixture.accept_party,
        &fixture.signed_funding.input[1].witness,
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(
        fixture.signed_funding.input[0].witness.to_vec(),
        fixture
            .offer_funding_witness_elements
            .iter()
            .map(|element| element.witness.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fixture.signed_funding.input[1].witness.to_vec(),
        fixture
            .accept_funding_witness_elements
            .iter()
            .map(|element| element.witness.clone())
            .collect::<Vec<_>>()
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
    let funding_script = fixture.transactions.funding_script_pubkey.as_script();
    let fund_output_value = fixture.transactions.get_fund_output().value;
    for (index, unsigned_cet) in fixture.transactions.cets.iter().enumerate() {
        for (signed_cet, party_pubkey, counterparty_pubkey) in [
            (
                &fixture.signed_cets_by_offer[index],
                &fixture.offer_party.params.fund_pubkey,
                &fixture.accept_party.params.fund_pubkey,
            ),
            (
                &fixture.signed_cets_by_accept[index],
                &fixture.accept_party.params.fund_pubkey,
                &fixture.offer_party.params.fund_pubkey,
            ),
        ] {
            if signed_cet.version != unsigned_cet.version
                || signed_cet.lock_time != unsigned_cet.lock_time
                || signed_cet.output != unsigned_cet.output
                || signed_cet.input.len() != 1
                || unsigned_cet.input.len() != 1
                || signed_cet.input[0].previous_output != unsigned_cet.input[0].previous_output
                || signed_cet.input[0].script_sig != unsigned_cet.input[0].script_sig
                || signed_cet.input[0].sequence != unsigned_cet.input[0].sequence
            {
                return Err(FixtureValidationError::CetWitness.into());
            }
            validate_multisig_witness(
                &secp,
                signed_cet,
                0,
                funding_script,
                fund_output_value,
                party_pubkey,
                counterparty_pubkey,
                FixtureValidationError::CetWitness,
            )
            .map_err(|error| error.to_string())?;
        }
    }
    if fixture.signed_refund.version != fixture.transactions.refund.version
        || fixture.signed_refund.lock_time != fixture.transactions.refund.lock_time
        || fixture.signed_refund.output != fixture.transactions.refund.output
        || fixture.signed_refund.input.len() != 1
        || fixture.transactions.refund.input.len() != 1
        || fixture.signed_refund.input[0].previous_output
            != fixture.transactions.refund.input[0].previous_output
        || fixture.signed_refund.input[0].script_sig
            != fixture.transactions.refund.input[0].script_sig
        || fixture.signed_refund.input[0].sequence != fixture.transactions.refund.input[0].sequence
    {
        return Err(FixtureValidationError::RefundWitness.into());
    }
    validate_multisig_witness(
        &secp,
        &fixture.signed_refund,
        0,
        funding_script,
        fund_output_value,
        &fixture.offer_party.params.fund_pubkey,
        &fixture.accept_party.params.fund_pubkey,
        FixtureValidationError::RefundWitness,
    )
    .map_err(|error| error.to_string())?;

    dlc::verify_tx_input_sig(
        &secp,
        &fixture.offer_refund_signature,
        &fixture.transactions.refund,
        0,
        funding_script,
        fund_output_value,
        &fixture.offer_party.params.fund_pubkey,
    )?;
    dlc::verify_tx_input_sig(
        &secp,
        &fixture.accept_refund_signature,
        &fixture.transactions.refund,
        0,
        funding_script,
        fund_output_value,
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
    expect_fixture_error(
        validate_message_binding(
            fixture,
            &fixture.offer,
            &changed_temporary_id,
            &fixture.sign,
        ),
        FixtureValidationError::ContractId,
        "changed temporary contract id",
    )?;

    let mut changed_final_id = fixture.sign.clone();
    changed_final_id.contract_id[0] ^= 1;
    expect_fixture_error(
        validate_message_binding(fixture, &fixture.offer, &fixture.accept, &changed_final_id),
        FixtureValidationError::ContractId,
        "changed final contract id",
    )?;

    let mut changed_funding_outpoint = fixture.accept.clone();
    // Keep vout 0 but replace the serialized previous transaction with a
    // different valid transaction. The parsed txid must still reconcile with
    // both the declared input and the assembled funding transaction.
    changed_funding_outpoint.funding_inputs[0].prev_tx =
        fixture.offer_party.funding_input.prev_tx.clone();
    expect_fixture_error(
        validate_message_binding(
            fixture,
            &fixture.offer,
            &changed_funding_outpoint,
            &fixture.sign,
        ),
        FixtureValidationError::FundingInputOutpoint,
        "changed funding outpoint vout",
    )?;

    let mut changed_payout = fixture.offer.clone();
    if let ContractInfo::SingleContractInfo(single) = &mut changed_payout.contract_info {
        if let ContractDescriptor::EnumeratedContractDescriptor(descriptor) =
            &mut single.contract_info.contract_descriptor
        {
            descriptor.payouts[0].offer_payout = Amount::from_sat(40_000_001);
        }
    }
    expect_fixture_error(
        validate_message_binding(fixture, &changed_payout, &fixture.accept, &fixture.sign),
        FixtureValidationError::ContractInfo,
        "changed contract payout",
    )?;

    let mut changed_serial_id = fixture.accept.clone();
    changed_serial_id.payout_serial_id += 1;
    expect_fixture_error(
        validate_message_binding(fixture, &fixture.offer, &changed_serial_id, &fixture.sign),
        FixtureValidationError::PayoutSerialId,
        "changed payout serial id",
    )?;

    let mut wrong_outcome =
        oracle_attestation(fixture, OUTCOME_NO, fixture.oracle.outcome_signatures[1]);
    expect_dlc_error_category(
        validate_attestation_binding(&secp, &wrong_outcome, &fixture.oracle.announcement),
        "wrong oracle outcome",
        is_invalid_argument,
    )?;

    let alternate_secret = fixed_secret(9);
    wrong_outcome.signatures[0] = sign_oracle_outcome(
        &secp,
        &alternate_secret,
        &fixture.oracle.nonce_secret,
        OUTCOME_YES,
    );
    wrong_outcome.outcomes[0] = OUTCOME_YES.into();
    expect_dlc_error_category(
        validate_attestation_binding(&secp, &wrong_outcome, &fixture.oracle.announcement),
        "wrong oracle signing key",
        is_invalid_argument,
    )?;

    let wrong_adaptor_outcome = vec![vec![outcome_message("maybe")]];
    expect_dlc_error_category(
        dlc::verify_cet_adaptor_sig_from_oracle_info(
            &secp,
            &fixture.accept_adaptor_signatures[0],
            &fixture.transactions.cets[0],
            &fixture.oracle.oracle_infos,
            &fixture.accept_party.params.fund_pubkey,
            fixture.transactions.funding_script_pubkey.as_script(),
            fixture.transactions.get_fund_output().value,
            &wrong_adaptor_outcome,
        ),
        "wrong adaptor outcome",
        is_secp256k1_error,
    )?;

    let mut incomplete_accept = fixture.accept.clone();
    incomplete_accept
        .cet_adaptor_signatures
        .ecdsa_adaptor_signatures
        .clear();
    expect_fixture_error(
        validate_message_binding(fixture, &fixture.offer, &incomplete_accept, &fixture.sign),
        FixtureValidationError::SignatureCardinality,
        "incomplete accept adaptor signatures",
    )?;

    let mut incomplete_sign = fixture.sign.clone();
    incomplete_sign
        .funding_signatures
        .funding_signatures
        .clear();
    expect_fixture_error(
        validate_message_binding(fixture, &fixture.offer, &fixture.accept, &incomplete_sign),
        FixtureValidationError::SignatureCardinality,
        "incomplete sign funding signatures",
    )?;

    let truncated_accept = &fixture.accept_message_bytes[..fixture.accept_message_bytes.len() - 1];
    let mut cursor = lightning::io::Cursor::new(&truncated_accept[2..]);
    match AcceptDlc::read(&mut cursor) {
        Err(lightning::ln::msgs::DecodeError::ShortRead) => {}
        Err(error) => {
            return Err(format!("truncated accept: unexpected decode error {error:?}").into())
        }
        Ok(_) => return Err("truncated accept: expected ShortRead".into()),
    }

    let mut bad_refund_locktime = fixture.transactions.refund.clone();
    bad_refund_locktime.lock_time = LockTime::from_consensus(REFUND_LOCKTIME - 1);
    expect_fixture_error(
        validate_refund_artifact(fixture, &bad_refund_locktime),
        FixtureValidationError::Locktime,
        "refund locktime",
    )?;

    let mut bad_refund_collateral = fixture.transactions.refund.clone();
    bad_refund_collateral.output[0].value += Amount::from_sat(1);
    expect_fixture_error(
        validate_refund_artifact(fixture, &bad_refund_collateral),
        FixtureValidationError::RefundOutputs,
        "refund collateral outputs",
    )?;

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
        "positive_artifacts=offer,accept,sign,funding,signed_funding,cet[2],refund,signed_cet[2],signed_refund"
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
