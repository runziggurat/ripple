use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::{BufMut, BytesMut};
use secp256k1::{constants::PUBLIC_KEY_SIZE, Message, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use tempfile::TempDir;
use tokio::time::timeout;

// serialization type field constants from rippled
const ST_TAG_SEQUENCE: u8 = 0x24;
const ST_TAG_VARIABLE_LENGTH_BASE: u8 = 0x70;
const ST_TAG_PUBLIC_KEY: u8 = 0x71;
const ST_TAG_SIGNING_PUBLIC_KEY: u8 = 0x73;
const ST_TAG_SIGNATURE: u8 = 0x76;
const ST_TAG_MASTER_SIGNATURE: u8 = 0x12;

const ONE_YEAR: u32 = 86400 * 365;
const JAN1_2000: u32 = 946684800;
const RAND_SEQUENCE_NUMBER: u32 = 2022102584;
const MANIFEST_PREFIX: &[u8] = b"MAN\x00";
const WAIT_MSG_TIMEOUT: Duration = Duration::from_secs(5);

// The master public key should be in the validators.txt file, in ~/.ziggurat/ripple/setup
const MASTER_SECRET: &str = "8484781AE8EEB87D8A5AA38483B5CBBCCE6AD66B4185BB193DDDFAD5C1F4FC06";
const MASTER_PUBLIC: &str = "02ED521B8124454DD5B7769C813BD40E8D36E134DD51ACED873B49E165327F6DF2";
const SIGNING_SECRET: &str = "00F963180681C0D1D51D1128096B8FF8668AFDC41CBDED511D12D390105EFDDC";
const SIGNING_PUBLIC: &str = "03859B76317C8AA64F2D253D3547831E413F2663AE2568F7A17E85B283CC8861E4";

use crate::{
    protocol::{
        codecs::message::{BinaryMessage, Payload},
        proto::TmValidatorList,
    },
    setup::node::{Node, NodeType},
    tests::conformance::{perform_expected_message_test, PUBLIC_KEY_TYPES},
    tools::synth_node::SyntheticNode,
};

#[derive(Deserialize, Serialize)]
struct Validator {
    validation_public_key: String,
    manifest: String,
}

#[derive(Deserialize, Serialize)]
struct ValidatorList {
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
                    .expect("unable to convert decoded blob to a string");
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

fn create_validator_list_json(manifest: &[u8], public_key: &str) -> String {
    let validator = Validator {
        validation_public_key: public_key.to_string(),
        manifest: base64::encode(manifest),
    };

    let validator_list = ValidatorList {
        sequence: RAND_SEQUENCE_NUMBER,
        expiration: get_expiration(),
        validators: vec![validator],
    };
    serde_json::to_string(&validator_list).unwrap()
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

fn sign_manifest(mut manifest: BytesMut, master_signature: &[u8], signature: &[u8]) -> BytesMut {
    // serialize signature
    manifest.put_u8(ST_TAG_SIGNATURE);
    manifest.put_u8(signature.len() as u8);
    manifest.extend_from_slice(signature);

    // serialize master signature
    manifest.put_u8(ST_TAG_VARIABLE_LENGTH_BASE);
    manifest.put_u8(ST_TAG_MASTER_SIGNATURE);
    manifest.put_u8(master_signature.len() as u8);
    manifest.extend_from_slice(master_signature);

    manifest
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
    prefixed_buffer.extend_from_slice(buffer);

    sign_buffer(secret_key, &prefixed_buffer)
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

    // create & connect two synth nodes
    let synth_node1 = SyntheticNode::new(&Default::default()).await;
    synth_node1
        .connect(node.addr())
        .await
        .expect("unable to connect");
    let mut synth_node2 = SyntheticNode::new(&Default::default()).await;
    synth_node2
        .connect(node.addr())
        .await
        .expect("unable to connect");

    // 1. Setup keys & prefix.  Both master and signing key pairs have been previously generated.
    let master_secret = hex::decode(MASTER_SECRET).expect("unable to decode hex");
    let master_public = hex::decode(MASTER_PUBLIC).expect("unable to decode hex");
    let master_secret_key =
        SecretKey::from_slice(master_secret.as_slice()).expect("unable to create secret key");

    let signing_secret = hex::decode(SIGNING_SECRET).expect("unable to decode hex");
    let signing_public = hex::decode(SIGNING_PUBLIC).expect("unable to decode hex");
    let signing_secret_key =
        SecretKey::from_slice(signing_secret.as_slice()).expect("unable to create secret key");

    // 2. Create manifest with sequence, public key, signing public key (without signatures)
    assert_eq!(
        master_public.len(),
        PUBLIC_KEY_SIZE,
        "invalid master public key length: {}",
        master_public.len()
    );
    assert_eq!(
        signing_public.len(),
        PUBLIC_KEY_SIZE,
        "invalid signing public key length: {}",
        master_public.len()
    );
    let manifest = create_manifest(1, &master_public, &signing_public);

    // 3. Sign the manifest with master secret key, get master signature
    let master_signature_bytes =
        sign_buffer_with_prefix(MANIFEST_PREFIX, &master_secret_key, &manifest);

    // 4. Sign it with signing private key, get signature
    let signature_bytes = sign_buffer_with_prefix(MANIFEST_PREFIX, &signing_secret_key, &manifest);

    // 5. Create signed manifest with sequence, public key, signing public key, master signature, signature
    let signed_manifest = sign_manifest(manifest, &master_signature_bytes, &signature_bytes);

    // 6. Create Validator blob.
    let blob = create_validator_list_json(&signed_manifest, MASTER_PUBLIC);

    // 7.  Get signature for blob using master private key
    let signature = sign_buffer(&signing_secret_key, blob.as_bytes());

    // 8. Setup payload, send it
    let manifest = base64::encode(signed_manifest).as_bytes().to_vec();
    let signature = hex::encode_upper(signature).as_bytes().to_vec();
    let blob = base64::encode(&blob).as_bytes().to_vec();

    let payload = Payload::TmValidatorList(TmValidatorList {
        manifest,
        blob,
        signature,
        version: 1,
    });
    synth_node1
        .unicast(node.addr(), payload)
        .expect("unable to send message");

    let check = |m: &BinaryMessage| {
        if let Payload::TmValidatorListCollection(validator_list_collection) = &m.payload {
            if let Some(blob_info) = validator_list_collection.blobs.first() {
                let decoded_blob =
                    base64::decode(&blob_info.blob).expect("unable to decode a blob");

                let text = String::from_utf8(decoded_blob)
                    .expect("unable to convert decoded blob to a string");

                let validator_list = serde_json::from_str::<ValidatorList>(&text)
                    .expect("unable to deserialize a validator list");

                // Only our message has a single validator, so we skip the others
                if validator_list.validators.len() == 1 {
                    assert_eq!(validator_list.sequence, RAND_SEQUENCE_NUMBER);
                    assert_eq!(
                        validator_list.validators[0].validation_public_key,
                        MASTER_PUBLIC
                    );
                    return true;
                }
            }
        }
        false
    };

    timeout(WAIT_MSG_TIMEOUT, async {
        while !synth_node2.expect_message(&check).await {
            continue;
        }
    })
    .await
    .expect("valid TmValidatorListCollection not received in time");

    // Shutdown.
    synth_node1.shut_down().await;
    synth_node2.shut_down().await;
    node.stop().expect("unable to stop the rippled node");
}
