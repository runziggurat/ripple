use std::time::Duration;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use tokio::time::sleep;
use sha2::{Sha512, Digest};
use std::time::{SystemTime, UNIX_EPOCH};

use secp256k1::{
    constants::{PUBLIC_KEY_SIZE},
    Secp256k1, SecretKey, Message
};

use crate::{
    protocol::{
        codecs::message::{BinaryMessage, Payload},
        proto::TmValidatorList,
    },
    setup::node::{Node, NodeType},
    tests::conformance::{perform_expected_message_test, PUBLIC_KEY_TYPES},
    tools::{config::TestConfig, synth_node::SyntheticNode},
};


#[derive(Deserialize,Serialize)]
struct ValidatorList {
    validators: Vec<Validator>,
}

#[derive(Deserialize,Serialize)]
struct Validator {
    validation_public_key: String,
    manifest: String,
}

#[derive(Deserialize, Serialize)]
struct ValidatorBlob {
    sequence: u32,
    expiration: u32,
    validators: Vec<Validator>
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

fn create_sha512_half_digest(buffer: &Vec<u8>) -> [u8; 32]{
    let mut hasher = Sha512::new();
    hasher.update(buffer);
    let result = hasher.finalize();

    // we return 32 bytes of 64-byte result
    let mut signature = [0u8; 32];
    for i in 0..32 {
        signature[i] = result[i];
    }
    signature
}

// run this to create a key pair.
// use the strings to instantiate a SecretKey and PublicKey
fn _gen_keys() {
    let engine = Secp256k1::new();
    let (private_key, public_key) = engine.generate_keypair(&mut secp256k1::rand::thread_rng());
    let secret_bytes = private_key.secret_bytes();
    let public_bytes = public_key.serialize();
    let secret_hex = hex::encode_upper(secret_bytes);
    let public_hex = hex::encode_upper(public_bytes);
    println!("secret key hex string {}", secret_hex);
    println!("public key hex string {}", public_hex);
}

fn create_validator_blob_json(manifest: &Vec<u8>, pkstr: &String) -> String{
    let mstr = base64::encode(manifest);
    let v = Validator {
        validation_public_key: pkstr.clone(),
        manifest: mstr,
    };
    let mut vvec: Vec<Validator> = Vec::new();
    vvec.push(v);


    // Set expiration to 1 year from now.
    // validator blob uses delta from Jan 1 2000,
    // and not 1970, per Unix epoch time
    let start = SystemTime::now();
    let epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let now = epoch.as_secs() as u32;
    let jan1_2000: u32 = 946684800;
    let year: u32 = 86400*365;
    let expiration: u32 = now + year - jan1_2000;

    let vblob = ValidatorBlob {
        sequence: 2022100501,
        expiration: expiration,
        validators: vvec
    };
    let jstr = serde_json::to_string(&vblob).unwrap();
    jstr
}

fn create_signable_manifest(sequence: u32, public_key: &Vec<u8>, signing_pub_key: &Vec<u8>) -> Vec<u8> {
    let size = 5 + 2 + public_key.len() + 2 + signing_pub_key.len();
    let mut manifest: Vec<u8> = vec!(0; size);
    manifest[0] = 0x24;
    manifest[1] = ((sequence >> 24) & 0xff) as u8;
    manifest[2] = ((sequence >> 16) & 0xff) as u8;
    manifest[3] = ((sequence >>  8) & 0xff) as u8;
    manifest[4] = (sequence & 0xff) as u8;
    let mut i = 5;

    // serialize public key
    manifest[i] = 0x71; // field code 1 for "PublicKey"
    manifest[i+1] = PUBLIC_KEY_SIZE as u8;
    i += 2;
    manifest[i..i+PUBLIC_KEY_SIZE].clone_from_slice(public_key.as_slice());
    i += PUBLIC_KEY_SIZE;

    // serialize signing public key
    manifest[i] = 0x73; // field code 3 for "SigningPubKey"
    manifest[i+1] = PUBLIC_KEY_SIZE as u8;
    i += 2;
    manifest[i..i+PUBLIC_KEY_SIZE].clone_from_slice(signing_pub_key.as_slice());
    manifest
}

fn create_final_manifest(sequence: u32, public_key: &Vec<u8>, signing_pub_key: &Vec<u8>, master_signature: &Vec<u8>, signature: &Vec<u8>) -> Vec<u8> {
    let size = 5 + 2 + public_key.len() + 2 + signing_pub_key.len() + 3 + master_signature.len() + 2 + signature.len();
    let mut manifest: Vec<u8> = vec!(0; size);
    manifest[0] = 0x24;
    manifest[1] = ((sequence >> 24) & 0xff) as u8;
    manifest[2] = ((sequence >> 16) & 0xff) as u8;
    manifest[3] = ((sequence >>  8) & 0xff) as u8;
    manifest[4] = (sequence & 0xff) as u8;
    let mut i = 5;

    // serialize public key
    manifest[i] = 0x71; // field code 1 for "PublicKey"
    manifest[i+1] = PUBLIC_KEY_SIZE as u8;
    i += 2;
    manifest[i..i+PUBLIC_KEY_SIZE].clone_from_slice(public_key.as_slice());
    i += PUBLIC_KEY_SIZE;

    // serialize signing public key
    manifest[i] = 0x73; // field code 3 for "SigningPubKey"
    manifest[i+1] = PUBLIC_KEY_SIZE as u8;
    i += 2;
    manifest[i..i+PUBLIC_KEY_SIZE].clone_from_slice(signing_pub_key.as_slice());
    i += PUBLIC_KEY_SIZE;

    // serialize signature
    manifest[i] = 0x76; // field code 6 for "Signature"
    manifest[i+1] = signature.len() as u8;
    i += 2;
    manifest[i..i+signature.len()].clone_from_slice(&signature.as_slice());
    i += signature.len();

    // serialize master signature
    manifest[i] = 0x70; // field code 18 for "MasterSignature"
    manifest[i+1] = 0x12;
    manifest[i+2] = master_signature.len() as u8;
    i += 3;
    manifest[i..i+master_signature.len()].clone_from_slice(&master_signature.as_slice());
    manifest

}


fn sign_buffer(secret_key: &SecretKey, buffer: &Vec<u8>) ->  Vec<u8> {
    let engine = Secp256k1::new();
    let digest = create_sha512_half_digest(buffer);
    let message = Message::from_slice(&digest).unwrap();
    let sig = engine.sign_ecdsa(&message, secret_key);
    let sigser = sig.serialize_der();
    let sigb64 = base64::encode(sigser);
    let sigbytes = base64::decode(sigb64).expect("unable to decode a blob");
    sigbytes
}

#[tokio::test]
async fn c026() {

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
    let master_secret_hex = String::from("8484781AE8EEB87D8A5AA38483B5CBBCCE6AD66B4185BB193DDDFAD5C1F4FC06");
    // The master public key should be in the validators.txt file, in ~/.ziggurat/ripple/setup
    let master_public_hex = String::from("02ED521B8124454DD5B7769C813BD40E8D36E134DD51ACED873B49E165327F6DF2");
    let master_secret_bytes = hex::decode(&master_secret_hex).expect("unable to decode hex");
    let master_public_bytes = hex::decode(&master_public_hex).expect("unable to decode hex");
    let master_secret_key = SecretKey::from_slice(master_secret_bytes.as_slice()).expect("unable to create secret key");

    let signing_secret_hex = String::from("00F963180681C0D1D51D1128096B8FF8668AFDC41CBDED511D12D390105EFDDC");
    let signing_public_hex = String::from("03859B76317C8AA64F2D253D3547831E413F2663AE2568F7A17E85B283CC8861E4");
    let signing_secret_bytes = hex::decode(&signing_secret_hex).expect("unable to decode hex");
    let signing_public_bytes = hex::decode(&signing_public_hex).expect("unable to decode hex");
    let signing_secret_key = SecretKey::from_slice(signing_secret_bytes.as_slice()).expect("unable to create secret key");
    let man_prefix: Vec<u8> = vec!(b'M', b'A', b'N', 0);

    // 2. Create signable manifest with sequence, public key, signing public key (without signatures)
    if master_public_bytes.len() != PUBLIC_KEY_SIZE {
        panic!("invalid master public key length: {}", master_public_bytes.len());
    }
    if signing_public_bytes.len() != PUBLIC_KEY_SIZE {
        panic!("invalid signing public key length: {}", signing_public_bytes.len());
    }
    let signable_manifest = create_signable_manifest(1, &master_public_bytes, &signing_public_bytes);

    // 3. append manifest prefix
    let mut prefixed_signable: Vec<u8> = vec!(0; signable_manifest.len() + 4);
    prefixed_signable[0..4].clone_from_slice(man_prefix.as_slice());
    prefixed_signable[4..4+signable_manifest.len()].clone_from_slice(signable_manifest.clone().as_slice());

    // 4. Sign the signable manifest with master secret key, get master signature
    let master_signature_bytes = sign_buffer(&master_secret_key, &prefixed_signable);

    // 5. Sign it with signing private key, get signature
    let signature_bytes = sign_buffer(&signing_secret_key, &prefixed_signable);

    // 6. Create final manifest with sequence, public key, signing public key, master signature, signature
    let manifest = create_final_manifest(1, &master_public_bytes, &signing_public_bytes, &master_signature_bytes, &signature_bytes);

    // 7. Create Validator blob.
    let validator_blob = create_validator_blob_json(&manifest, &master_public_hex);
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

    sleep(Duration::from_secs(300)).await;
    synth_node.shut_down().await;
    node.stop().expect("unable to stop stateful node");
}
