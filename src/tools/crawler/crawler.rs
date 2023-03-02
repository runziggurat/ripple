use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use futures_util::{future::BoxFuture, FutureExt};
use pea2pea::protocols::Handshake;
use rand::Rng;
use reqwest::Client;
use tokio::time::sleep;
use tracing::{debug, trace, warn};
use ziggurat_xrpl::tools::inner_node::InnerNode;

use crate::{
    crawl::{get_crawl_response, CrawlResponse, Peer},
    network::KnownNetwork,
};
const CRAWLER_DEFAULT_PORT: u16 = 51235;
const PROTOCOL_DEFAULT_PORT: u16 = 2459;

const CONNECTION_RETRY_MIN_SEC: u64 = 3 * 60; // 3 minutes
const CONNECTION_RETRY_MAX_SEC: u64 = 5 * 60; // 5 minutes

pub(super) struct Crawler {
    pub(super) known_network: Arc<KnownNetwork>,
}

impl Crawler {
    pub(super) async fn new() -> Self {
        Self {
            known_network: Default::default(),
        }
    }
}

/// Spawns a tokio's task to crawl given address. After receiving the response it will
/// process it and start more crawl tasks recursively.
pub(super) fn crawl(
    client: Client,
    ip: IpAddr,
    port: Option<u16>,
    known_network: Arc<KnownNetwork>,
) -> BoxFuture<'static, ()> {
    // Wrapped in box to allow for async recursion.
    async move {
        tokio::spawn(async move {
            if !known_network
                .new_node(SocketAddr::new(ip, port.unwrap_or(CRAWLER_DEFAULT_PORT)))
                .await
            {
                trace!("Skip crawling a known node {ip}");
                return;
            }

            trace!("Crawling {ip}");
            let ports = get_ports_to_try(port);
            loop {
                let mut success = false;
                for port in &ports {
                    // TODO(team): decide how to use this information about the handshake_successful data
                    tokio::spawn(try_handshake(
                        SocketAddr::new(ip, *port),
                        known_network.clone(),
                    ));
                    success = try_crawling(client.clone(), ip, *port, known_network.clone()).await;
                    if success {
                        break;
                    }
                }
                if !success {
                    let failures = known_network
                        .increase_connection_failures(SocketAddr::new(
                            ip,
                            port.unwrap_or(CRAWLER_DEFAULT_PORT),
                        ))
                        .await;
                    if failures == u8::MAX {
                        warn!("Giving up connecting to {ip}");
                        break;
                    }
                }

                // Even if connection was successful - try again after a while to update peers.
                let duration = rand::thread_rng()
                    .gen_range(CONNECTION_RETRY_MIN_SEC..=CONNECTION_RETRY_MAX_SEC);
                sleep(Duration::from_secs(duration)).await;
            }
        });
    }
    .boxed()
}

fn get_ports_to_try(from_response: Option<u16>) -> HashSet<u16> {
    let mut ports = HashSet::new();
    if let Some(port) = from_response {
        ports.insert(port);
    } else {
        ports.insert(CRAWLER_DEFAULT_PORT);
        ports.insert(PROTOCOL_DEFAULT_PORT);
    }
    ports
}

async fn try_handshake(addr: SocketAddr, known_network: Arc<KnownNetwork>) {
    let (sender, _receiver) = tokio::sync::mpsc::channel(1024);
    let node = InnerNode::new(&Default::default(), sender).await;
    node.enable_handshake().await;

    let result = node.connect(addr).await.is_ok();
    known_network.set_handshake_successful(addr, result).await;
    if result {
        trace!("Successful handshake to {}", addr);
    } else {
        trace!("Unsuccessful handshake to {}", addr);
    }
    node.shut_down().await;
}

async fn try_crawling(
    client: Client,
    ip: IpAddr,
    port: u16,
    known_network: Arc<KnownNetwork>,
) -> bool {
    match get_crawl_response(client.clone(), SocketAddr::new(ip, port)).await {
        Ok((response, connecting_time)) => {
            let addresses = extract_known_nodes(&response).await;
            known_network
                .update_stats(
                    SocketAddr::new(ip, port),
                    connecting_time,
                    response.server.build_version,
                )
                .await;
            let peers = addresses
                .iter()
                .map(|(ip, port)| SocketAddr::new(*ip, port.unwrap_or(CRAWLER_DEFAULT_PORT)))
                .collect::<Vec<_>>();
            known_network
                .insert_connections(SocketAddr::new(ip, port), &peers)
                .await;
            for (ip, port) in addresses {
                crawl(client.clone(), ip, port, known_network.clone()).await;
            }
            true
        }
        Err(e) => {
            debug!("Unable to get crawl response from {}: {:?}", ip, e);
            false
        }
    }
}

/// Extract addresses from /crawl response.
async fn extract_known_nodes(response: &CrawlResponse) -> Vec<(IpAddr, Option<u16>)> {
    response
        .peerlist
        .active
        .iter()
        .filter_map(parse_peer_addr)
        .collect()
}

/// Tries to parse address information from response.
/// On success returns optional tuple of Ip address, and optional port.
fn parse_peer_addr(peer: &Peer) -> Option<(IpAddr, Option<u16>)> {
    let ip = peer.ip.as_ref()?.parse().ok()?;
    Some((ip, peer.port()))
}
