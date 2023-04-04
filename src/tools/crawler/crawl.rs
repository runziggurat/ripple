//! Contains structs and methods to crawl ripple network according to instruction at https://xrpl.org/peer-crawler.html

use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use thiserror::Error;
use tokio::time::Instant;

/// Each member of the overlay active array is an object with the following fields.
#[derive(Debug, Deserialize, Clone)]
pub struct Peer {
    /// The range of ledger versions this peer has available.
    pub complete_ledgers: Option<String>,

    /// The range of ledger history shards this peer has available.
    pub complete_shards: Option<String>,

    /// The IP address of this connected peer.
    ///
    /// Omitted if the peer is configured as a validator or a private peer.
    pub ip: Option<String>,

    /// The port number on the peer server that serves RTXP.
    ///
    /// Typically 51235.
    /// Omitted if the peer is configured as a validator or a private peer.
    pub port: Option<Port>,

    /// The public key of the ECDSA key pair used by this peer to sign RTXP messages.
    pub public_key: String,

    /// Indicating whether the TCP connection to the peer is incoming or outgoing.
    ///
    /// The value is "in" or "out".
    #[serde(rename = "type")]
    pub connection_type: String,

    /// The number of seconds the server has been connected to this peer.
    #[serde(rename = "uptime")]
    pub connection_uptime: u32,

    /// The rippled version number the peer reports to be using.
    pub version: String,
}

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ip = self.ip.clone().unwrap_or_default();
        let port = self.port.clone().unwrap_or_default();
        let public_key = &self.public_key;
        let uptime = &self.connection_uptime;

        writeln!(f, "[{uptime:05}]: {ip}:{port}\t\t\t{public_key}")?;
        writeln!(
            f,
            "[{:3}]: \t\t\t\tversion: {:20} \t\tshards: {:?} ledgers: {:?}",
            self.connection_type, self.version, self.complete_shards, self.complete_ledgers
        )
    }
}

impl Peer {
    /// Returns port number for the peer.
    pub fn port(&self) -> Option<u16> {
        self.port.as_ref().and_then(|p| match p {
            Port::Number(n) => Some(*n),
            Port::String(s) => s.parse().ok(),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CrawlResponse {
    /// Information about the peer servers currently connected to this one,
    /// similar to the response from the peers method.
    #[serde(rename = "overlay")]
    pub peerlist: Overlay,

    /// Information about this server.
    pub server: Server,
}

impl fmt::Display for CrawlResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, " server: {}", self.server)?;
        writeln!(f, " peerlist:\n{}", self.peerlist)
    }
}

#[derive(Debug, Deserialize)]
pub struct Overlay {
    pub active: Vec<Peer>,
}

impl fmt::Display for Overlay {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Sort the peers here by the IP address.
        let mut peers = self.active.clone();
        peers.sort_by(|a, b| {
            let a_ip = a.ip.clone().unwrap_or_default();
            let b_ip = b.ip.clone().unwrap_or_default();

            a_ip.cmp(&b_ip)
        });

        for peer in peers {
            writeln!(f, "{peer}")?;
        }
        writeln!(f)
    }
}

/// Information about this server.
#[derive(Debug, Deserialize)]
pub struct Server {
    /// The version number of the running rippled version.
    pub build_version: String,

    ///  A string indicating to what extent the server is participating in the network.
    pub server_state: String,

    /// Number of consecutive seconds that the server has been operational.
    pub uptime: u32,
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "build_version: {}, ", self.build_version)?;
        write!(f, "server_state: {}, ", self.server_state)?;
        write!(f, "server_uptime: {}", self.uptime)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Port {
    Number(u16),
    String(String),
}

impl Default for Port {
    fn default() -> Self {
        Self::String("".to_owned())
    }
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let port_str = match self {
            Self::Number(num) => num.to_string(),
            Self::String(string) => string.clone(),
        };
        write!(f, "{port_str}")
    }
}

/// Connects to `https://IP:PORT/crawl` to query `addr's` peers.
/// On success returns the response and the time it took to connect, send the request and read the response.
/// On failure it returns a [CrawlError].
pub async fn get_crawl_response(
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
        format!("[{ip}]")
    } else {
        addr.ip().to_string()
    }
}

#[derive(Debug, Error)]
pub enum CrawlError {
    #[error("unable to connect: {0}")]
    Connection(String),

    #[error("invalid response: {0}")]
    Response(String),
}

#[cfg(test)]
mod test {
    use super::*;

    const PORT_STRING: &str = "20";
    const PORT_NUMBER: u16 = 20;

    #[test]
    fn should_return_empty_for_invalid_port() {
        let peer = Peer {
            complete_ledgers: None,
            complete_shards: None,
            ip: None,
            port: Some(Port::String("not valid".into())),
            public_key: "".to_string(),
            connection_type: "".to_string(),
            connection_uptime: 0,
            version: "".to_string(),
        };
        assert!(peer.port().is_none());
    }

    #[test]
    fn should_return_some_for_string_port() {
        let peer = Peer {
            complete_ledgers: None,
            complete_shards: None,
            ip: None,
            port: Some(Port::String(PORT_STRING.into())),
            public_key: "".to_string(),
            connection_type: "".to_string(),
            connection_uptime: 0,
            version: "".to_string(),
        };
        assert!(matches!(peer.port(), Some(PORT_NUMBER)));
    }

    #[test]
    fn should_return_some_for_number_port() {
        let peer = Peer {
            complete_ledgers: None,
            complete_shards: None,
            ip: None,
            port: Some(Port::Number(PORT_NUMBER)),
            public_key: "".to_string(),
            connection_type: "".to_string(),
            connection_uptime: 0,
            version: "".to_string(),
        };
        assert!(matches!(peer.port(), Some(PORT_NUMBER)));
    }
}
