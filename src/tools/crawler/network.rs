use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    sync::RwLock,
    time::{sleep, Instant},
};
use tracing::debug;
use ziggurat_core_crawler::{connection::KnownConnection, summary::NetworkSummary};

use crate::metrics::{new_network_summary, NetworkMetrics};

const SUMMARY_LOOP_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct KnownNetwork {
    nodes: RwLock<HashMap<SocketAddr, KnownNode>>,
    connections: RwLock<HashSet<KnownConnection>>,
}

impl KnownNetwork {
    /// Inserts addr to known_nodes if not yet present (so to avoid overriding the node's statistics).
    /// Returns true if it's a new node, false otherwise.
    pub(super) async fn new_node(&self, addr: SocketAddr) -> bool {
        let mut nodes = self.nodes.write().await;
        if let Entry::Vacant(e) = nodes.entry(addr) {
            e.insert(KnownNode::default());
            debug!("Known nodes: {}", nodes.len());
            true
        } else {
            false
        }
    }

    /// Inserts connection from `from` to `peers`.
    pub(super) async fn insert_connections(&self, from: SocketAddr, peers: &[SocketAddr]) {
        let mut connections = self.connections.write().await;
        peers.iter().for_each(|peer| {
            connections.insert(KnownConnection::new(from, *peer));
        });
    }

    /// Updates stats for `peer`.
    pub(super) async fn update_stats(
        &self,
        peer: SocketAddr,
        connecting_time: Duration,
        server_version: String,
    ) {
        let mut nodes = self.nodes.write().await;
        let mut node = nodes.get_mut(&peer).unwrap();
        node.last_connected = Some(Instant::now());
        node.connection_failures = 0;
        node.connecting_time = Some(connecting_time);
        node.server = Some(server_version);
    }

    /// Increases connection failures to the `addr` and returns its new value.
    pub(super) async fn increase_connection_failures(&self, addr: SocketAddr) -> u8 {
        let mut nodes = self.nodes.write().await;
        let mut node = nodes.get_mut(&addr).unwrap();
        node.connection_failures = node.connection_failures.saturating_add(1);
        node.connection_failures
    }

    pub(super) async fn set_handshake_successful(&self, addr: SocketAddr, success: bool) {
        let mut nodes = self.nodes.write().await;
        let mut node = nodes.get_mut(&addr).unwrap();
        node.handshake_successful = success;
    }

    /// Returns a snapshot of the known connections.
    pub async fn connections(&self) -> HashSet<KnownConnection> {
        self.connections.read().await.clone()
    }

    /// Returns a snapshot of the known nodes.
    pub async fn nodes(&self) -> HashMap<SocketAddr, KnownNode> {
        self.nodes.read().await.clone()
    }
}

pub(super) async fn update_summary_snapshot_task(
    known_network: Arc<KnownNetwork>,
    summary_snapshot: Arc<Mutex<NetworkSummary>>,
) {
    let start_time = Instant::now();
    let mut network_metrics = NetworkMetrics::default();
    loop {
        sleep(SUMMARY_LOOP_INTERVAL).await;
        network_metrics.update_graph(known_network.clone()).await;
        let new_network_summary = new_network_summary(
            known_network.clone(),
            &mut network_metrics,
            start_time.elapsed(),
        )
        .await;
        *summary_snapshot
            .lock()
            .expect("unable to take `summary_snapshot` lock") = new_network_summary;
    }
}

/// A node encountered in the network or obtained from one of the peers.
#[derive(Debug, Default, Clone)]
pub struct KnownNode {
    // // The address is omitted, as it's a key in the owning HashMap.
    /// The last time the node was successfully connected to.
    pub last_connected: Option<Instant>,
    /// The time it took to complete a connection.
    pub connecting_time: Option<Duration>,
    /// The node's server version.
    pub server: Option<String>,
    /// The number of subsequent connection errors.
    pub connection_failures: u8,
    /// Status for binary protocol connection/handshake attempt.
    pub handshake_successful: bool,
}
