use std::net::IpAddr;

use tempfile::TempDir;

use crate::{
    protocol::{
        codecs::message::{BinaryMessage, Payload},
        proto::{TmCluster, TmClusterNode},
    },
    setup::{
        constants::{DEFAULT_PORT, SYNTHETIC_NODE_PUBLIC_KEY},
        node::{Node, NodeType},
    },
    tools::{config::SynthNodeCfg, synth_node::SyntheticNode},
};

#[allow(non_snake_case)]
#[tokio::test]
async fn c024_TM_CLUSTER_node_should_connect_to_other_nodes_in_cluster() {
    // ZG-CONFORMANCE-024

    // Start a synthetic node configured to use known keys so that rippled knows who it's talking to.
    let synth_node_ip = "127.0.0.2".parse().unwrap();
    let mut test_config = SynthNodeCfg::default();
    test_config.pea2pea_config.listener_ip = Some(IpAddr::V4(synth_node_ip));
    test_config.pea2pea_config.desired_listening_port = Some(DEFAULT_PORT);
    test_config.generate_new_keys = false;

    let mut synth_node = SyntheticNode::new(&test_config).await;
    let listening_addr = synth_node
        .start_listening()
        .await
        .expect("unable to start listening");

    // Start a rippled node with enabled clustering.
    let target = TempDir::new().expect("unable to create TempDir");
    let mut node = Node::builder()
        .enable_cluster(true)
        .initial_peers(vec![listening_addr])
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("unable to start rippled node");

    // Check for a TmCluster message.
    let check = |m: &BinaryMessage| {
        matches!(
            &m.payload,
            Payload::TmCluster(TmCluster { cluster_nodes, .. })
            if cluster_nodes.len() == 2 && public_key_in_cluster_nodes(cluster_nodes)
        )
    };
    assert!(synth_node.expect_message(&check).await);

    // Shutdown.
    synth_node.shut_down().await;
    node.stop().expect("unable to stop the rippled node");
}

fn public_key_in_cluster_nodes(cluster_nodes: &[TmClusterNode]) -> bool {
    cluster_nodes
        .iter()
        .any(|node| node.public_key == SYNTHETIC_NODE_PUBLIC_KEY)
}
