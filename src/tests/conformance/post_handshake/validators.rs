use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::{BufMut, BytesMut};
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
const RAND_SEQUENCE_NUMBER: u32 = 2022100501;
const MANIFEST_PREFIX: &[u8] = b"MAN\x00";

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
        .expect("time went backwards");
    let now = epoch.as_secs() as u32;
    let year: u32 = ONE_YEAR;
    now + year - JAN1_2000
}

fn create_validator_blob_json(manifest: &[u8], public_key: &str) -> String {
    let validator = Validator {
        validation_public_key: public_key.to_string(),
        manifest: base64::encode(manifest),
    };

    let vblob = ValidatorBlob {
        sequence: RAND_SEQUENCE_NUMBER,
        expiration: get_expiration(),
        validators: vec![validator],
    };
    serde_json::to_string(&vblob).unwrap()
}

fn create_manifest(sequence: u32, public_key: &[u8], signing_pub_key: &[u8]) -> BytesMut {
    let mut buf = BytesMut::with_capacity(1024);

    buf.put_u8(ST_TAG_SEQUENCE);
    buf.put_u32(sequence);

    // serialize public key
    buf.put_u8(ST_TAG_PUBLIC_KEY);
    buf.put_u8(PUBLIC_KEY_SIZE as u8);
    buf.extend_from_slice(public_key);

    // serialize signing public key
    buf.put_u8(ST_TAG_SIGNING_PUBLIC_KEY);
    buf.put_u8(PUBLIC_KEY_SIZE as u8);
    buf.extend_from_slice(signing_pub_key);

    buf
}

fn sign_manifest(manifest: &[u8], master_signature: &[u8], signature: &[u8]) -> BytesMut {
    let mut buf = BytesMut::with_capacity(1024);

    // copy manifest into buffer
    buf.extend_from_slice(manifest);

    // serialize signature
    buf.put_u8(ST_TAG_SIGNATURE);
    buf.put_u8(signature.len() as u8);
    buf.extend_from_slice(signature);

    // serialize master signature
    buf.put_u8(ST_TAG_VARIABLE_LENGTH_BASE);
    buf.put_u8(ST_TAG_MASTER_SIGNATURE);
    buf.put_u8(master_signature.len() as u8);
    buf.extend_from_slice(master_signature);

    buf
}

fn sign_buffer(secret_key: &SecretKey, buffer: &[u8]) -> Vec<u8> {
    let engine = Secp256k1::new();
    let digest = create_sha512_half_digest(buffer);
    let message = Message::from_slice(&digest).unwrap();
    let signature = engine.sign_ecdsa(&message, secret_key).serialize_der();
    let signature_b64 = base64::encode(signature);
    base64::decode(signature_b64).expect("unable to decode a blob")
}

fn sign_buffer_with_prefix(hash_prefix: &[u8], secret_key: &SecretKey, buffer: &[u8]) -> Vec<u8> {
    let mut prefixed_buffer = BytesMut::with_capacity(1024);
    prefixed_buffer.put(hash_prefix);
    prefixed_buffer.extend_from_slice(&buffer);
    return sign_buffer(secret_key, &prefixed_buffer);
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
    let manifest = create_manifest(1, &master_public_bytes, &signing_public_bytes);

    // 3. append manifest prefix
    //let mut prefixed_signable = BytesMut::with_capacity(1024);

    // 3. Sign the signable manifest with master secret key, get master signature
    let master_signature_bytes =
        sign_buffer_with_prefix(MANIFEST_PREFIX, &master_secret_key, &manifest);

    // 4. Sign it with signing private key, get signature
    let signature_bytes = sign_buffer_with_prefix(MANIFEST_PREFIX, &signing_secret_key, &manifest);

    // 5. Create final manifest with sequence, public key, signing public key, master signature, signature
    let signed_manifest = sign_manifest(&manifest, &master_signature_bytes, &signature_bytes);

    // 7. Create Validator blob.
    let blob = create_validator_blob_json(&signed_manifest, master_public_hex);
    let blob_b64 = base64::encode(&blob);
    let blob_bytes = base64::decode(&blob_b64).expect("unable to decode a blob");

    // 8.  Get signature for blob using master private key
    let signature = sign_buffer(&signing_secret_key, &blob_bytes);

    // 9. Setup payload, send it
    let manifest_b64 = base64::encode(signed_manifest);
    let manifest_b64_bytes = manifest_b64.as_bytes().to_vec();
    let siganture_hex = hex::encode_upper(signature);
    let siganture_hex_bytes = siganture_hex.as_bytes().to_vec();
    let blob_b64_bytes = blob_b64.as_bytes().to_vec();

    let payload = Payload::TmValidatorList(TmValidatorList {
        manifest: manifest_b64_bytes,
        blob: blob_b64_bytes,
        signature: siganture_hex_bytes,
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
