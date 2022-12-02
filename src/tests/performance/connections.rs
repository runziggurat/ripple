use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
    time::{Duration, Instant},
};

use tabled::{Table, Tabled};
use tempfile::TempDir;
use tokio::sync::mpsc::Sender;

use crate::{
    setup::node::{Node, NodeType},
    tools::{
        config::TestConfig,
        ips::IPS,
        metrics::{
            recorder::TestMetrics,
            tables::{fmt_table, table_float_display},
        },
        synth_node::SyntheticNode,
    },
};

#[derive(Tabled, Default, Debug, Clone)]
struct Stats {
    #[tabled(rename = "\n max peers ")]
    pub max_peers: u16,
    #[tabled(rename = "\n peers ")]
    pub peers: u16,
    #[tabled(rename = " connection \n accepted ")]
    pub accepted: u16,
    #[tabled(rename = " connection \n rejected ")]
    pub rejected: u16,
    #[tabled(rename = " connection \n terminated ")]
    pub terminated: u16,
    #[tabled(rename = " connection \n error ")]
    pub conn_error: u16,
    #[tabled(rename = " connection \n timed out ")]
    pub timed_out: u16,
    #[tabled(rename = "\n time (s) ")]
    #[tabled(display_with = "table_float_display")]
    pub time: f64,
}

impl Stats {
    fn new(max_peers: u16, peers: u16) -> Self {
        Self {
            max_peers,
            peers,
            ..Default::default()
        }
    }
}

const CONNECTION_PORT: u16 = 31337;

const METRIC_ACCEPTED: &str = "perf_conn_accepted";
const METRIC_TERMINATED: &str = "perf_conn_terminated";
const METRIC_REJECTED: &str = "perf_conn_rejected";
const METRIC_ERROR: &str = "perf_conn_error";

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn p002_connections_load() {
    // ZG-PERFORMANCE-002
    //
    // The node sheds or rejects connections when necessary.
    //
    //  1. Start a node with max_peers set to `N`
    //  2. Initiate connections from `M > N` peer nodes
    //  3. Expect only `N` to be active at a time
    //
    // Seems rippled not perform like the above way. max_peers is not a limit for connection
    // number which is set (hardcoded?) to about 20 at the time. max_peer seems to make difference
    // if any connections will be terminated. Need to investigate in the next commit.
    //
    // Sample results:
    // ┌─────────────┬─────────┬──────────────┬──────────────┬──────────────┬──────────────┬──────────────┬────────────┐
    // │             │         │  connection  │  connection  │  connection  │  connection  │  connection  │            │
    // │  max peers  │  peers  │  accepted    │  rejected    │  terminated  │  error       │  timed out   │  time (s)  │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          20 │       1 │            1 │            0 │            0 │            0 │            0 │       0.50 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          20 │       5 │            5 │            0 │            0 │            0 │            0 │       0.91 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          20 │      10 │           10 │            0 │            4 │            0 │            0 │       2.08 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          20 │      20 │           20 │            0 │           17 │            0 │            0 │       3.98 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          20 │      30 │           30 │            0 │           24 │            0 │            0 │       5.84 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          20 │      50 │           50 │            0 │           47 │            0 │            0 │       9.96 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          20 │     100 │           99 │            1 │           94 │            0 │            0 │      18.92 │
    // └─────────────┴─────────┴──────────────┴──────────────┴──────────────┴──────────────┴──────────────┴────────────┘
    //
    // ┌─────────────┬─────────┬──────────────┬──────────────┬──────────────┬──────────────┬──────────────┬────────────┐
    // │             │         │  connection  │  connection  │  connection  │  connection  │  connection  │            │
    // │  max peers  │  peers  │  accepted    │  rejected    │  terminated  │  error       │  timed out   │  time (s)  │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          50 │       1 │            1 │            0 │            0 │            0 │            0 │       0.34 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          50 │       5 │            5 │            0 │            0 │            0 │            0 │       0.70 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          50 │      10 │           10 │            0 │            0 │            0 │            0 │       2.05 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          50 │      20 │           20 │            0 │            0 │            0 │            0 │       3.97 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          50 │      30 │           29 │            1 │           24 │            0 │            0 │       5.63 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          50 │      50 │           50 │            0 │           46 │            0 │            0 │       9.39 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │          50 │     100 │          100 │            0 │           96 │            0 │            0 │      19.89 │
    // └─────────────┴─────────┴──────────────┴──────────────┴──────────────┴──────────────┴──────────────┴────────────┘
    //
    // ┌─────────────┬─────────┬──────────────┬──────────────┬──────────────┬──────────────┬──────────────┬────────────┐
    // │             │         │  connection  │  connection  │  connection  │  connection  │  connection  │            │
    // │  max peers  │  peers  │  accepted    │  rejected    │  terminated  │  error       │  timed out   │  time (s)  │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │         100 │       1 │            1 │            0 │            0 │            0 │            0 │       0.12 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │         100 │       5 │            5 │            0 │            0 │            0 │            0 │       0.70 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │         100 │      10 │           10 │            0 │            0 │            0 │            0 │       1.88 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │         100 │      20 │           20 │            0 │            0 │            0 │            0 │       4.21 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │         100 │      30 │           30 │            0 │            0 │            0 │            0 │       5.98 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │         100 │      50 │           50 │            0 │           28 │            0 │            0 │      10.25 │
    // ├─────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
    // │         100 │     100 │          100 │            0 │           96 │            0 │            0 │      19.42 │
    // └─────────────┴─────────┴──────────────┴──────────────┴──────────────┴──────────────┴──────────────┴────────────┘

    // maximum time allowed for a single iteration of the test
    const MAX_ITER_TIME: Duration = Duration::from_secs(25);

    /// maximum peers to configure node with
    const MAX_PEERS: u16 = 100;

    let synth_counts = vec![1u16, 5, 10, 20, 30, 50, 100];

    let mut all_stats = Vec::new();

    let target = TempDir::new().expect("couldn't create a temporary directory");
    // start node
    let mut node = Node::builder()
        .max_peers(MAX_PEERS as usize)
        .start(target.path(), NodeType::Stateless)
        .await
        .unwrap();
    let node_addr = node.addr();

    // This is "the hack" but is needed to perform next tests if IPS table is not empty. The
    // standard TIME_WAIT is 60s before we can use the same addr and port again.
    // So we're taking already used IPs and each thread in each iteration will use different IP.
    // If the table is empty or too small, the thread itself will notice it and will use the
    // local IP.
    // It can be removed once pea2pea will offer REUSE_ADDR options.
    let mut ip_idx = 0;

    for synth_count in synth_counts {
        // setup metrics recorder
        let test_metrics = TestMetrics::default();
        // register metrics
        metrics::register_counter!(METRIC_ACCEPTED);
        metrics::register_counter!(METRIC_TERMINATED);
        metrics::register_counter!(METRIC_REJECTED);
        metrics::register_counter!(METRIC_ERROR);

        let mut synth_handles = Vec::with_capacity(synth_count as usize);
        let mut synth_exits = Vec::with_capacity(synth_count as usize);
        let (handshake_tx, mut handshake_rx) =
            tokio::sync::mpsc::channel::<()>(synth_count as usize);

        let test_start = Instant::now();

        // start synthetic nodes
        for _ in 0..synth_count {
            let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
            synth_exits.push(exit_tx);

            let synth_handshaken = handshake_tx.clone();
            // Synthetic node runs until it completes or is instructed to exit
            synth_handles.push(tokio::spawn(async move {
                tokio::select! {
                    _ = exit_rx => {},
                    _ = simulate_peer(node_addr, synth_handshaken, ip_idx) => {},
                };
            }));
            ip_idx += 1;
        }

        // Wait for all peers to indicate that they've completed the handshake portion
        // or the iteration timeout is exceeded.
        let _ = tokio::time::timeout(MAX_ITER_TIME, async move {
            for _ in 0..synth_count {
                handshake_rx.recv().await.unwrap();
            }
        })
        .await;

        // Send stop signal to peer nodes. We ignore the possible error
        // result as this will occur with peers that have already exited.
        for stop in synth_exits {
            let _ = stop.send(());
        }

        // Wait for peers to complete
        for handle in synth_handles {
            handle.await.unwrap();
        }

        // Collect stats for this run
        let mut stats = Stats::new(MAX_PEERS, synth_count);
        stats.time = test_start.elapsed().as_secs_f64();
        {
            let snapshot = test_metrics.take_snapshot();

            stats.accepted = snapshot.get_counter(METRIC_ACCEPTED) as u16;
            stats.terminated = snapshot.get_counter(METRIC_TERMINATED) as u16;
            stats.rejected = snapshot.get_counter(METRIC_REJECTED) as u16;
            stats.conn_error = snapshot.get_counter(METRIC_ERROR) as u16;

            stats.timed_out = synth_count - stats.accepted - stats.rejected - stats.conn_error;
        }
        all_stats.push(stats);
    }

    // TODO: assertions with expected test results.

    node.stop().expect("unable to stop the node");

    // Display results table
    println!("\r\n{}", fmt_table(Table::new(&all_stats)));
}

async fn simulate_peer(node_addr: SocketAddr, handshake_complete: Sender<()>, thread_num: usize) {
    let mut config = TestConfig::default();

    // If there is address for our thread in the pool we can use it.
    // Otherwise we'll not set bound_addr and use local IP addr (127.0.0.1).
    if IPS.len() > thread_num {
        // We can safely use the same port as every thread will use different IP.
        let source_addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::from_str(IPS[thread_num]).unwrap()),
            CONNECTION_PORT,
        );
        config.pea2pea_config.bound_addr = Some(source_addr);
    }

    let mut synth_node = SyntheticNode::new(&config).await;

    // Establish peer connection
    let handshake_result = synth_node.connect(node_addr).await;
    handshake_complete.send(()).await.unwrap();
    match handshake_result {
        Ok(_) => {
            metrics::counter!(METRIC_ACCEPTED, 1);
        }
        Err(_err) => {
            metrics::counter!(METRIC_REJECTED, 1);
            return;
        }
    };

    // Keep connection alive by consuming messages
    loop {
        match synth_node
            .recv_message_timeout(Duration::from_millis(100))
            .await
        {
            Ok(_) => continue, // consume every message ignoring it
            Err(_timeout) => {
                // check for broken connection
                if !synth_node.is_connected(node_addr) {
                    metrics::counter!(METRIC_TERMINATED, 1);
                    synth_node.shut_down().await;
                    return;
                }
            }
        }
    }
}
