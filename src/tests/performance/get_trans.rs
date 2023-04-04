use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
    time::{Duration, Instant},
};

use tempfile::TempDir;
use tokio::{net::TcpSocket, task::JoinSet, time::timeout};
use ziggurat_core_metrics::{
    latency_tables::{LatencyRequestStats, LatencyRequestsTable},
    recorder::TestMetrics,
    tables::duration_as_ms,
};
use ziggurat_core_utils::err_constants::{
    ERR_NODE_BUILD, ERR_NODE_STOP, ERR_SOCKET_BIND, ERR_SYNTH_CONNECT, ERR_SYNTH_UNICAST,
    ERR_TEMPDIR_NEW,
};

use crate::{
    protocol::{
        codecs::message::Payload,
        proto::{
            tm_get_object_by_hash::ObjectType, TmGetObjectByHash, TmIndexedObject, TmTransactions,
        },
    },
    setup::node::{Node, NodeType},
    tools::{
        constants::{EXPECTED_RESULT_TIMEOUT, TEST_ACCOUNT},
        ips::ips,
        rpc::{get_transaction_info, wait_for_account_data, wait_for_state},
        synth_node::SyntheticNode,
    },
};

const MAX_PEERS: usize = 100;
const METRIC_LATENCY: &str = "transaction_test_latency";
// number of requests to send per peer
const REQUESTS: u16 = 150;

// Time to wait for response - increasing it gives better completion results but also increases
// the time it takes to run the test. 7 seconds is a good balance between the two.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(7);
const TX_HASH_LEN: usize = 32;

#[cfg_attr(
    not(feature = "performance"),
    ignore = "run this test with the 'performance' feature enabled"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[allow(non_snake_case)]
async fn p003_t1_GET_TRANSACTION_latency() {
    // ZG-PERFORMANCE-003, Transaction getting latency
    //
    // The node behaves as expected under load from other peers.
    //
    // We test the overall performance of a node's get transaction latency.
    // There are possible several error messages during the test, all with the error kind:
    // thread 'tokio-runtime-worker' panicked at 'unable to connect to node: Kind(TimedOut)', src/tests/performance/get_trans.rs:152:54
    // The above error means that ziggurat was unable to connect to the node. This error is
    // thrown above 100 concurrent connections.
    //
    // Sample results:
    //
    // ┌─────────┬────────────┬────────────┬────────────┬────────────────┬────────────┬────────────┬────────────┬────────────┬────────────┬────────────────┬────────────┬──────────────┐
    // │  peers  │  requests  │  min (ms)  │  max (ms)  │  std dev (ms)  │  10% (ms)  │  50% (ms)  │  75% (ms)  │  90% (ms)  │  99% (ms)  │  completion %  │  time (s)  │  requests/s  │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │       1 │        150 │          0 │         45 │              4 │          0 │          0 │          0 │          0 │         45 │         100.00 │       0.36 │       420.34 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      10 │        150 │          0 │         58 │              4 │          0 │          0 │          0 │          0 │          0 │         100.00 │       2.40 │       626.27 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      20 │        150 │          0 │        121 │              4 │          0 │          0 │          0 │          0 │          1 │         100.00 │       4.78 │       627.50 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      50 │        150 │          0 │        111 │              5 │          0 │          2 │          2 │          2 │         42 │         100.00 │      12.24 │       612.76 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      75 │        150 │          0 │        248 │              4 │          2 │          3 │          3 │          4 │          7 │         100.00 │      16.60 │       677.61 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │     100 │        150 │          0 │        217 │              5 │          1 │          2 │          2 │          3 │         12 │          59.00 │      22.52 │       392.93 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │     125 │        150 │          0 │        374 │             22 │          3 │          4 │          4 │          5 │          9 │          74.40 │      33.87 │       411.89 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │     150 │        150 │          0 │       1318 │             41 │          3 │          3 │          4 │          4 │        117 │          56.67 │      43.46 │       293.39 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │     200 │        150 │          0 │       7001 │            644 │          3 │          4 │          4 │          5 │       4178 │          42.50 │      55.57 │       229.45 │
    // └─────────┴────────────┴────────────┴────────────┴────────────────┴────────────┴────────────┴────────────┴────────────┴────────────┴────────────────┴────────────┴──────────────┘
    // *NOTE* run with `cargo test --release tests::performance::get_transaction -- --nocapture`
    // Before running test generate dummy devices with different ips using toos/ips.py

    let synth_counts = vec![1, 10, 20, 50, 75, 100, 125, 150, 200];

    let mut table = LatencyRequestsTable::default();

    for synth_count in synth_counts {
        let target = TempDir::new().expect(ERR_TEMPDIR_NEW);
        let mut node = Node::builder()
            .max_peers(MAX_PEERS)
            .start(target.path(), NodeType::Stateful)
            .await
            .expect(ERR_NODE_BUILD);
        let node_addr = node.addr();

        let mut synth_sockets = Vec::with_capacity(synth_count);
        let mut ips = ips();

        for _ in 0..synth_count {
            // If there is address for our thread in the pool we can use it.
            // Otherwise we'll not set bound_addr and use local IP addr (127.0.0.1).
            let ip = ips.pop().unwrap_or("127.0.0.1".to_string());

            let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::from_str(&ip).unwrap()), 0);
            let socket = TcpSocket::new_v4().unwrap();

            // Make sure we can reuse the address and port
            socket.set_reuseaddr(true).unwrap();
            socket.set_reuseport(true).unwrap();

            socket.bind(ip).expect(ERR_SOCKET_BIND);
            synth_sockets.push(socket);
        }

        // setup metrics recorder
        let test_metrics = TestMetrics::default();
        // clear metrics and register metrics
        metrics::register_histogram!(METRIC_LATENCY);

        // Wait for correct state and account data.
        wait_for_state(&node.rpc_url(), "proposing".into()).await;
        let account_data =
            wait_for_account_data(&node.rpc_url(), TEST_ACCOUNT, EXPECTED_RESULT_TIMEOUT)
                .await
                .expect("unable to get account data");

        // Get transaction info by rpc to put in cache.
        let tx = account_data.result.account_data.previous_transaction;
        let _ = get_transaction_info(&node.rpc_url(), tx.clone())
            .await
            .expect("unable to get transaction info");

        let mut tx_hash = [0u8; TX_HASH_LEN];
        hex::decode_to_slice(&tx, &mut tx_hash as &mut [u8])
            .expect("unable to decode transaction hash");

        let mut synth_handles = JoinSet::new();
        let test_start = tokio::time::Instant::now();

        for socket in synth_sockets {
            synth_handles.spawn(simulate_peer(node_addr, socket, tx_hash));
        }

        // wait for peers to complete
        while (synth_handles.join_next().await).is_some() {}

        let time_taken_secs = test_start.elapsed().as_secs_f64();

        let snapshot = test_metrics.take_snapshot();
        if let Some(latencies) = snapshot.construct_histogram(METRIC_LATENCY) {
            if latencies.entries() >= 1 {
                // add stats to table display
                table.add_row(LatencyRequestStats::new(
                    synth_count as u16,
                    REQUESTS,
                    latencies,
                    time_taken_secs,
                ));
            }
        }

        node.stop().expect(ERR_NODE_STOP);
    }

    // Display results table
    println!("\r\n{table}");
}

#[allow(unused_must_use)] // just for result of the timeout
async fn simulate_peer(node_addr: SocketAddr, socket: TcpSocket, tx_hash: [u8; TX_HASH_LEN]) {
    let mut synth_node = SyntheticNode::new(&Default::default()).await;

    // Establish peer connection
    synth_node
        .connect_from(node_addr, socket)
        .await
        .expect(ERR_SYNTH_CONNECT);

    for seq in 0..REQUESTS {
        let payload = Payload::TmGetObjectByHash(TmGetObjectByHash {
            r#type: ObjectType::OtTransactions as i32,
            query: true,
            seq: Some(seq as u32),
            ledger_hash: None,
            fat: None,
            objects: vec![TmIndexedObject {
                hash: Some(tx_hash.into()),
                node_id: None,
                index: None,
                data: None,
                ledger_seq: None,
            }],
        });

        // Query transaction via peer protocol.
        if !synth_node.is_connected(node_addr) {
            break;
        }

        synth_node
            .unicast(node_addr, payload)
            .expect(ERR_SYNTH_UNICAST);

        let now = Instant::now();

        // We can safely drop the result here because we don't care about it - if the message is
        // received and it's our response we simply register it for histogram and break the loop.
        // In every other case we simply move out and go to another request iteration.
        timeout(RESPONSE_TIMEOUT, async {
            loop {
                let m = synth_node.recv_message().await;
                if matches!(
                    &m.1.payload,
                    Payload::TmTransactions(TmTransactions {transactions})
                    if transactions.len() == 1
                ) {
                    metrics::histogram!(METRIC_LATENCY, duration_as_ms(now.elapsed()));
                    break;
                }
            }
        })
        .await;
    }

    synth_node.shut_down().await
}
