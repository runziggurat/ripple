use std::time::{Duration, SystemTime, UNIX_EPOCH};

use secp256k1::{constants::PUBLIC_KEY_SIZE, Message, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use tempfile::TempDir;
use tokio::time::sleep;

// serialization type field constants from rippled
const ST_TAG_SEQUENCE: u8 = 0x24;
const ST_TAG_VARIABLE_LENGTH_BASE: u8 = 0x70;
const ST_TAG_PUBLIC_KEY: u8 = 0x71;
const ST_TAG_SIGNING_PUBLIC_KEY: u8 = 0x73;
const ST_TAG_SIGNATURE: u8 = 0x76;
const ST_TAG_MASTER_SIGNATURE: u8 = 0x12;

const ONE_YEAR: u32 = 86400 * 365;
const JAN1_2000: u32 = 946684800;
const SOME_SEQUENCE_NUMBER: u32 = 2022100501;

use crate::{
    protocol::{
        codecs::message::{BinaryMessage, Payload},
        proto::TmValidatorList,
    },
    setup::node::{Node, NodeType},
    tests::conformance::{perform_expected_message_test, PUBLIC_KEY_TYPES},
    tools::{config::TestConfig, synth_node::SyntheticNode},
};

#[derive(Deserialize, Serialize)]
struct ValidatorList {
    validators: Vec<Validator>,
}

#[derive(Deserialize, Serialize)]
struct Validator {
    validation_public_key: String,
    manifest: String,
}

#[derive(Deserialize, Serialize)]
struct ValidatorBlob {
    sequence: u32,
    expiration: u32,
    validators: Vec<Validator>,
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c015_TM_VALIDATOR_LIST_COLLECTION_node_should_send_validator_list() {
    // ZG-CONFORMANCE-015

    // Check for a TmValidatorListCollection message.
    let check = |m: &BinaryMessage| {
        if let Payload::TmValidatorListCollection(validator_list_collection) = &m.payload {
            if let Some(blob_info) = validator_list_collection.blobs.first() {
                let decoded_blob =
                    base64::decode(&blob_info.blob).expect("unable to decode a blob");
                let text = String::from_utf8(decoded_blob)
                    .expect("unable to convert decoded blob bytes to a string");
                let validator_list = serde_json::from_str::<ValidatorList>(&text)
                    .expect("unable to deserialize a validator list");
                if validator_list.validators.is_empty() {
                    return false;
                }
                for validator in &validator_list.validators {
                    let key = hex::decode(&validator.validation_public_key)
                        .expect("unable to decode a public key");
                    if key.len() != PUBLIC_KEY_SIZE {
                        panic!("invalid public key length: {}", key.len());
                    }
                    if !PUBLIC_KEY_TYPES.contains(&key[0]) {
                        panic!("invalid public key type: {}", key[0]);
                    }
                    if validator.manifest.is_empty() {
                        panic!("empty manifest");
                    }
                }
                return true;
            }
        }
        false
    };
    perform_expected_message_test(Default::default(), &check).await;
}

fn create_sha512_half_digest(buffer: &[u8]) -> [u8; 32] {
    let mut hasher = Sha512::new();
    hasher.update(buffer);
    let result = hasher.finalize();

    // we return 32 bytes of 64-byte result
    let mut signature = [0u8; 32];
    signature.copy_from_slice(&result[..32]);
    signature
}

fn get_expiration() -> u32 {
    // expiration  = now + 1 year.
    // however, validator blob uses delta from Jan 1 2000,
    // and not 1970, per Unix epoch time
    // so we subtract time for January 1 2000
    let start = SystemTime::now();
    let epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let now = epoch.as_secs() as u32;
    let year: u32 = ONE_YEAR;
    now + year - JAN1_2000
}

fn create_validator_blob_json(manifest: &[u8], public_key: &str) -> String {
    let manifest = base64::encode(manifest);
    let v = Validator {
        validation_public_key: public_key.to_string(),
        manifest,
    };
    let vvec: Vec<Validator> = vec![v];

    let vblob = ValidatorBlob {
        sequence: SOME_SEQUENCE_NUMBER,
        expiration: get_expiration(),
        validators: vvec,
    };
    serde_json::to_string(&vblob).unwrap()
}

fn create_signable_manifest(sequence: u32, public_key: &[u8], signing_pub_key: &[u8]) -> Vec<u8> {
    let mut manifest: Vec<u8> = vec![0; 5];
    manifest[0] = ST_TAG_SEQUENCE;
    manifest[1] = ((sequence >> 24) & 0xff) as u8;
    manifest[2] = ((sequence >> 16) & 0xff) as u8;
    manifest[3] = ((sequence >> 8) & 0xff) as u8;
    manifest[4] = (sequence & 0xff) as u8;

    // serialize public key
    manifest.push(ST_TAG_PUBLIC_KEY);
    manifest.push(PUBLIC_KEY_SIZE as u8);
    manifest.extend_from_slice(public_key);

    // serialize signing public key
    manifest.push(ST_TAG_SIGNING_PUBLIC_KEY);
    manifest.push(PUBLIC_KEY_SIZE as u8);
    manifest.extend_from_slice(signing_pub_key);
    manifest
}

fn create_final_manifest(
    sequence: u32,
    public_key: &[u8],
    signing_pub_key: &[u8],
    master_signature: &[u8],
    signature: &[u8],
) -> Vec<u8> {
    let mut manifest: Vec<u8> = vec![0; 5];
    manifest[0] = ST_TAG_SEQUENCE;
    manifest[1] = ((sequence >> 24) & 0xff) as u8;
    manifest[2] = ((sequence >> 16) & 0xff) as u8;
    manifest[3] = ((sequence >> 8) & 0xff) as u8;
    manifest[4] = (sequence & 0xff) as u8;

    // serialize public key
    manifest.push(ST_TAG_PUBLIC_KEY);
    manifest.push(PUBLIC_KEY_SIZE as u8);
    manifest.extend_from_slice(public_key);

    // serialize signing public key
    manifest.push(ST_TAG_SIGNING_PUBLIC_KEY);
    manifest.push(PUBLIC_KEY_SIZE as u8);
    manifest.extend_from_slice(signing_pub_key);

    // serialize signature
    manifest.push(ST_TAG_SIGNATURE);
    manifest.push(signature.len() as u8);
    manifest.extend_from_slice(signature);

    // serialize master signature
    manifest.push(ST_TAG_VARIABLE_LENGTH_BASE);
    manifest.push(ST_TAG_MASTER_SIGNATURE);
    manifest.push(master_signature.len() as u8);
    manifest.extend_from_slice(master_signature);
    manifest
}

fn sign_buffer(secret_key: &SecretKey, buffer: &[u8]) -> Vec<u8> {
    let engine = Secp256k1::new();
    let digest = create_sha512_half_digest(buffer);
    let message = Message::from_slice(&digest).unwrap();
    let sig = engine.sign_ecdsa(&message, secret_key);
    let sigser = sig.serialize_der();
    let sigb64 = base64::encode(sigser);
    base64::decode(sigb64).expect("unable to decode a blob")
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c026_TM_VALIDATOR_LIST_send_validator_list() {
    // Create stateful node.
    let target = TempDir::new().expect("unable to create TempDir");
    let mut node = Node::builder()
        .log_to_stdout(true)
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("unable to start stateful node");

    let mut test_config = TestConfig::default();
    test_config.synth_node_config.generate_new_keys = false;
    let synth_node = SyntheticNode::new(&test_config).await;

    synth_node
        .connect(node.addr())
        .await
        .expect("unable to connect");

    // 1. Setup keys & prefix.  Both master and signing key pairs have been previously generated.
    let master_secret_hex = "8484781AE8EEB87D8A5AA38483B5CBBCCE6AD66B4185BB193DDDFAD5C1F4FC06";
    // The master public key should be in the validators.txt file, in ~/.ziggurat/ripple/setup
    let master_public_hex = "02ED521B8124454DD5B7769C813BD40E8D36E134DD51ACED873B49E165327F6DF2";
    let master_secret_bytes = hex::decode(master_secret_hex).expect("unable to decode hex");
    let master_public_bytes = hex::decode(master_public_hex).expect("unable to decode hex");
    let master_secret_key =
        SecretKey::from_slice(master_secret_bytes.as_slice()).expect("unable to create secret key");

    let signing_secret_hex = "00F963180681C0D1D51D1128096B8FF8668AFDC41CBDED511D12D390105EFDDC";
    let signing_public_hex = "03859B76317C8AA64F2D253D3547831E413F2663AE2568F7A17E85B283CC8861E4";
    let signing_secret_bytes = hex::decode(signing_secret_hex).expect("unable to decode hex");
    let signing_public_bytes = hex::decode(signing_public_hex).expect("unable to decode hex");
    let signing_secret_key = SecretKey::from_slice(signing_secret_bytes.as_slice())
        .expect("unable to create secret key");

    // 2. Create signable manifest with sequence, public key, signing public key (without signatures)

    assert_eq!(
        master_public_bytes.len(),
        PUBLIC_KEY_SIZE,
        "invalid master public key length: {}",
        master_public_bytes.len()
    );
    assert_eq!(
        signing_public_bytes.len(),
        PUBLIC_KEY_SIZE,
        "invalid signing public key length: {}",
        master_public_bytes.len()
    );

    let signable_manifest =
        create_signable_manifest(1, &master_public_bytes, &signing_public_bytes);

    // 3. append manifest prefix
    let mut prefixed_signable: Vec<u8> = vec![0; signable_manifest.len() + 4];
    prefixed_signable[0..4].clone_from_slice(b"MAN\x00");
    prefixed_signable[4..4 + signable_manifest.len()]
        .clone_from_slice(signable_manifest.clone().as_slice());

    // 4. Sign the signable manifest with master secret key, get master signature
    let master_signature_bytes = sign_buffer(&master_secret_key, &prefixed_signable);

    // 5. Sign it with signing private key, get signature
    let signature_bytes = sign_buffer(&signing_secret_key, &prefixed_signable);

    // 6. Create final manifest with sequence, public key, signing public key, master signature, signature
    let manifest = create_final_manifest(
        1,
        &master_public_bytes,
        &signing_public_bytes,
        &master_signature_bytes,
        &signature_bytes,
    );

    // 7. Create Validator blob.
    let validator_blob = create_validator_blob_json(&manifest, master_public_hex);
    let bstr = base64::encode(&validator_blob);
    let blob_bytes = base64::decode(&bstr).expect("unable to decode a blob");
    let bb = bstr.as_bytes().to_vec();

    // 8.  Get signature for blob using master private key
    let blob_signature_bytes = sign_buffer(&signing_secret_key, &blob_bytes);

    // 9. Setup payload, send it
    let mstr = base64::encode(manifest);
    let mb = mstr.as_bytes().to_vec();
    let sstr = hex::encode_upper(blob_signature_bytes);
    let sb = sstr.as_bytes().to_vec();

    let payload = Payload::TmValidatorList(TmValidatorList {
        manifest: mb,
        blob: bb,
        signature: sb,
        version: 1,
    });
    synth_node
        .unicast(node.addr(), payload)
        .expect("unable to send message");

    // TODO: confirm result from rippled that the message was valid
    // will be done in new PR

    sleep(Duration::from_secs(5)).await;
    synth_node.shut_down().await;
    node.stop().expect("unable to stop stateful node");
}
