use std::{net::SocketAddr, time::Duration};

use tabled::{Table, Tabled};
use tempfile::TempDir;
use tokio::sync::mpsc::Sender;

use crate::{
    setup::node::{Node, NodeType},
    tools::{
        config::TestConfig,
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

const METRIC_ACCEPTED: &str = "perf_conn_accepted";
const METRIC_TERMINATED: &str = "perf_conn_terminated";
const METRIC_REJECTED: &str = "perf_conn_rejected";
const METRIC_ERROR: &str = "perf_conn_error";

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn p002_connections_load_() {
    // ZG-PERFORMANCE-002
    //
    // The node sheds or rejects connections when necessary.
    //
    //  1. Start a node with max_peers set to `N`
    //  2. Initiate connections from `M > N` peer nodes
    //  3. Expect only `N` to be active at a time
    //

    // maximum time allowed for a single iteration of the test
    const MAX_ITER_TIME: Duration = Duration::from_secs(20);

    /// maximum peers to configure node with
    const MAX_PEERS: u16 = 50;

    //let synth_counts = vec![100u16, 1_000, 5_000, 10_000, 15_000, 20_000];
    let synth_counts = vec![100u16];

    let mut all_stats = Vec::new();

    let target = TempDir::new().expect("Unable to create TempDir");
    // start node
    let mut node = Node::builder()
        .max_peers(MAX_PEERS as usize)
        .start(target.path(), NodeType::Stateless)
        .await
        .unwrap();
    let node_addr = node.addr();

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

        let test_start = tokio::time::Instant::now();

        // start synthetic nodes
        for _ in 0..synth_count {
            let node_addr = node.addr();

            let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
            synth_exits.push(exit_tx);

            let synth_handshaken = handshake_tx.clone();
            // Synthetic node runs until it completes or is instructed to exit
            synth_handles.push(tokio::spawn(async move {
                tokio::select! {
                    _ = exit_rx => {},
                    _ = simulate_peer(node_addr, synth_handshaken) => {},
                };
            }));
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

    node.stop().unwrap();

    // Display results table
    println!("{}", fmt_table(Table::new(&all_stats)));
}

async fn simulate_peer(node_addr: SocketAddr, handshake_complete: Sender<()>) {
    let config = TestConfig::default();
    let mut synth_node = SyntheticNode::new(&config).await;

    // Establish peer connection
    let handshake_result = synth_node.connect(node_addr).await;
    handshake_complete.send(()).await.unwrap();
    match handshake_result {
        Ok(stream) => {
            metrics::counter!(METRIC_ACCEPTED, 1);
            stream
        }
        Err(_err) => {
            metrics::counter!(METRIC_REJECTED, 1);
            return;
        }
    };

    // Keep connection alive by replying to incoming Pings etc,
    // and check for terminated connection.
    //
    loop {
        match synth_node
            .recv_message_timeout(Duration::from_millis(300))
            .await
        {
            Ok((_, message)) => continue,   // consume every message ignoring it
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
