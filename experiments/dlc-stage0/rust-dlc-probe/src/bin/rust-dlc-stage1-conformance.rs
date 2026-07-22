//! Deterministic Stage 1 rejection coverage for the pinned rust-dlc boundary.
//!
//! This binary intentionally stays below the Gateway integration boundary. It
//! uses fixed keys, synthetic transactions, and the upstream message
//! validation/adaptor-signature APIs. It does not create a wallet, persist
//! secrets, or claim CET/session readiness.

use bitcoin::hashes::Hash;
use bitcoin::{Amount, OutPoint, Script, ScriptBuf, Txid};
use dlc::{DlcTransactions, OracleInfo, PartyParams, Payout, TxInputInfo};
use dlc_messages::oracle_msgs::{
    EnumEventDescriptor, EventDescriptor, OracleAnnouncement, OracleAttestation, OracleEvent,
};
use lightning::util::ser::Writeable;
use secp256k1_zkp::{Keypair, Message, PublicKey, Secp256k1, SecretKey, XOnlyPublicKey};

const EVENT_ID: &str = "stage1-enum-event";

struct OracleFixture {
    announcement: OracleAnnouncement,
    attestation: OracleAttestation,
    oracle_secret: SecretKey,
    nonce_secret: SecretKey,
}

fn announcement_message(event: &OracleEvent) -> Result<Message, Box<dyn std::error::Error>> {
    let mut encoded_event = Vec::new();
    event.write(&mut encoded_event)?;
    let event_hash = bitcoin::hashes::sha256::Hash::hash(&encoded_event);
    Ok(Message::from_digest(event_hash.to_byte_array()))
}

fn outcome_message(outcome: &str) -> Message {
    let outcome_hash = bitcoin::hashes::sha256::Hash::hash(outcome.as_bytes());
    Message::from_digest(outcome_hash.to_byte_array())
}

fn sign_outcome(
    secp: &Secp256k1<secp256k1_zkp::All>,
    oracle_secret: &SecretKey,
    nonce_secret: &SecretKey,
    outcome: &str,
) -> secp256k1_zkp::schnorr::Signature {
    let keypair = Keypair::from_secret_key(secp, oracle_secret);
    dlc::secp_utils::schnorrsig_sign_with_nonce(
        secp,
        &outcome_message(outcome),
        &keypair,
        &nonce_secret.secret_bytes(),
    )
}

fn oracle_fixture() -> Result<OracleFixture, Box<dyn std::error::Error>> {
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
        event_id: EVENT_ID.into(),
    };
    let announcement = OracleAnnouncement {
        announcement_signature: secp
            .sign_schnorr(&announcement_message(&oracle_event)?, &oracle_keypair),
        oracle_public_key,
        oracle_event,
    };
    let attestation = OracleAttestation {
        event_id: EVENT_ID.into(),
        oracle_public_key,
        signatures: vec![sign_outcome(&secp, &oracle_secret, &nonce_secret, "yes")],
        outcomes: vec!["yes".into()],
    };

    Ok(OracleFixture {
        announcement,
        attestation,
        oracle_secret,
        nonce_secret,
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

fn require_error_category<T>(
    label: &str,
    result: Result<T, dlc::Error>,
    expected: impl Fn(&dlc::Error) -> bool,
    expected_category: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match result {
        Ok(_) => Err(format!("{label} was accepted unexpectedly").into()),
        Err(error) if expected(&error) => Ok(()),
        Err(error) => Err(format!(
            "{label} returned an unexpected error category: expected {expected_category}, got {error:?}"
        )
        .into()),
    }
}

fn require_invalid_argument<T>(
    label: &str,
    result: Result<T, dlc::Error>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_error_category(
        label,
        result,
        |error| matches!(error, dlc::Error::InvalidArgument),
        "dlc::Error::InvalidArgument",
    )
}

fn require_secp256k1<T>(
    label: &str,
    result: Result<T, dlc::Error>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_error_category(
        label,
        result,
        |error| matches!(error, dlc::Error::Secp256k1(_)),
        "dlc::Error::Secp256k1",
    )
}

fn check_valid_oracle_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let fixture = oracle_fixture()?;
    fixture.announcement.validate(&secp)?;
    validate_attestation_binding(&secp, &fixture.attestation, &fixture.announcement)?;
    Ok(())
}

fn check_wrong_event_id() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let fixture = oracle_fixture()?;
    let mut wrong_event = fixture.attestation.clone();
    wrong_event.event_id = "different-event".into();

    // rust-dlc v0.8.0 validates signatures, keys, and nonce points but does
    // not compare OracleAttestation.event_id with OracleEvent.event_id.
    if wrong_event.validate(&secp, &fixture.announcement).is_err() {
        return Err("upstream event-id behavior changed; refresh the Stage 1 evidence".into());
    }
    require_invalid_argument(
        "wrong event id",
        validate_attestation_binding(&secp, &wrong_event, &fixture.announcement),
    )
}

fn check_wrong_oracle_key() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let fixture = oracle_fixture()?;
    let alternate_secret = SecretKey::from_slice(&[9u8; 32])?;
    let alternate_keypair = Keypair::from_secret_key(&secp, &alternate_secret);
    let mut wrong_key = fixture.attestation.clone();
    wrong_key.oracle_public_key = XOnlyPublicKey::from_keypair(&alternate_keypair).0;

    require_invalid_argument(
        "wrong oracle key",
        validate_attestation_binding(&secp, &wrong_key, &fixture.announcement),
    )
}

fn check_signed_outcome_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let fixture = oracle_fixture()?;
    let mut mutated_outcome = fixture.attestation.clone();
    mutated_outcome.outcomes[0] = "no".into();

    require_invalid_argument(
        "signed-outcome mutation (signature/outcome binding)",
        validate_attestation_binding(&secp, &mutated_outcome, &fixture.announcement),
    )
}

fn check_unannounced_outcome_domain() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let fixture = oracle_fixture()?;
    let mut unannounced_outcome = fixture.attestation.clone();
    unannounced_outcome.outcomes[0] = "maybe".into();
    unannounced_outcome.signatures[0] = sign_outcome(
        &secp,
        &fixture.oracle_secret,
        &fixture.nonce_secret,
        "maybe",
    );

    // rust-dlc v0.8.0 verifies the signature/key/nonce material but does not
    // check that an enumerated outcome belongs to the announcement descriptor.
    if let Err(error) = unannounced_outcome.validate(&secp, &fixture.announcement) {
        return Err(format!(
            "upstream began rejecting a correctly signed unannounced outcome; refresh the Stage 1 evidence: {error:?}"
        )
        .into());
    }

    require_invalid_argument(
        "unannounced outcome/domain",
        validate_attestation_binding(&secp, &unannounced_outcome, &fixture.announcement),
    )
}

fn check_invalid_announcement_signature() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let fixture = oracle_fixture()?;
    let alternate_secret = SecretKey::from_slice(&[9u8; 32])?;
    let alternate_keypair = Keypair::from_secret_key(&secp, &alternate_secret);
    let mut invalid = fixture.announcement.clone();
    invalid.announcement_signature = secp.sign_schnorr(
        &announcement_message(&invalid.oracle_event)?,
        &alternate_keypair,
    );

    require_secp256k1("invalid announcement signature", invalid.validate(&secp))
}

fn check_invalid_attestation_signature() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let fixture = oracle_fixture()?;
    let alternate_secret = SecretKey::from_slice(&[9u8; 32])?;
    let mut invalid = fixture.attestation.clone();
    invalid.signatures[0] = sign_outcome(&secp, &alternate_secret, &fixture.nonce_secret, "yes");

    require_invalid_argument(
        "invalid attestation signature",
        validate_attestation_binding(&secp, &invalid, &fixture.announcement),
    )
}

fn synthetic_party(secret: [u8; 32], tx_byte: u8, serial_id: u64) -> PartyParams {
    let secp = Secp256k1::new();
    let secret = SecretKey::from_slice(&secret).expect("fixed key");
    let fund_pubkey = secret.public_key(&secp);
    PartyParams {
        fund_pubkey,
        change_script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        change_serial_id: serial_id + 1,
        payout_script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        payout_serial_id: serial_id + 2,
        inputs: vec![TxInputInfo {
            outpoint: OutPoint {
                txid: Txid::from_byte_array([tx_byte; 32]),
                vout: 0,
            },
            max_witness_len: 107,
            redeem_script: ScriptBuf::new(),
            serial_id,
        }],
        input_amount: Amount::from_sat(100_000_000),
        collateral: Amount::from_sat(50_000_000),
    }
}

fn transaction_fixture(
) -> Result<(DlcTransactions, SecretKey, PublicKey), Box<dyn std::error::Error>> {
    let offer_secret = SecretKey::from_slice(&[1u8; 32])?;
    let accept_secret = SecretKey::from_slice(&[2u8; 32])?;
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
    let _ = accept_secret;
    Ok((transactions, offer_secret, offer.fund_pubkey))
}

fn check_wrong_funding_outpoint() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let fixture = oracle_fixture()?;
    let oracle_infos = vec![OracleInfo {
        public_key: fixture.announcement.oracle_public_key,
        nonces: fixture.announcement.oracle_event.oracle_nonces.clone(),
    }];
    let (transactions, funding_secret, funding_public_key) = transaction_fixture()?;
    let message = outcome_message("yes");
    let messages = vec![vec![vec![message]], vec![vec![message]]];
    let funding_script: &Script = transactions.funding_script_pubkey.as_script();
    let fund_value = transactions.get_fund_output().value;
    let adaptor_signatures = dlc::create_cet_adaptor_sigs_from_oracle_info(
        &secp,
        &transactions.cets,
        &oracle_infos,
        &funding_secret,
        funding_script,
        fund_value,
        &messages,
    )?;
    let valid = dlc::verify_cet_adaptor_sig_from_oracle_info(
        &secp,
        &adaptor_signatures[0],
        &transactions.cets[0],
        &oracle_infos,
        &funding_public_key,
        funding_script,
        fund_value,
        &messages[0],
    );
    valid?;

    let mut wrong_outpoint_cet = transactions.cets[0].clone();
    wrong_outpoint_cet.input[0].previous_output = OutPoint {
        txid: Txid::from_byte_array([0x42; 32]),
        vout: 1,
    };
    require_secp256k1(
        "wrong funding outpoint/transaction binding",
        dlc::verify_cet_adaptor_sig_from_oracle_info(
            &secp,
            &adaptor_signatures[0],
            &wrong_outpoint_cet,
            &oracle_infos,
            &funding_public_key,
            funding_script,
            fund_value,
            &messages[0],
        ),
    )
}

fn run_all_checks() -> Result<(), Box<dyn std::error::Error>> {
    check_valid_oracle_boundary()?;
    check_wrong_event_id()?;
    check_wrong_oracle_key()?;
    check_signed_outcome_mutation()?;
    check_unannounced_outcome_domain()?;
    check_invalid_announcement_signature()?;
    check_invalid_attestation_signature()?;
    check_wrong_funding_outpoint()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_all_checks()?;
    println!("stage1=valid_oracle_boundary passed=1");
    println!("stage1=oracle_rejection_cases passed=6");
    println!("stage1=transaction_binding_rejection_cases passed=1");
    println!("stage1=total passed=8 failed=0");
    println!("upstream_event_id_check=not_implemented wrapper_enforced=true");
    println!("upstream_enum_domain_check=not_implemented wrapper_enforced=true");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        check_invalid_announcement_signature, check_invalid_attestation_signature,
        check_signed_outcome_mutation, check_unannounced_outcome_domain,
        check_valid_oracle_boundary, check_wrong_event_id, check_wrong_funding_outpoint,
        check_wrong_oracle_key,
    };

    #[test]
    fn valid_oracle_boundary_accepts() {
        check_valid_oracle_boundary().unwrap();
    }

    #[test]
    fn wrong_event_id_rejects_at_binding_boundary() {
        check_wrong_event_id().unwrap();
    }

    #[test]
    fn wrong_oracle_key_rejects() {
        check_wrong_oracle_key().unwrap();
    }

    #[test]
    fn signed_outcome_mutation_rejects() {
        check_signed_outcome_mutation().unwrap();
    }

    #[test]
    fn unannounced_outcome_domain_rejects_at_binding_boundary() {
        check_unannounced_outcome_domain().unwrap();
    }

    #[test]
    fn invalid_announcement_signature_rejects() {
        check_invalid_announcement_signature().unwrap();
    }

    #[test]
    fn invalid_attestation_signature_rejects() {
        check_invalid_attestation_signature().unwrap();
    }

    #[test]
    fn wrong_funding_outpoint_rejects() {
        check_wrong_funding_outpoint().unwrap();
    }
}
