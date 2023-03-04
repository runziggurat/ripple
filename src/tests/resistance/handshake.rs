use std::net::{IpAddr, Ipv4Addr};

use pea2pea::{
    ConnectionSide,
    ConnectionSide::{Initiator, Responder},
};
use tempfile::TempDir;

use crate::{
    setup::{
        constants::CONNECTION_TIMEOUT,
        node::{Node, NodeType},
    },
    tools::{config::SynthNodeCfg, synth_node::SyntheticNode},
    wait_until,
};

#[allow(non_snake_case)]
#[tokio::test]
async fn r001_t1_HANDSHAKE_reject_if_user_agent_too_long() {
    // ZG-RESISTANCE-001

    // Build and start the Ripple node.
    let target = TempDir::new().expect("couldn't create a temporary directory");
    let mut node = Node::builder()
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("unable to start the node");

    // Start the first synthetic node with a 'User-Agent' header that's too long.
    let mut cfg = SynthNodeCfg::default();
    cfg.handshake = cfg.handshake.map(|mut hs_cfg| {
        hs_cfg.http_ident = format!("{:8192}", 0);
        hs_cfg
    });

    let synth_node1 = SyntheticNode::new(&cfg).await;
    // Ensure this connection was rejected by the node.
    assert!(synth_node1.connect(node.addr()).await.is_err());
    assert_eq!(synth_node1.num_connected(), 0);
    assert!(!synth_node1.is_connected(node.addr()));

    // Start the second synthetic node with the default 'User-Agent'.
    let synth_node2 = SyntheticNode::new(&Default::default()).await;
    synth_node2.connect(node.addr()).await.unwrap();
    // Ensure this connection was successful.
    assert_eq!(synth_node2.num_connected(), 1);
    assert!(synth_node2.is_connected(node.addr()));

    // Shutdown all nodes.
    synth_node1.shut_down().await;
    synth_node2.shut_down().await;
    node.stop().unwrap();
}

#[allow(non_snake_case)]
#[tokio::test]
async fn r001_t2_HANDSHAKE_reject_if_server_too_long() {
    // ZG-RESISTANCE-001

    // Start the first synthetic node. Set identification ('Server' header) for the value that's too long.
    let mut cfg = SynthNodeCfg::default();
    cfg.pea2pea_config.listener_ip = Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)));
    cfg.handshake = cfg.handshake.map(|mut hs_cfg| {
        hs_cfg.http_ident = format!("{:8192}", 0);
        hs_cfg
    });

    let synth_node1 = SyntheticNode::new(&cfg).await;
    let sn1_listening_addr = synth_node1
        .start_listening()
        .await
        .expect("unable to start listening");

    // Start the second synthetic node with the default 'Server' header.
    let mut cfg2 = SynthNodeCfg::default();
    cfg2.pea2pea_config.listener_ip = Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3)));
    let synth_node2 = SyntheticNode::new(&cfg2).await;
    let sn2_listening_addr = synth_node2
        .start_listening()
        .await
        .expect("unable to start listening");

    // Build and start the Ripple node. Configure its peers such that it connects to the synthetic node above.
    let target = TempDir::new().expect("couldn't create a temporary directory");
    let mut node = Node::builder()
        .initial_peers(vec![sn1_listening_addr, sn2_listening_addr])
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("unable to start the node");

    // Ensure the connection to the second synthetic node was successful.
    wait_until!(CONNECTION_TIMEOUT, synth_node2.num_connected() > 0);

    // Ensure the connection to the first synthetic node was rejected by the node.
    wait_until!(CONNECTION_TIMEOUT, synth_node1.num_connected() == 0);

    // Shutdown all nodes.
    synth_node1.shut_down().await;
    synth_node2.shut_down().await;
    node.stop().unwrap();
}

#[allow(non_snake_case)]
#[tokio::test]
async fn r003_t1_HANDSHAKE_reject_if_public_key_has_bit_flipped() {
    // ZG-RESISTANCE-003

    // Prepare config for a synthetic node. Flip bit in the public_key.
    let mut cfg = SynthNodeCfg::default();
    cfg.handshake = cfg.handshake.map(|mut hs_cfg| {
        hs_cfg.bitflip_pub_key = true;
        hs_cfg
    });

    run_and_assert_handshake_failure(&cfg, Responder).await;
    run_and_assert_handshake_failure(&cfg, Initiator).await;
}

#[allow(non_snake_case)]
#[tokio::test]
async fn r003_t2_HANDSHAKE_reject_if_shared_value_has_bit_flipped() {
    // ZG-RESISTANCE-003

    // Prepare config for a synthetic node. Flip bit in the shared_value.
    let mut cfg = SynthNodeCfg::default();
    cfg.handshake = cfg.handshake.map(|mut hs_cfg| {
        hs_cfg.bitflip_shared_val = true;
        hs_cfg
    });

    run_and_assert_handshake_failure(&cfg, Responder).await;
    run_and_assert_handshake_failure(&cfg, Initiator).await;
}

async fn run_and_assert_handshake_failure(config: &SynthNodeCfg, connection_side: ConnectionSide) {
    // Start a SyntheticNode with the required config.
    let synth_node = SyntheticNode::new(config).await;
    let listening_addr = synth_node
        .start_listening()
        .await
        .expect("unable to start listening");

    // Build and start the Ripple node.
    let target = TempDir::new().expect("couldn't create a temporary directory");
    let initial_peers = match connection_side {
        Initiator => vec![],
        Responder => vec![listening_addr],
    };
    let mut node = Node::builder()
        .initial_peers(initial_peers)
        .start(target.path(), NodeType::Stateless)
        .await
        .expect("unable to start the node");

    // Try to connect to rippled if Initiator side.
    if connection_side == Initiator {
        assert!(synth_node.connect(node.addr()).await.is_ok());
    }
    // Sleep for some time. This is needed either for:
    // 1. Rippled to connect to the synth node (for Responder side) and reject the handshake,
    // 2. Rippled to drop connection after an unsuccessful handshake (for Initiator side)
    wait_until!(
        CONNECTION_TIMEOUT,
        !synth_node.is_connected_ip(node.addr().ip())
    );

    // Shutdown all nodes.
    synth_node.shut_down().await;
    node.stop().unwrap();
}
