use tempfile::TempDir;

use crate::{
    protocol::codecs::message::BinaryMessage,
    setup::{
        constants::CONNECTION_TIMEOUT,
        node::{Node, NodeType},
    },
    tests::conformance::{perform_expected_message_test, TestConfig},
    tools::synth_node::SyntheticNode,
    wait_until,
};

#[tokio::test]
async fn c001_handshake_when_node_receives_connection() {
    // ZG-CONFORMANCE-001

    // crate::tools::synth_node::enable_tracing();

    // Build and start the Ripple node
    let target = TempDir::new().expect("Can't build tmp dir");
    let mut node = Node::builder()
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("Unable to start node");

    // Start synthetic node.
    let synth_node = SyntheticNode::new(&Default::default()).await;
    synth_node.connect(node.addr()).await.unwrap();

    // This is only set post-handshake.
    assert_eq!(synth_node.num_connected(), 1);
    assert!(synth_node.is_connected(node.addr()));

    // Shutdown both nodes
    synth_node.shut_down().await;
    node.stop().unwrap();
}

#[tokio::test]
async fn c002_handshake_when_node_initiates_connection() {
    // ZG-CONFORMANCE-002

    // crate::tools::synth_node::enable_tracing();

    // Start synthetic node.
    let synth_node = SyntheticNode::new(&Default::default()).await;
    let listening_addr = synth_node
        .start_listening()
        .await
        .expect("unable to start listening");

    // Build and start the Ripple node and set the synth node as an initial peer.
    let target = TempDir::new().expect("Can't build tmp dir");
    let mut node = Node::builder()
        .initial_peers(vec![listening_addr])
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("Unable to start node");

    wait_until!(CONNECTION_TIMEOUT, synth_node.num_connected() == 1);
    assert!(synth_node.is_connected_ip(node.addr().ip()));

    // Shutdown both nodes
    synth_node.shut_down().await;
    node.stop().unwrap();
}

#[tokio::test]
#[should_panic]
#[allow(non_snake_case)]
async fn c006_node_should_not_send_any_messages_if_no_handshake() {
    // ZG-CONFORMANCE-006
    let response_check = |_: &BinaryMessage| true;

    perform_expected_message_test(TestConfig::default().with_handshake(None), &response_check)
        .await;
}
