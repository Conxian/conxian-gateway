//! Bounded Stage 0 probe for upstream rust-dlc v0.8.0.

use bitcoin::hashes::Hash;
use bitcoin::{Amount, OutPoint, ScriptBuf, Transaction, Txid};
use dlc::{DlcTransactions, EnumerationPayout, PartyParams, Payout, TxInputInfo};
use dlc_messages::oracle_msgs::{
    EnumEventDescriptor, EventDescriptor, OracleAnnouncement, OracleAttestation, OracleEvent,
};
use lightning::util::ser::Writeable;
use secp256k1_zkp::{Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};

fn enum_oracle_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let oracle_secret = SecretKey::from_slice(&[7u8; 32])?;
    let oracle_keypair = Keypair::from_secret_key(&secp, &oracle_secret);
    let oracle_public_key = XOnlyPublicKey::from_keypair(&oracle_keypair).0;

    let nonce_secret = SecretKey::from_slice(&[8u8; 32])?;
    let nonce_keypair = Keypair::from_secret_key(&secp, &nonce_secret);
    let nonce_public_key = XOnlyPublicKey::from_keypair(&nonce_keypair).0;
    let oracle_event = OracleEvent {
        oracle_nonces: vec![nonce_public_key],
        event_maturity_epoch: 100,
        event_descriptor: EventDescriptor::EnumEvent(EnumEventDescriptor {
            outcomes: vec!["no".into(), "yes".into()],
        }),
        event_id: "stage0-enum-event".into(),
    };

    let mut encoded_event = Vec::new();
    oracle_event.write(&mut encoded_event)?;
    let event_hash = bitcoin::hashes::sha256::Hash::hash(&encoded_event);
    let announcement = OracleAnnouncement {
        announcement_signature: secp.sign_schnorr(
            &Message::from_digest(event_hash.to_byte_array()),
            &oracle_keypair,
        ),
        oracle_public_key,
        oracle_event,
    };
    announcement.validate(&secp)?;

    let outcome_hash = bitcoin::hashes::sha256::Hash::hash(b"yes");
    let outcome_msg = Message::from_digest(outcome_hash.to_byte_array());
    let attestation_signature = dlc::secp_utils::schnorrsig_sign_with_nonce(
        &secp,
        &outcome_msg,
        &oracle_keypair,
        &nonce_secret.secret_bytes(),
    );
    let attestation = OracleAttestation {
        event_id: "stage0-enum-event".into(),
        oracle_public_key,
        signatures: vec![attestation_signature],
        outcomes: vec!["yes".into()],
    };
    attestation.validate(&secp, &announcement)?;

    let mut tampered = attestation.clone();
    tampered.outcomes[0] = "no".into();
    assert!(tampered.validate(&secp, &announcement).is_err());
    Ok(())
}

fn synthetic_party(secret: [u8; 32], tx_byte: u8, serial_id: u64) -> PartyParams {
    let secp = Secp256k1::new();
    let secret = SecretKey::from_slice(&secret).expect("fixed key");
    let fund_pubkey = secret.public_key(&secp);
    let input = TxInputInfo {
        outpoint: OutPoint {
            txid: Txid::from_byte_array([tx_byte; 32]),
            vout: 0,
        },
        max_witness_len: 107,
        redeem_script: ScriptBuf::new(),
        serial_id,
    };
    PartyParams {
        fund_pubkey,
        change_script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        change_serial_id: serial_id + 1,
        payout_script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        payout_serial_id: serial_id + 2,
        inputs: vec![input],
        input_amount: Amount::from_sat(100_000_000),
        collateral: Amount::from_sat(50_000_000),
    }
}

fn transaction_flow_primitives() -> Result<DlcTransactions, Box<dyn std::error::Error>> {
    let offer = synthetic_party([1u8; 32], 1, 10);
    let accept = synthetic_party([2u8; 32], 2, 20);
    let payouts = vec![
        Payout {
            offer: Amount::from_sat(40_000_000),
            accept: Amount::from_sat(60_000_000),
        },
        Payout {
            offer: Amount::from_sat(60_000_000),
            accept: Amount::from_sat(40_000_000),
        },
    ];
    let transactions = dlc::create_dlc_transactions(&offer, &accept, &payouts, 200, 1, 0, 100, 5)?;
    assert_eq!(transactions.cets.len(), 2);
    assert!(transactions.get_fund_output().value >= Amount::from_sat(100_000_000));
    assert_eq!(
        transactions.get_fund_outpoint().vout,
        transactions.get_fund_output_index() as u32
    );

    let _cets: Vec<Transaction> = dlc::create_cets(
        &transactions.fund.input[0],
        &offer.payout_script_pubkey,
        offer.payout_serial_id,
        &accept.payout_script_pubkey,
        accept.payout_serial_id,
        &payouts,
        100,
    );
    let _refund = dlc::create_refund_transaction(
        transactions.get_fund_output().clone(),
        transactions.get_fund_output().clone(),
        transactions.cets[0].input[0].clone(),
        200,
    );
    Ok(transactions)
}

fn public_surface_typechecks() {
    type CreateTransactions = fn(
        &PartyParams,
        &PartyParams,
        &[Payout],
        u32,
        u64,
        u32,
        u32,
        u64,
    ) -> Result<DlcTransactions, dlc::Error>;
    type CreateCets = fn(
        &bitcoin::TxIn,
        &bitcoin::Script,
        u64,
        &bitcoin::Script,
        u64,
        &[Payout],
        u32,
    ) -> Vec<Transaction>;
    type CreateRefund = fn(bitcoin::TxOut, bitcoin::TxOut, bitcoin::TxIn, u32) -> Transaction;
    let _create_transactions: CreateTransactions = dlc::create_dlc_transactions;
    let _create_cets: CreateCets = dlc::create_cets;
    let _create_refund: CreateRefund = dlc::create_refund_transaction;
    let _enumerated_payout = EnumerationPayout {
        outcome: "yes".into(),
        payout: Payout {
            offer: Amount::from_sat(1),
            accept: Amount::from_sat(1),
        },
    };
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    public_surface_typechecks();
    enum_oracle_boundary()?;
    let transactions = transaction_flow_primitives()?;
    println!("announcement_attestation=pass");
    println!(
        "funding_outputs={} cets={} refund_outputs={}",
        transactions.fund.output.len(),
        transactions.cets.len(),
        transactions.refund.output.len()
    );
    println!("fund_txid={}", transactions.fund.compute_txid());
    Ok(())
}
