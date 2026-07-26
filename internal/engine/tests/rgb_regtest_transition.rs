use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use amplify::confinement::{Confined, SmallOrdMap};
use amplify::num::u5;
use amplify::ByteArray;
use bitcoin::consensus::encode::deserialize_hex;
use bitcoin::{Address, Amount, Network, Transaction};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use commit_verify::{CommitId, DigestExt};
use conxian_engine::bitcoin::{Bip340IssuerPolicy, StashResolver};
use rgb::RgbSealDef;
use rgb_persist_fs::{PileFs, StockpileDir};
use secp256k1::{Keypair, Message, Secp256k1, SecretKey};
use serde_json::{json, Value};
use strict_encoding::{StrictDecode, StrictEncode, StrictReader, StrictWriter};

const ISSUER_IDENTITY: &str = "ssi:anonymous";
const RECEIVER_AMOUNT: u64 = 40;
const CHANGE_AMOUNT: u64 = 60;
const ESPLORA_UNUSED_LOOPBACK: &str = "http://127.0.0.1:1/api";

#[derive(Debug)]
struct HarnessEnv {
    rpc_url: String,
    rpc_user: String,
    rpc_password: String,
    genesis_txid: String,
    genesis_vout: u32,
    receiver_address: Address,
    change_address: Address,
    mining_address: Address,
    artifacts: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnedSnapshot {
    amount: u64,
    opid: String,
    txid: String,
    vout: u32,
}

struct TransitionConsignment<'a> {
    source_dir: &'a Path,
    operation: &'a rgb::Operation,
    anchor: &'a bp::seals::Anchor,
    signed_hex: &'a str,
    receiver_token: rgb::AuthToken,
    name: &'a str,
    signing_key: SecretKey,
}

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} must be set by tests/rgb/rgb_regtest_e2e.sh"))
}

impl HarnessEnv {
    fn load() -> Self {
        let parse_address = |name: &str| {
            Address::from_str(&required_env(name))
                .unwrap_or_else(|_| panic!("{name} must be a Bitcoin address"))
                .require_network(Network::Regtest)
                .unwrap_or_else(|_| panic!("{name} must be a regtest address"))
        };
        Self {
            rpc_url: required_env("RGB_REGTEST_RPC_URL"),
            rpc_user: required_env("RGB_REGTEST_RPC_USER"),
            rpc_password: required_env("RGB_REGTEST_RPC_PASSWORD"),
            genesis_txid: required_env("RGB_REGTEST_GENESIS_TXID"),
            genesis_vout: required_env("RGB_REGTEST_GENESIS_VOUT")
                .parse()
                .expect("RGB_REGTEST_GENESIS_VOUT must be a u32"),
            receiver_address: parse_address("RGB_REGTEST_RECEIVER_ADDRESS"),
            change_address: parse_address("RGB_REGTEST_CHANGE_ADDRESS"),
            mining_address: parse_address("RGB_REGTEST_MINING_ADDRESS"),
            artifacts: PathBuf::from(required_env("RGB_REGTEST_ARTIFACT_DIR")),
        }
    }

    fn rpc(&self) -> Client {
        Client::new(
            &self.rpc_url,
            Auth::UserPass(self.rpc_user.clone(), self.rpc_password.clone()),
        )
        .expect("valid Bitcoin Core RPC client")
    }
}

fn copy_dir(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn sign_articles(path: &Path, secret_key: SecretKey) -> [u8; 32] {
    let mut bytes = fs::read(path).unwrap();
    let mut offset_reader = StrictReader::in_memory::<{ usize::MAX }>(&bytes[10..]);
    rgb::parse_consignment(&mut offset_reader).unwrap();
    u8::strict_decode(&mut offset_reader).unwrap();
    rgb::Semantics::strict_decode(&mut offset_reader).unwrap();
    let signature_start = 10 + offset_reader.into_cursor().position() as usize;

    let mut reader = StrictReader::in_memory::<{ usize::MAX }>(&bytes[10..]);
    rgb::parse_consignment(&mut reader).unwrap();
    u8::strict_decode(&mut reader).unwrap();
    let semantics = rgb::Semantics::strict_decode(&mut reader).unwrap();
    Option::<rgb::SigBlob>::strict_decode(&mut reader).unwrap();
    let issue = rgb::Issue::strict_decode(&mut reader).unwrap();

    let articles = rgb::Articles::with(
        semantics,
        issue,
        None,
        |_, _, _| -> Result<(), Infallible> { unreachable!() },
    )
    .unwrap();
    let commitment = articles.articles_id().commit_id().to_byte_array();
    let secp = Secp256k1::new();
    let keypair = Keypair::from_secret_key(&secp, &secret_key);
    let signature = secp.sign_schnorr_no_aux_rand(&Message::from_digest(commitment), &keypair);
    let encoded = Some(rgb::SigBlob::from_slice_checked(signature.as_ref()))
        .strict_encode(StrictWriter::in_memory::<4096>())
        .unwrap()
        .unbox()
        .unconfine();

    // The exported consignment is unsigned, so its Option<SigBlob> is one zero
    // byte. Replace exactly that option with the real BIP340 signature.
    assert_eq!(bytes[signature_start], 0);
    bytes.splice(signature_start..=signature_start, encoded);
    fs::write(path, bytes).unwrap();
    commitment
}

fn issuer_policy(secret_key: SecretKey) -> Bip340IssuerPolicy {
    let secp = Secp256k1::new();
    let keypair = Keypair::from_secret_key(&secp, &secret_key);
    let (public_key, _) = keypair.x_only_public_key();
    Bip340IssuerPolicy::from_json_str(
        &json!({
            "version": 1,
            "issuers": [{
                "identity": ISSUER_IDENTITY,
                "algorithm": "bip340-secp256k1",
                "xonly_public_key_hex": public_key.to_string(),
            }]
        })
        .to_string(),
    )
    .unwrap()
}

fn build_rgb_source(
    env: &HarnessEnv,
) -> (
    PathBuf,
    String,
    rgb::Operation,
    bp::seals::WTxoSeal,
    bp::seals::Anchor,
    [u8; 32],
) {
    let source_dir = env.artifacts.join("source.contract");
    fs::create_dir_all(&source_dir).unwrap();
    let issuer_path = env.artifacts.join("Test.issuer");
    fs::write(
        &issuer_path,
        include_bytes!("../src/bitcoin/testdata/Test.issuer"),
    )
    .unwrap();
    let issuer =
        rgb::Issuer::load(&issuer_path, |_, _, _| -> Result<_, Infallible> { Ok(()) }).unwrap();

    let genesis_outpoint = bp::Outpoint::new(
        bp::Txid::from_str(&env.genesis_txid).unwrap(),
        env.genesis_vout,
    );
    let mut noise = commit_verify::Sha256::default();
    noise.input_raw(b"conxian-rgb-regtest-v1");
    let mut params = rgb::CreateParams::new_bitcoin_testnet(issuer.codex_id(), "ConxianRegtest");
    params.push_owned_unlocked(
        "amount",
        rgb::Assignment::new_internal(genesis_outpoint, RECEIVER_AMOUNT + CHANGE_AMOUNT),
    );
    let mut contract = rgb::Contract::<rgb_persist_fs::StockFs, PileFs<bp::seals::TxoSeal>>::issue(
        issuer,
        params.transform(noise.clone()),
        |_| Ok(source_dir.clone()),
    )
    .unwrap();
    let contract_id = contract.contract_id().to_string();

    let genesis_consignment = env.artifacts.join("genesis.rgb");
    let genesis_terminal = *contract.full_state().raw.auth.keys().next().unwrap();
    contract
        .consign_to_file(&genesis_consignment, [genesis_terminal])
        .unwrap();

    let genesis_state = contract
        .state()
        .owned
        .get(&rgb::StateName::from("amount"))
        .unwrap()[0]
        .clone();
    assert_eq!(genesis_state.assignment.seal.primary, genesis_outpoint);

    let receiver_seal =
        bp::seals::WTxoSeal::vout_no_fallback(bp::Vout::from(0u32), noise.clone(), 1);
    let change_seal = bp::seals::WTxoSeal::vout_no_fallback(bp::Vout::from(1u32), noise, 2);
    let mut core = rgb::CoreParams::new("transfer");
    core.push_owned_unlocked("amount", receiver_seal.auth_token(), RECEIVER_AMOUNT);
    core.push_owned_unlocked("amount", change_seal.auth_token(), CHANGE_AMOUNT);
    let call = rgb::CallParams {
        core,
        using: BTreeMap::from([(genesis_state.addr, None)]),
        reading: vec![],
    };
    let seals = SmallOrdMap::from_checked(BTreeMap::from([(0, receiver_seal), (1, change_seal)]));
    let operation = contract.call(call, seals).unwrap();
    let opid = operation.opid();

    let bundle = bp::seals::mmb::BundleProof {
        map: SmallOrdMap::from_checked(BTreeMap::from([(
            0,
            bp::seals::mmb::Message::from_byte_array(opid.to_byte_array()),
        )])),
    };
    let protocol =
        bp::seals::mpc::ProtocolId::from_byte_array(operation.contract_id.to_byte_array());
    let source = bp::seals::mpc::Source {
        min_depth: u5::with(3),
        entropy: 0xC0_6E_58_1A,
        messages: bp::seals::mpc::MessageMap::from(Confined::from_checked(BTreeMap::from([(
            protocol,
            bp::seals::mpc::MessageSource::Mmb(bundle.clone()),
        )]))),
    };
    let tree = source.into_merkle_tree().unwrap();
    let mpc_proof = tree
        .clone()
        .into_proofs()
        .find_map(|(id, proof)| (id == protocol).then_some(proof))
        .unwrap();
    let anchor = bp::seals::Anchor {
        mmb_proof: bundle,
        mpc_protocol: protocol,
        mpc_proof,
        dbc_proof: None,
        fallback_proof: Default::default(),
    };
    (
        source_dir,
        contract_id,
        operation,
        receiver_seal,
        anchor,
        tree.commit_id().to_byte_array(),
    )
}

fn funded_signed_transaction(env: &HarnessEnv, commitment: [u8; 32]) -> (String, Transaction) {
    let rpc = env.rpc();
    let outputs = json!([
        { env.receiver_address.to_string(): Amount::from_sat(1_000).to_btc() },
        { "data": hex::encode(commitment) }
    ]);
    let raw: String = rpc
        .call(
            "createrawtransaction",
            &[
                json!([{"txid": env.genesis_txid, "vout": env.genesis_vout}]),
                outputs,
            ],
        )
        .unwrap();
    let funded: Value = rpc
        .call(
            "fundrawtransaction",
            &[
                json!(raw),
                json!({
                    "add_inputs": false,
                    "changeAddress": env.change_address.to_string(),
                    "changePosition": 1,
                    "fee_rate": 2.0
                }),
            ],
        )
        .unwrap();
    assert_eq!(funded["changepos"], 1);
    let signed: Value = rpc
        .call("signrawtransactionwithwallet", &[funded["hex"].clone()])
        .unwrap();
    assert_eq!(signed["complete"], true);
    let hex = signed["hex"].as_str().unwrap().to_owned();
    let tx: Transaction = deserialize_hex(&hex).unwrap();
    assert_eq!(tx.input.len(), 1);
    assert_eq!(
        tx.input[0].previous_output.txid.to_string(),
        env.genesis_txid
    );
    assert_eq!(tx.input[0].previous_output.vout, env.genesis_vout);
    assert_eq!(tx.output.len(), 3);
    assert_eq!(
        tx.output[0].script_pubkey,
        env.receiver_address.script_pubkey()
    );
    assert_eq!(
        tx.output[1].script_pubkey,
        env.change_address.script_pubkey()
    );
    assert_eq!(
        tx.output[2].script_pubkey.as_bytes(),
        [0x6a, 0x20]
            .iter()
            .chain(commitment.iter())
            .copied()
            .collect::<Vec<_>>()
    );
    (hex, tx)
}

fn transition_consignment(env: &HarnessEnv, fixture: TransitionConsignment<'_>) -> PathBuf {
    let contract_dir = env.artifacts.join(format!("{}.contract", fixture.name));
    copy_dir(fixture.source_dir, &contract_dir);
    let mut contract = rgb::Contract::<rgb_persist_fs::StockFs, PileFs<bp::seals::TxoSeal>>::load(
        contract_dir.clone(),
        contract_dir,
    )
    .unwrap();
    let witness = bp::Tx::from_str(fixture.signed_hex).unwrap();
    assert_eq!(
        witness.txid().to_string(),
        deserialize_hex::<Transaction>(fixture.signed_hex)
            .unwrap()
            .compute_txid()
            .to_string()
    );
    contract.include(fixture.operation.opid(), fixture.anchor.clone(), &witness);
    let consignment = env.artifacts.join(format!("{}.rgb", fixture.name));
    contract
        .consign_to_file(&consignment, [fixture.receiver_token])
        .unwrap();
    sign_articles(&consignment, fixture.signing_key);
    consignment
}

fn read_owned(stash: &Path, contract_id: &str) -> Vec<OwnedSnapshot> {
    let stockpile = StockpileDir::<bp::seals::TxoSeal>::load(
        stash.to_path_buf(),
        rgb::Consensus::Bitcoin,
        true,
    )
    .unwrap();
    let contracts: rgb::Contracts<StockpileDir<bp::seals::TxoSeal>, HashMap<_, _>, HashMap<_, _>> =
        rgb::Contracts::load(stockpile);
    let state = contracts.contract_state(rgb::ContractId::from_str(contract_id).unwrap());
    state
        .owned
        .get(&rgb::StateName::from("amount"))
        .into_iter()
        .flatten()
        .map(|owned| OwnedSnapshot {
            amount: if owned.assignment.data == RECEIVER_AMOUNT.into() {
                RECEIVER_AMOUNT
            } else if owned.assignment.data == CHANGE_AMOUNT.into() {
                CHANGE_AMOUNT
            } else if owned.assignment.data == (RECEIVER_AMOUNT + CHANGE_AMOUNT).into() {
                RECEIVER_AMOUNT + CHANGE_AMOUNT
            } else {
                panic!("unexpected RGB amount state: {:?}", owned.assignment.data)
            },
            opid: owned.addr.opid.to_string(),
            txid: owned.assignment.seal.primary.txid.to_string(),
            vout: owned.assignment.seal.primary.vout_u32(),
        })
        .collect()
}

#[test]
#[ignore = "requires tests/rgb/rgb_regtest_e2e.sh and pinned Bitcoin Core"]
fn bitcoin_core_signed_mined_rgb_transition_is_durable_and_fail_closed() {
    let env = HarnessEnv::load();
    fs::create_dir_all(&env.artifacts).unwrap();
    let issuer_key = SecretKey::from_slice(&[0x11; 32]).unwrap();
    let wrong_key = SecretKey::from_slice(&[0x22; 32]).unwrap();
    let policy = issuer_policy(issuer_key);

    let (source_dir, contract_id, operation, receiver_seal, anchor, commitment) =
        build_rgb_source(&env);
    let genesis = env.artifacts.join("genesis.rgb");
    let articles_commitment = sign_articles(&genesis, issuer_key);

    let mut wrong_commitment = commitment;
    wrong_commitment[0] ^= 0x01;
    let (bad_hex, _) = funded_signed_transaction(&env, wrong_commitment);
    let bad_commitment_consignment = transition_consignment(
        &env,
        TransitionConsignment {
            source_dir: &source_dir,
            operation: &operation,
            anchor: &anchor,
            signed_hex: &bad_hex,
            receiver_token: receiver_seal.auth_token(),
            name: "bad-commitment",
            signing_key: issuer_key,
        },
    );

    let (good_hex, good_tx) = funded_signed_transaction(&env, commitment);
    let good_consignment = transition_consignment(
        &env,
        TransitionConsignment {
            source_dir: &source_dir,
            operation: &operation,
            anchor: &anchor,
            signed_hex: &good_hex,
            receiver_token: receiver_seal.auth_token(),
            name: "good-transition",
            signing_key: issuer_key,
        },
    );
    let bad_signature_consignment = transition_consignment(
        &env,
        TransitionConsignment {
            source_dir: &source_dir,
            operation: &operation,
            anchor: &anchor,
            signed_hex: &good_hex,
            receiver_token: receiver_seal.auth_token(),
            name: "bad-signature",
            signing_key: wrong_key,
        },
    );

    let rpc = env.rpc();
    let acceptance: Vec<Value> = rpc.call("testmempoolaccept", &[json!([good_hex])]).unwrap();
    assert_eq!(acceptance[0]["allowed"], true);
    let txid = rpc.send_raw_transaction(&good_tx).unwrap();
    assert_eq!(txid, good_tx.compute_txid());
    rpc.generate_to_address(1, &env.mining_address).unwrap();
    let tx_info: Value = rpc
        .call("getrawtransaction", &[json!(txid), json!(true)])
        .unwrap();
    assert!(tx_info["confirmations"].as_u64().unwrap() >= 1);

    let stash = env.artifacts.join("receiver-stash");
    let resolver = StashResolver::new_with_network(&stash, ESPLORA_UNUSED_LOOPBACK, true).unwrap();
    resolver
        .import_consignment(&genesis, &contract_id, &policy)
        .unwrap();
    resolver
        .register_auth_token(&receiver_seal.auth_token().to_string(), receiver_seal)
        .unwrap();
    drop(resolver);
    let genesis_state = read_owned(&stash, &contract_id);
    assert_eq!(genesis_state.len(), 1);
    assert_eq!(genesis_state[0].amount, RECEIVER_AMOUNT + CHANGE_AMOUNT);

    let resolver = StashResolver::new_with_network(&stash, ESPLORA_UNUSED_LOOPBACK, true).unwrap();
    assert!(resolver
        .import_consignment(&bad_signature_consignment, &contract_id, &policy)
        .is_err());
    drop(resolver);
    assert_eq!(read_owned(&stash, &contract_id), genesis_state);

    let resolver = StashResolver::new_with_network(&stash, ESPLORA_UNUSED_LOOPBACK, true).unwrap();
    assert!(resolver
        .import_consignment(&bad_commitment_consignment, &contract_id, &policy)
        .is_err());
    drop(resolver);
    assert_eq!(read_owned(&stash, &contract_id), genesis_state);

    let resolver = StashResolver::new_with_network(&stash, ESPLORA_UNUSED_LOOPBACK, true).unwrap();
    resolver
        .import_consignment(&good_consignment, &contract_id, &policy)
        .unwrap();
    drop(resolver);
    let expected = vec![
        OwnedSnapshot {
            amount: RECEIVER_AMOUNT,
            opid: operation.opid().to_string(),
            txid: txid.to_string(),
            vout: 0,
        },
        OwnedSnapshot {
            amount: CHANGE_AMOUNT,
            opid: operation.opid().to_string(),
            txid: txid.to_string(),
            vout: 1,
        },
    ];
    assert_eq!(read_owned(&stash, &contract_id), expected);

    // A second full load proves that the semantic state, witness relation and
    // seal survive process-lifetime resolver release and filesystem reload.
    assert_eq!(read_owned(&stash, &contract_id), expected);
    fs::write(
        env.artifacts.join("proof.json"),
        serde_json::to_vec_pretty(&json!({
            "contract_id": contract_id,
            "operation_id": operation.opid().to_string(),
            "bitcoin_txid": txid.to_string(),
            "bitcoin_confirmations": tx_info["confirmations"],
            "mpc_commitment_hex": hex::encode(commitment),
            "articles_commitment_hex": hex::encode(articles_commitment),
            "receiver_amount": RECEIVER_AMOUNT,
            "receiver_vout": 0,
            "change_amount": CHANGE_AMOUNT,
            "change_vout": 1,
            "durability": "verified_after_reopen",
            "negative_bad_signature": "rejected_without_state_mutation",
            "negative_wrong_bitcoin_commitment": "rejected_without_state_mutation"
        }))
        .unwrap(),
    )
    .unwrap();
}
