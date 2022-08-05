//! Contains test with basic queries.
//!
//! Queries and expected replies:
//!
//!     - Ping           -> Pong

use crate::{
    protocol::{
        codecs::binary::{BinaryMessage, Payload},
        proto::TmPing,
    },
    setup::node::Node,
    tools::synth_node::SyntheticNode,
};

#[tokio::test]
async fn ping() {
    let mut node = start_node().await;
    let mut synth_node = start_synth_node().await;
    synth_node.connect(node.addr()).await.unwrap();
    let payload = Payload::TmPing(TmPing {
        r#type: 0,
        seq: Some(42),
        ping_time: None,
        net_time: None,
    });
    synth_node.unicast(node.addr(), payload).unwrap();
    let check = |m: &BinaryMessage| {
        matches!(
            &m.payload,
            Payload::TmPing(TmPing {
                r#type: 1,
                seq: Some(42u32),
                ..
            })
        )
    };
    assert!(synth_node.expect_message(check).await);
    synth_node.shut_down().await;
    node.stop().unwrap();
}

async fn start_synth_node() -> SyntheticNode {
    let node_config = pea2pea::Config {
        listener_ip: Some("127.0.0.1".parse().unwrap()),
        ..Default::default()
    };
    SyntheticNode::new(node_config).await
}

async fn start_node() -> Node {
    let mut node = Node::new().unwrap();
    node.log_to_stdout(false).start().await.unwrap();
    node
}
