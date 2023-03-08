//! Development test suite
//!
//! Some helper tools:
//!  - Change process niceness:
//!     sudo renice -n -19 -p $(pidof rippled)
//!
use std::{net::SocketAddr, path::Path};

use chrono::{DateTime, Utc};
use reqwest::Client;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};
use ziggurat_core_utils::err_constants::{
    ERR_NODE_BUILD, ERR_SYNTH_CONNECT, ERR_SYNTH_START_LISTENING, ERR_TEMPDIR_NEW,
};

use crate::{
    setup::node::{Node, NodeType},
    tools::{
        config::SynthNodeCfg,
        crawl,
        synth_node::{self, SyntheticNode},
    },
};

#[derive(Default)]
enum NodeLogToStdout {
    #[default]
    Off,
    On,
}

impl NodeLogToStdout {
    fn is_on(self) -> bool {
        match self {
            NodeLogToStdout::Off => false,
            _ => true,
        }
    }
}

#[derive(PartialEq, Default)]
enum TracingOpt {
    #[default]
    Off,
    On,
}

#[derive(Default)]
#[allow(non_camel_case_types)]
enum SynthNodeOpt {
    #[default]
    Off,
    On_OnlyListening(SynthNodeCfg),
    On_TryToConnect(SynthNodeCfg),
}

#[derive(Default)]
enum PeriodicCrawlOpt {
    #[default]
    Off,
    On(Duration),
}

/// A simple configuration for the dev test customization.
#[derive(Default)]
struct DevTestCfg {
    /// Print the node's log to the stdout.
    log_to_stdout: NodeLogToStdout,

    /// Enable tracing.
    tracing: TracingOpt,

    /// Print out the crawl response periodically.
    crawl: PeriodicCrawlOpt,

    /// Attach a synthetic node to the node.
    synth_node: SynthNodeOpt,
}

#[tokio::test]
#[allow(non_snake_case)]
#[ignore = "convenience test to tinker with a running node for dev purposes"]
async fn dev001_t1_RUN_NODE_FOREVER_with_logs() {
    // This test is used for testing/development purposes.

    let cfg = DevTestCfg {
        log_to_stdout: NodeLogToStdout::On,
        tracing: TracingOpt::On,
        ..Default::default()
    };
    node_run_forever(cfg).await;

    panic!("the node shouldn't have died");
}

#[tokio::test]
#[allow(non_snake_case)]
#[ignore = "convenience test to tinker with a running node for dev purposes"]
async fn dev001_t2_RUN_NODE_FOREVER_no_logs() {
    // This test is used for testing/development purposes.

    let mut cfg = DevTestCfg::default();
    cfg.crawl = PeriodicCrawlOpt::On(Duration::from_secs(2));
    node_run_forever(cfg).await;

    panic!("the node shouldn't have died");
}

#[tokio::test]
#[allow(non_snake_case)]
#[ignore = "convenience test to tinker with a running node for dev purposes"]
async fn dev002_t1_MONITOR_NODE_FOREVER_WITH_SYNTH_NODE_sn_is_conn_initiator() {
    // This test is used for testing/development purposes.

    let mut cfg = DevTestCfg::default();
    cfg.tracing = TracingOpt::On;
    cfg.crawl = PeriodicCrawlOpt::On(Duration::from_secs(5));
    cfg.synth_node = SynthNodeOpt::On_TryToConnect(SynthNodeCfg::default());
    node_run_forever(cfg).await;

    panic!("the node shouldn't have died");
}

#[tokio::test]
#[allow(non_snake_case)]
#[ignore = "convenience test to tinker with a running node for dev purposes"]
async fn dev002_t2_MONITOR_NODE_FOREVER_WITH_SYNTH_NODE_sn_is_conn_responder() {
    // This test is used for testing/development purposes.

    let mut cfg = DevTestCfg::default();
    cfg.log_to_stdout = NodeLogToStdout::On;
    cfg.tracing = TracingOpt::On;
    cfg.crawl = PeriodicCrawlOpt::On(Duration::from_secs(5));
    cfg.synth_node = SynthNodeOpt::On_OnlyListening(SynthNodeCfg::default());
    node_run_forever(cfg).await;

    panic!("the node shouldn't have died");
}

/// Runs the node forever!
/// The test asserts the node process won't be killed.
///
/// Function complexity is increased due to many customization options,
/// which is not nice but it is what it is.
///
/// In short, here are the customization options which are provided via the cfg arg:
///
///  - enable/disable node's logs to stdout [cfg.log_to_stdout]
///
///  - enable/disable tracing [cfg.tracing]
///
///  - enable/disable printing crawler response periodically [cfg.crawl]
///    - suboption: the duration of the period
///
///  - enable/disable attaching a single synthetic node to the node [cfg.synth_node]
///    - suboption: choose the initiator for the connection
///    - SyntheticNode's TestConfig configuration is customizable
///
async fn node_run_forever(cfg: DevTestCfg) {
    let target = TempDir::new().expect(ERR_TEMPDIR_NEW);
    let log_to_stdout = cfg.log_to_stdout.is_on();

    // Enable tracing possibly.
    if cfg.tracing == TracingOpt::On {
        synth_node::enable_tracing();
    }

    // SyntheticNode is spawned only if option is chosen in cfg options.
    let mut initial_peers = vec![];
    let mut synth_node: Option<SyntheticNode> = match cfg.synth_node {
        SynthNodeOpt::On_TryToConnect(cfg) => Some(SyntheticNode::new(&cfg).await),
        SynthNodeOpt::On_OnlyListening(cfg) => {
            let sn = SyntheticNode::new(&cfg).await;
            let listening_addr = sn.start_listening().await.expect(ERR_SYNTH_START_LISTENING);
            initial_peers.push(listening_addr);
            Some(sn)
        }
        _ => None,
    };

    let mut node = node_start(target.path(), log_to_stdout, initial_peers.clone()).await;
    let addr = node.addr();

    if let Some(synth_node) = synth_node.as_ref() {
        // Alternative check to the On_TryToConnect option.
        if initial_peers.is_empty() {
            synth_node.connect(addr).await.expect(ERR_SYNTH_CONNECT);
        }
    }

    // Print received messages from another thread.
    if let Some(synth_node) = synth_node.take() {
        spawn_periodic_msg_recv(synth_node).await;
    }

    // Enable crawler possibly from another thread.
    if let PeriodicCrawlOpt::On(period) = cfg.crawl {
        // Periodic crawler prints out the crawl response every n seconds.
        spawn_periodic_crawler(addr, period).await;
    }

    // The node should run forever unless something bad happens to it.
    node.wait_until_exit().await;

    println!("\tThe node has stopped running ({})", current_time_str());
}

/// Create and start the node and print the extra useful debug info.
async fn node_start(path: &Path, log_to_stdout: bool, initial_peers: Vec<SocketAddr>) -> Node {
    println!("\tTime before the node is started: {}", current_time_str());

    let node = Node::builder()
        .log_to_stdout(log_to_stdout)
        .initial_peers(initial_peers.clone())
        .start(path, NodeType::Stateless)
        .await
        .expect(ERR_NODE_BUILD);

    println!("\tThe node directory files are located at {path:?}");
    println!("\tThe node has started running ({})", current_time_str());
    println!("\tInitial peers: {initial_peers:?}");
    println!("\tThe node is listening on {}", node.addr());

    if !log_to_stdout {
        let log_path = path.join("rippled/debug.log");
        println!("\tThe node logs can be found at {log_path:?}");
    }

    node
}

/// Use recv_message to clear up the inbound queue and print out
/// the received messages.
async fn spawn_periodic_msg_recv(mut synth_node: SyntheticNode) {
    tokio::spawn(async move {
        loop {
            let (_from_addr, msg) = synth_node.recv_message().await;

            let payload = msg.payload;
            tracing::info!("message received: {payload:?}");
        }
    });
}

/// Periodic crawler prints out the crawl response every n seconds.
async fn spawn_periodic_crawler(addr: SocketAddr, period: Duration) {
    tokio::spawn(async move {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(1))
            .build()
            .expect("unable to build the web client");

        loop {
            let (rsp, _duration) = crawl::get_crawl_response(client.clone(), addr)
                .await
                .expect("couldn't get the crawl response");

            println!("{rsp}\n");
            sleep(period).await;
        }
    });
}

fn current_time_str() -> String {
    let now: DateTime<Utc> = Utc::now();
    now.format("%T %a %b %e %Y").to_string()
}
