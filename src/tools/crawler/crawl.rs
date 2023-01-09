//! Contains structs and methods to crawl ripple network according to instruction at https://xrpl.org/peer-crawler.html

use std::{
    net::{IpAddr, SocketAddr},
    num::ParseIntError,
    time::Duration,
};

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use thiserror::Error;
use tokio::time::Instant;

const CRAWLER_DEFAULT_PORT: u16 = 51235;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(super) struct Peer {
    pub(super) complete_ledgers: Option<String>,
    pub(super) complete_shards: Option<String>,
    pub(super) ip: Option<String>,
    pub(super) port: Option<Port>,
    pub(super) public_key: String,
    #[serde(rename = "type")]
    pub(super) connection_type: String,
    pub(super) uptime: u32,
    pub(super) version: String,
}

impl Peer {
    /// Returns port number for the peer. If no port was present in the response then returns the default port.
    /// Returns [ParseIntError] if the response contained invalid value.
    pub(super) fn port(&self) -> Result<u16, ParseIntError> {
        self.port
            .as_ref()
            .map(|p| match p {
                Port::Number(n) => Ok(*n),
                Port::String(s) => s.parse(),
            })
            .unwrap_or(Ok(CRAWLER_DEFAULT_PORT))
    }
}

#[derive(Deserialize)]
pub(super) struct CrawlResponse {
    pub(super) overlay: Overlay,
    pub(super) server: Server,
}

#[derive(Deserialize)]
pub(super) struct Overlay {
    pub(super) active: Vec<Peer>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
pub(super) struct Server {
    pub(super) build_version: String,
    pub(super) server_state: String,
    pub(super) uptime: u32,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum Port {
    Number(u16),
    String(String),
}

/// Connects to `https://IP:PORT/crawl` to query `addr's` peers.
/// On success returns the response and the time it took to connect, send the request and read the response.
/// On failure it returns a [CrawlError].
pub(super) async fn get_crawl_response(
    client: Client,
    addr: SocketAddr,
) -> Result<(CrawlResponse, Duration), CrawlError> {
    let url = format!("https://{}:{}/crawl", format_ip_for_url(addr), addr.port());
    let start = Instant::now();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| CrawlError::Connection(e.to_string()))?;
    let elapsed = start.elapsed();
    if response.status() == StatusCode::OK {
        let response = response
            .json::<CrawlResponse>()
            .await
            .map_err(|e| CrawlError::Response(e.to_string()))?;
        Ok((response, elapsed))
    } else {
        Err(CrawlError::Response(format!(
            "status: {}",
            response.status()
        )))
    }
}

/// Formats ip address to be used in http url format.
/// That means that IPv6 address is wrapped in []
fn format_ip_for_url(addr: SocketAddr) -> String {
    if let IpAddr::V6(ip) = addr.ip() {
        format!("[{}]", ip)
    } else {
        addr.ip().to_string()
    }
}

#[derive(Debug, Error)]
pub(super) enum CrawlError {
    #[error("unable to connect: {0}")]
    Connection(String),

    #[error("invalid response: {0}")]
    Response(String),
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn should_return_error_for_invalid_port() {
        let peer = Peer {
            complete_ledgers: None,
            complete_shards: None,
            ip: None,
            port: Some(Port::String("not valid".into())),
            public_key: "".to_string(),
            connection_type: "".to_string(),
            uptime: 0,
            version: "".to_string(),
        };
        assert!(peer.port().is_err());
    }
}
