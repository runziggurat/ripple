use std::time::Duration;

use tempfile::TempDir;

use crate::{
    fuzzing::{random_bytes, seeded_rng},
    setup::node::{Node, NodeType},
    tools::{config::TestConfig, synth_node::SyntheticNode},
    wait_until,
};

const ITERATIONS: usize = 20;
const DISCONNECT_TIMEOUT: Duration = Duration::from_millis(200);

#[tokio::test]
async fn r002_node_must_disconnect_when_receiving_random_bytes() {
    // ZG-RESISTANCE-002

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let target = TempDir::new().expect("couldn't create a temporary directory");

    let mut node = Node::builder()
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("unable to start the node");

    for payload in payloads {
        let synth_node = SyntheticNode::new(&Default::default()).await;
        synth_node.connect(node.addr()).await.unwrap();
        synth_node.unicast_bytes(node.addr(), payload).unwrap();

        // Ensure that the node has disconnected.
        wait_until!(
            DISCONNECT_TIMEOUT,
            !synth_node.is_connected_ip(node.addr().ip())
        );
        synth_node.shut_down().await;
    }

    node.stop().unwrap();
}

#[tokio::test]
async fn r004_node_must_disconnect_when_receiving_random_bytes_pre_handshake() {
    // ZG-RESISTANCE-004

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let target = TempDir::new().expect("couldn't create a temporary directory");

    let mut node = Node::builder()
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("unable to start the node");

    let mut cfg: TestConfig = Default::default();
    cfg.synth_node_config.do_handshake = false; // Disable handshake.
    for payload in payloads {
        let synth_node = SyntheticNode::new(&cfg).await;
        synth_node.connect(node.addr()).await.unwrap();
        synth_node.unicast_bytes(node.addr(), payload).unwrap();

        // Ensure that the node has disconnected.
        wait_until!(
            DISCONNECT_TIMEOUT,
            !synth_node.is_connected_ip(node.addr().ip())
        );
        synth_node.shut_down().await;
    }

    node.stop().unwrap();
}
