use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
    time::{Duration, Instant},
};

use rand::{thread_rng, RngCore};
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
        proto::{tm_ping::PingType, TmPing},
    },
    setup::node::{Node, NodeType},
    tools::{config::SynthNodeCfg, ips::IPS, synth_node::SyntheticNode},
};

const MAX_PEERS: usize = 100;
const PINGS: u16 = 1000;
const METRIC_LATENCY: &str = "ping_perf_latency";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg_attr(
    not(feature = "performance"),
    ignore = "run this test with the 'performance' feature enabled"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[allow(non_snake_case)]
async fn p001_t1_PING_PONG_throughput() {
    // ZG-PERFORMANCE-001, Ping-Pong latency
    //
    // Testing the overall performance of a node's Ping-Pong latency. Two main parameters are
    // important for this test:
    // - the number of pings sent to the node by each synthetic peer
    // - the number of synthetic peers
    //
    // Note: This test does not assert any requirements, but requires manual inspection
    //       of the results table. This is because the results will rely on the machine
    //       running the test.
    //
    // rippled: Currently seems to perform quite well. There is one important condition - connections
    //          must be established with different source IPs. When connections come from single IP
    //          the performance drops significantly.
    //          There are possible several error messages during the test:
    //          `Result::unwrap()` on an `Err` value: Kind(InvalidData) - when connect to node failed
    //          `Result::unwrap()` on an `Err` value: Os { code: 32, kind: BrokenPipe, message: "Broken pipe" }' - communication with
    //                  already connected node suddenly failed.
    //          'Error receiving message: true' - when timeout occurs and reply was not received in 10s after sending request.
    //
    //          Conclusion: seems that lower completion percentage when there are more synthetic peers is caused by the fact that
    //          connections cannot be established and other ones are closed during the test. However, amount of nodes and ping count
    //          does not affect the latency and rippled responses have similar std time.
    //
    // Example test result (with percentile latencies) - 1000 pings per node with max_peers set to 100:
    // ┌─────────┬────────────┬────────────┬────────────┬────────────────┬────────────┬────────────┬────────────┬────────────┬────────────┬────────────────┬────────────┬──────────────┐
    // │  peers  │  requests  │  min (ms)  │  max (ms)  │  std dev (ms)  │  10% (ms)  │  50% (ms)  │  75% (ms)  │  90% (ms)  │  99% (ms)  │  completion %  │  time (s)  │  requests/s  │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │       1 │       1000 │          0 │         42 │              2 │          0 │          0 │          0 │          0 │          0 │         100.00 │       0.29 │      3412.58 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      10 │       1000 │          0 │         55 │              3 │          0 │          0 │          0 │          0 │          0 │         100.00 │       2.89 │      3455.60 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      15 │       1000 │          0 │         58 │              4 │          0 │          0 │          0 │          0 │          0 │         100.00 │       4.33 │      3465.85 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      20 │       1000 │          0 │         81 │              6 │          0 │          0 │          0 │          0 │         43 │         100.00 │       5.43 │      3685.35 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      30 │       1000 │          0 │        155 │              6 │          0 │          0 │          0 │          0 │         42 │         100.00 │       8.29 │      3616.67 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      50 │       1000 │          0 │         60 │              5 │          0 │          0 │          1 │          1 │         20 │         100.00 │      12.71 │      3933.25 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │     100 │       1000 │          0 │        175 │             11 │          0 │          1 │          1 │          2 │         50 │          85.00 │      31.81 │      2672.18 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │     150 │       1000 │          0 │        483 │             13 │          0 │          1 │          1 │          2 │         51 │          56.66 │      42.20 │      2014.08 │
    // └─────────┴────────────┴────────────┴────────────┴────────────────┴────────────┴────────────┴────────────┴────────────┴────────────┴────────────────┴────────────┴──────────────┘
    // *NOTE* run with `cargo test --release tests::performance::ping_pong -- --nocapture`
    // Before running test generate dummy devices with different ips using toos/ips.py

    let synth_counts = vec![1, 10, 15, 20, 30, 50, 100, 150];

    let mut table = LatencyRequestsTable::default();

    for synth_count in synth_counts {
        let target = TempDir::new().expect(ERR_TEMPDIR_NEW);
        let mut node = Node::builder()
            .max_peers(MAX_PEERS)
            .start(target.path(), NodeType::Stateless)
            .await
            .expect(ERR_NODE_BUILD);
        let node_addr = node.addr();

        let mut synth_sockets = Vec::with_capacity(synth_count);
        let mut ips = IPS.to_vec();

        for _ in 0..synth_count {
            // If there is address for our thread in the pool we can use it.
            // Otherwise we'll not set bound_addr and use local IP addr (127.0.0.1).
            let ip = ips.pop().unwrap_or("127.0.0.1");

            let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::from_str(ip).unwrap()), 0);
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

        let mut synth_handles = JoinSet::new();
        let test_start = tokio::time::Instant::now();

        for socket in synth_sockets {
            synth_handles.spawn(simulate_peer(node_addr, socket));
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
                    PINGS,
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
async fn simulate_peer(node_addr: SocketAddr, socket: TcpSocket) {
    let config = SynthNodeCfg::default();

    let mut synth_node = SyntheticNode::new(&config).await;

    // Establish peer connection
    synth_node
        .connect_from(node_addr, socket)
        .await
        .expect(ERR_SYNTH_CONNECT);

    let mut seq;

    for _ in 0..PINGS {
        // Generate unique sequence for each ping
        seq = thread_rng().next_u32();

        let payload = Payload::TmPing(TmPing {
            r#type: PingType::PtPing as i32,
            seq: Some(seq),
            ping_time: None,
            net_time: None,
        });

        // Send Ping
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
                    Payload::TmPing(TmPing {
                    r#type: r_type,
                    seq: Some(s),
                    ..
                    }) if *s == seq && *r_type == PingType::PtPong as i32
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
