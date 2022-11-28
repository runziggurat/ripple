use std::{
    io::ErrorKind,
    net::SocketAddr,
    time::{Duration, Instant},
};

use rand::{thread_rng, RngCore};
use tempfile::TempDir;

use crate::{
    protocol::{
        codecs::message::Payload,
        proto::{tm_ping::PingType, TmPing},
    },
    setup::node::{Node, NodeType},
    tools::{
        config::TestConfig,
        metrics::{
            recorder::TestMetrics,
            tables::{duration_as_ms, RequestStats, RequestsTable},
        },
        synth_node::SyntheticNode,
    },
};

const PINGS: u16 = 50;
const METRIC_LATENCY: &str = "ping_perf_latency";

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
    // rippled: Currently seems to perform a bit below the expectations. Default config for rippled sets max_peers
    //          to 0 which means no limit. As stated in src/ripple/peerfinder/impl/Tuning.h defaultMaxPeers = 21 so
    //          rippled should response fine at least to 21 peers.
    //          As it was in zcash, tests can produce different error messages during run to indicate what is
    //          going on with the current connection.
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
    // Example test result (with percentile latencies) - 50 pings per node:
    // ┌─────────┬────────────┬────────────┬────────────┬────────────────┬────────────┬────────────┬────────────┬────────────┬────────────┬────────────────┬────────────┬──────────────┐
    // │  peers  │  requests  │  min (ms)  │  max (ms)  │  std dev (ms)  │  10% (ms)  │  50% (ms)  │  75% (ms)  │  90% (ms)  │  99% (ms)  │  completion %  │  time (s)  │  requests/s  │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │       1 │         50 │          0 │         40 │              6 │          0 │          0 │          0 │          0 │         40 │         100.00 │       0.17 │       298.37 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      10 │         50 │          0 │         49 │              6 │          0 │          0 │          0 │          0 │         46 │         100.00 │       2.10 │       237.73 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      15 │         50 │          0 │         49 │              6 │          0 │          0 │          0 │          0 │         45 │         100.00 │       2.93 │       256.17 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      20 │         50 │          0 │         49 │              4 │          0 │          0 │          0 │          0 │          9 │          79.80 │      13.82 │        57.74 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      30 │         50 │          0 │        302 │             14 │          0 │          0 │          0 │          0 │         47 │          45.67 │      15.82 │        43.30 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      50 │         50 │          0 │         54 │              7 │          0 │          0 │          0 │          0 │         43 │          33.48 │      20.24 │        41.35 │
    // └─────────┴────────────┴────────────┴────────────┴────────────────┴────────────┴────────────┴────────────┴────────────┴────────────┴────────────────┴────────────┴──────────────┘
    //
    // Example test result (with percentile latencies) - 150 pings per node:
    // ┌─────────┬────────────┬────────────┬────────────┬────────────────┬────────────┬────────────┬────────────┬────────────┬────────────┬────────────────┬────────────┬──────────────┐
    // │  peers  │  requests  │  min (ms)  │  max (ms)  │  std dev (ms)  │  10% (ms)  │  50% (ms)  │  75% (ms)  │  90% (ms)  │  99% (ms)  │  completion %  │  time (s)  │  requests/s  │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │       1 │        150 │          0 │         47 │              4 │          0 │          0 │          0 │          0 │         47 │         100.00 │       0.32 │       464.77 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      10 │        150 │          0 │         58 │              4 │          0 │          0 │          0 │          0 │          0 │         100.00 │       2.26 │       664.91 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      15 │        150 │          0 │         49 │              8 │          0 │          0 │          0 │          0 │         49 │          14.84 │      12.08 │        27.64 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      20 │        150 │          0 │         41 │              5 │          0 │          0 │          0 │          0 │         41 │          16.13 │      13.04 │        37.12 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      30 │        150 │          0 │         23 │              2 │          0 │          0 │          0 │          0 │         11 │          13.24 │      15.40 │        38.70 │
    // ├─────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼────────────┼────────────┼────────────┼────────────┼────────────────┼────────────┼──────────────┤
    // │      50 │        150 │          0 │         36 │              2 │          0 │          0 │          0 │          0 │          1 │           9.59 │      18.67 │        38.52 │
    // └─────────┴────────────┴────────────┴────────────┴────────────────┴────────────┴────────────┴────────────┴────────────┴────────────┴────────────────┴────────────┴──────────────┘
    //
    // *NOTE* run with `cargo test --release tests::performance::ping_pong -- --nocapture`

    let synth_counts = vec![1, 10, 15, 20, 30, 50];

    let mut table = RequestsTable::default();

    let target = TempDir::new().expect("Unable to create TempDir");
    let mut node = Node::builder()
        .start(target.path(), NodeType::Stateless)
        .await
        .unwrap();
    let node_addr = node.addr();

    for synth_count in synth_counts {
        // setup metrics recorder
        let test_metrics = TestMetrics::default();
        // clear metrics and register metrics
        metrics::register_histogram!(METRIC_LATENCY);

        let mut synth_handles = Vec::with_capacity(synth_count);
        let test_start = tokio::time::Instant::now();
        for _ in 0..synth_count {
            synth_handles.push(tokio::spawn(simulate_peer(node_addr)));
        }

        // wait for peers to complete
        for handle in synth_handles {
            let _ = handle.await;
        }

        let time_taken_secs = test_start.elapsed().as_secs_f64();

        let snapshot = test_metrics.take_snapshot();
        if let Some(latencies) = snapshot.construct_histogram(METRIC_LATENCY) {
            if latencies.entries() >= 1 {
                // add stats to table display
                table.add_row(RequestStats::new(
                    synth_count as u16,
                    PINGS,
                    latencies,
                    time_taken_secs,
                ));
            }
        }
    }

    node.stop().unwrap();

    // Display results table
    println!("\r\n{}", table);
}

async fn simulate_peer(node_addr: SocketAddr) {
    let config = TestConfig::default();
    let mut synth_node = SyntheticNode::new(&config).await;

    synth_node.connect(node_addr).await.unwrap();
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
        synth_node.unicast(node_addr, payload).unwrap();

        let now = Instant::now();
        let mut matched = false;

        // There is a need to read messages in a loop as we can read message that is not ping reply.
        while !matched {
            match synth_node
                .recv_message_timeout(Duration::from_secs(10))
                .await
            {
                Ok(m) => {
                    if matches!(
                        &m.1.payload,
                        Payload::TmPing(TmPing {
                        r#type: r_type,
                        seq: Some(s),
                        ..
                        }) if *s == seq && *r_type == PingType::PtPong as i32
                    ) {
                        metrics::histogram!(METRIC_LATENCY, duration_as_ms(now.elapsed()));
                        matched = true;
                    }
                }
                Err(e) => match e.kind() {
                    ErrorKind::TimedOut => {
                        break;
                    }
                    _ => {
                        panic!("Unexpected error: {:?}", e);
                    }
                },
            }
        }
    }

    synth_node.shut_down().await
}
