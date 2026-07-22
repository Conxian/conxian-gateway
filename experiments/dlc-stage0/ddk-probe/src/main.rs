//! Bounded Stage 0 probe for DDK v1.1.2.

use std::str::FromStr;

use bitcoin::{
    address::NetworkUnchecked, hashes::Hash, Address, Amount, OutPoint, ScriptBuf, Txid,
};
use ddk_dlc::{
    create_dlc_transactions, EnumerationPayout, OracleInfo, PartyParams, Payout, TxInputInfo,
};
use ddk_manager::contract::{
    contract_info::ContractInfo,
    contract_input::{ContractInput, ContractInputInfo, OracleInput},
    enum_descriptor::EnumDescriptor,
    ContractDescriptor,
};
use ddk_messages::oracle_msgs::{
    tagged_announcement_msg, tagged_attestation_msg, EnumEventDescriptor, EventDescriptor,
    OracleAnnouncement, OracleAttestation, OracleEvent,
};
use secp256k1_zkp::{Keypair, PublicKey, Secp256k1, SecretKey, XOnlyPublicKey};

fn address_script(address: &str) -> ScriptBuf {
    let unchecked: Address<NetworkUnchecked> = Address::from_str(address).expect("fixed address");
    unchecked.assume_checked().script_pubkey()
}

fn party(secret_key: SecretKey, payout: &str, change: &str, serial_id: u64) -> PartyParams {
    PartyParams {
        fund_pubkey: PublicKey::from_secret_key(&Secp256k1::new(), &secret_key),
        change_script_pubkey: address_script(change),
        change_serial_id: serial_id + 1,
        payout_script_pubkey: address_script(payout),
        payout_serial_id: serial_id + 2,
        inputs: vec![TxInputInfo {
            outpoint: OutPoint {
                txid: Txid::all_zeros(),
                vout: 0,
            },
            max_witness_len: 107,
            redeem_script: ScriptBuf::new(),
            serial_id,
        }],
        dlc_inputs: vec![],
        input_amount: Amount::from_sat(1_000_000),
        collateral: Amount::from_sat(100_000),
    }
}

fn main() {
    let secp = Secp256k1::new();
    let offer_secret = SecretKey::from_slice(&[1u8; 32]).expect("fixed key");
    let accept_secret = SecretKey::from_slice(&[2u8; 32]).expect("fixed key");
    let oracle_secret = SecretKey::from_slice(&[3u8; 32]).expect("fixed key");
    let oracle_keypair = Keypair::from_secret_key(&secp, &oracle_secret);
    let oracle_public_key = XOnlyPublicKey::from_keypair(&oracle_keypair).0;

    let outcome = "yes".to_owned();
    let event_descriptor = EnumEventDescriptor {
        outcomes: vec!["yes".into(), "no".into()],
    };
    let provisional_attestation =
        secp.sign_schnorr(&tagged_attestation_msg(&outcome), &oracle_keypair);
    let nonce = XOnlyPublicKey::from_slice(&provisional_attestation[..32]).expect("fixed nonce");
    let event = OracleEvent {
        oracle_nonces: vec![nonce],
        event_maturity_epoch: 100,
        event_descriptor: EventDescriptor::EnumEvent(event_descriptor.clone()),
        event_id: "stage0-enum".into(),
    };
    let announcement = OracleAnnouncement {
        announcement_signature: secp
            .sign_schnorr(&tagged_announcement_msg(&event), &oracle_keypair),
        oracle_public_key,
        oracle_event: event,
    };
    announcement
        .validate(&secp)
        .expect("announcement validates");

    let attestation = OracleAttestation {
        event_id: "stage0-enum".into(),
        oracle_public_key,
        signatures: vec![provisional_attestation],
        outcomes: vec![outcome],
    };
    attestation
        .validate(&secp, &announcement)
        .expect("attestation validates");

    let offer = party(
        offer_secret,
        "bcrt1qszcrd5r5vfr9elze93wt39kjym7mq8lpw0rql5",
        "bcrt1q8p8qgw4lv0z3xdrune4e46e03all6hdzqhyv9s",
        10,
    );
    let accept = party(
        accept_secret,
        "bcrt1qx2rdxteeum3suhqvxgzwvyljtjjv5nd4g24vuy",
        "bcrt1q2vtmn8738rme3j56fkull9lz9ys9skut54m8l6",
        20,
    );
    let transactions = create_dlc_transactions(
        &offer,
        &accept,
        &[
            Payout {
                offer: Amount::from_sat(200_000),
                accept: Amount::ZERO,
            },
            Payout {
                offer: Amount::ZERO,
                accept: Amount::from_sat(200_000),
            },
        ],
        200,
        1,
        0,
        100,
        0,
        0,
    )
    .expect("transactions construct");
    assert_eq!(transactions.cets.len(), 2);
    assert_eq!(transactions.refund.lock_time.to_consensus_u32(), 200);

    let descriptor = EnumDescriptor {
        outcome_payouts: vec![
            EnumerationPayout {
                outcome: "yes".into(),
                payout: Payout {
                    offer: Amount::from_sat(200_000),
                    accept: Amount::ZERO,
                },
            },
            EnumerationPayout {
                outcome: "no".into(),
                payout: Payout {
                    offer: Amount::ZERO,
                    accept: Amount::from_sat(200_000),
                },
            },
        ],
    };
    descriptor
        .validate(&event_descriptor)
        .expect("descriptor validates");
    let oracle_info = OracleInfo {
        public_key: oracle_public_key,
        nonces: vec![nonce],
    };
    let offer_secret_for_adaptor = SecretKey::from_slice(&[1u8; 32]).expect("fixed key");
    let (_adaptor_kind, offer_adaptors) = descriptor
        .get_adaptor_info(
            &secp,
            std::slice::from_ref(&oracle_info),
            1,
            &offer_secret_for_adaptor,
            &transactions.funding_script_pubkey,
            transactions.get_fund_output().value,
            &transactions.cets,
        )
        .expect("adaptor info constructs");
    assert_eq!(offer_adaptors.len(), 2);
    let verified_count = descriptor
        .verify_adaptor_info(
            &secp,
            &[oracle_info],
            1,
            &offer.fund_pubkey,
            &transactions.funding_script_pubkey,
            transactions.get_fund_output().value,
            &transactions.cets,
            &offer_adaptors,
            0,
        )
        .expect("adaptor info verifies");
    assert_eq!(verified_count, 2);

    let mut executable_cet = transactions.cets[0].clone();
    let accept_secret_for_signing = SecretKey::from_slice(&[2u8; 32]).expect("fixed key");
    ddk_dlc::sign_cet(
        &secp,
        &mut executable_cet,
        &offer_adaptors[0],
        &[vec![provisional_attestation]],
        &accept_secret_for_signing,
        &offer.fund_pubkey,
        &transactions.funding_script_pubkey,
        transactions.get_fund_output().value,
    )
    .expect("CET signs");
    assert!(!executable_cet.input[0].witness.is_empty());

    let input = ContractInput {
        offer_collateral: Amount::from_sat(100_000),
        accept_collateral: Amount::from_sat(100_000),
        fee_rate: 1,
        contract_flags: 0,
        contract_infos: vec![ContractInputInfo {
            contract_descriptor: ContractDescriptor::Enum(descriptor),
            oracles: OracleInput {
                public_keys: vec![oracle_public_key],
                event_id: "stage0-enum".into(),
                threshold: 1,
            },
        }],
    };
    input.validate().expect("contract input validates");
    let manager_info = ContractInfo {
        contract_descriptor: input.contract_infos[0].contract_descriptor.clone(),
        oracle_announcements: vec![announcement],
        threshold: 1,
    };
    manager_info.validate().expect("contract info validates");

    println!(
        "ok: enum txs={} adaptor_sigs={} refund_lock={} adaptor_kind=enum",
        transactions.cets.len(),
        offer_adaptors.len(),
        transactions.refund.lock_time.to_consensus_u32()
    );
}
