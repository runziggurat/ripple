use std::{collections::HashMap, net::IpAddr, sync::Arc, time::Duration};

use spectre::{edge::Edge, graph::Graph};
use ziggurat_core_crawler::summary::NetworkSummary;

use crate::network::{KnownNetwork, KnownNode};

/// The elapsed time before a connection should be regarded as inactive.
pub const LAST_SEEN_CUTOFF: u64 = 10 * 60;

#[derive(Default)]
pub struct NetworkMetrics {
    graph: Graph<IpAddr>,
}

impl NetworkMetrics {
    /// Updates the network graph with new connections.
    pub(super) async fn update_graph(&mut self, known_network: Arc<KnownNetwork>) {
        for connection in known_network.connections().await {
            let edge = Edge::new(connection.a, connection.b);
            if connection.last_seen.elapsed().as_secs() > LAST_SEEN_CUTOFF {
                self.graph.remove(&edge);
            } else {
                self.graph.insert(edge);
            }
        }
    }
}

/// Builds a new [NetworkSummary] out of current state of [KnownNetwork]
pub(super) async fn new_network_summary(
    known_network: Arc<KnownNetwork>,
    metrics: &mut NetworkMetrics,
    crawler_runtime: Duration,
) -> NetworkSummary {
    let nodes = known_network.nodes().await;
    let connections = known_network.connections().await;
    let good_nodes = get_good_nodes(&nodes);
    let server_versions = get_server_versions(&nodes);

    let indices = metrics
        .graph
        .get_filtered_adjacency_indices(&good_nodes.keys().copied().collect());

    NetworkSummary {
        num_known_nodes: nodes.len(),
        num_good_nodes: good_nodes.len(),
        num_known_connections: connections.len(),
        node_ips: good_nodes.iter().map(|n| n.0.to_string()).collect(),
        user_agents: server_versions,
        crawler_runtime,
        indices,
        ..Default::default()
    }
}

fn get_server_versions(nodes: &HashMap<IpAddr, KnownNode>) -> HashMap<String, usize> {
    nodes.iter().fold(HashMap::new(), |mut map, (_, node)| {
        node.server.clone().map(|version| {
            map.entry(version)
                .and_modify(|count| *count += 1)
                .or_insert(1)
        });
        map
    })
}

fn get_good_nodes(nodes: &HashMap<IpAddr, KnownNode>) -> HashMap<IpAddr, KnownNode> {
    nodes
        .iter()
        .filter_map(|(addr, node)| {
            node.last_connected
                .filter(|last| last.elapsed().as_secs() < LAST_SEEN_CUTOFF)
                .map(|_| (*addr, node.clone()))
        })
        .collect()
}
