use tempfile::TempDir;

use crate::{
    protocol::{
        codecs::message::{BinaryMessage, Payload},
        handshake::HandshakeCfg,
    },
    setup::{
        constants::TESTNET_READY_TIMEOUT,
        node::{Node, NodeType},
        testnet::TestNet,
    },
    tools::{
        config::SynthNodeCfg,
        constants::GENESIS_ACCOUNT,
        rpc::{submit_transaction, wait_for_account_data},
        synth_node::SyntheticNode,
    },
};

mod cmd;
mod handshake;
mod post_handshake;
mod query;
mod stateful;
mod status;

pub const PUBLIC_KEY_TYPES: &[u8] = &[
    0xED, // ed25519
    0x02, // secp256k1
    0x03, // secp256k1 again as this type key has two correct magic bytes.
];

// A transaction blob representing a signed transaction. Extracted by executing `tools/transfer.py` and listening with `tcpdump -A -i lo dst port 5005 or src port 5005`.
pub const TRANSACTION_BLOB: &str = "12000022000000002400000001201B0000001E61400000012A05F20068400000000000000A73210330E7FC9D56BB25D6893BA3F317AE5BCF33B3291BD63DB32654A313222F7FD020744630440220297389244D36AF12115296F409C446D9A5D808880DC7FF323AA207ED529CE6C802207AAC5D2A96CB102CBDE85D2A4BA814253CA133AC9277041CAE2E1A349FB233FF8114B5F762798A53D543A014CAF8B297CFF8F2F937E883149193D6AED0CBBC25790ADE05D020C9C6D9201DCF";

/// Test configuration for tests using the below helper test function.
#[derive(Default)]
struct TestConfig {
    /// An initial message to be sent to the rippled node.
    pub initial_message: Option<Payload>,

    /// Synthetic node configuration.
    pub synth_node_cfg: SynthNodeCfg,
}

impl TestConfig {
    /// Configure an initial message for the test.
    pub fn with_initial_message(mut self, payload: Payload) -> Self {
        self.initial_message = Some(payload);
        self
    }

    /// Allow custom handshake configuration.
    pub fn with_handshake(mut self, handshake: Option<HandshakeCfg>) -> Self {
        self.synth_node_cfg.handshake = handshake;
        self
    }
}

/// Performs a check for the required message.
/// Scenario:
/// 1. Start a stateless rippled node.
/// 2. Connect a SyntheticNode to the rippled node.
/// 3. Optional: send a message to the rippled node (configured via [TestConfig]).
/// 4. Assert that the SyntheticNode received the required message.
async fn perform_expected_message_test(
    config: TestConfig,
    response_check: &dyn Fn(&BinaryMessage) -> bool,
) {
    // Build and start Ripple node
    let target = TempDir::new().expect("Unable to create TempDir");
    let mut node = Node::builder()
        .start(target.path(), NodeType::Stateless)
        .await
        .unwrap();

    // Start synth node and connect to Ripple
    let mut synth_node = SyntheticNode::new(&config.synth_node_cfg).await;
    synth_node.connect(node.addr()).await.unwrap();

    // Send the query message (if present)
    config
        .initial_message
        .map(|message| synth_node.unicast(node.addr(), message).unwrap());

    // Wait for a response and perform the given check for it
    assert!(synth_node.expect_message(response_check).await);

    // Shutdown both nodes
    synth_node.shut_down().await;
    node.stop().unwrap();
}

/// Performs a check for the required message after a new transaction in the testnet.
/// Scenario:
/// 1. Start a testnet and wait for 'ready' status.
/// 2. Connect a SyntheticNode to the second rippled node in the testnet.
/// 3. Submit a transaction via RPC call to the first rippled node in the testnet.
/// 4. Assert that the SyntheticNode received the required message.
pub async fn perform_testnet_transaction_check(check: &dyn Fn(&BinaryMessage) -> bool) {
    const NODE_IDS: [usize; 2] = [0, 1];

    // Start a testnet.
    let mut testnet = TestNet::new().unwrap();
    testnet.start().await.unwrap();
    wait_for_account_data(
        &testnet.running[NODE_IDS[0]].rpc_url(),
        GENESIS_ACCOUNT,
        TESTNET_READY_TIMEOUT,
    )
    .await
    .expect("Unable to get the account data.");

    // Start a synthetic node and connect to the second node in the testnet.
    let mut synth_node = SyntheticNode::new(&Default::default()).await;
    synth_node
        .connect(testnet.running[NODE_IDS[1]].addr())
        .await
        .expect("Unable to connect to the second node");

    // Submit a transaction to the first node via RPC.
    let transaction = submit_transaction(
        &testnet.running[NODE_IDS[0]].rpc_url(),
        TRANSACTION_BLOB.into(),
        false,
    )
    .await
    .expect("Unable to submit the transaction.");
    assert!(transaction.result.accepted);
    assert!(transaction.result.applied);
    assert!(transaction.result.broadcast);

    // Ensure that the synthetic node connected to the second node received the required message.
    assert!(synth_node.expect_message(&check).await);

    // Shutdown.
    testnet.stop().await.expect("Unable to stop the testnet.");
    synth_node.shut_down().await;
}
