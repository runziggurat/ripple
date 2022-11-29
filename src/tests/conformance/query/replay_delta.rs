use tempfile::TempDir;

use crate::{
    protocol::{
        codecs::message::{BinaryMessage, Payload},
        proto::TmReplayDeltaRequest,
    },
    setup::node::{Node, NodeType},
    tools::{rpc::wait_for_ledger_info, synth_node::SyntheticNode},
};

#[tokio::test]
#[allow(non_snake_case)]
async fn c022_TM_REPLAY_DELTA_REQUEST_TM_REPLAY_DELTA_RESPONSE_node_should_respond_for_replay_delta_request(
) {
    // ZG-CONFORMANCE-022

    // Create a rippled node.
    let target = TempDir::new().expect("Unable to create TempDir.");
    let mut node = Node::builder()
        .start(target.path(), NodeType::Stateful)
        .await
        .expect("Unable to start the rippled node.");

    // Create a synthetic node.
    let mut synth_node = SyntheticNode::new(&Default::default()).await;
    synth_node
        .connect(node.addr())
        .await
        .expect("Unable to connect.");

    // Create a payload with correct ledger hash.
    let ledger_info = wait_for_ledger_info(&node.rpc_url())
        .await
        .expect("Unable to get ledger info.");
    let ledger_hash =
        hex::decode(ledger_info.result.ledger.ledger_hash).expect("Unable to decode ledger hash.");
    let payload = Payload::TmReplayDeltaRequest(TmReplayDeltaRequest {
        ledger_hash: ledger_hash.clone(),
    });

    // Send a message from the synthetic node.
    synth_node
        .unicast(node.addr(), payload)
        .expect("Unable to send a message.");

    // Ensure that the synthetic node receives a TmReplayDeltaResponse message.
    let check = |m: &BinaryMessage| matches!(&m.payload, Payload::TmReplayDeltaResponse(response) if response.ledger_hash == ledger_hash);
    assert!(synth_node.expect_message(&check).await);

    // Shutdown.
    synth_node.shut_down().await;
    node.stop().expect("Unable to stop the rippled node.");
}
