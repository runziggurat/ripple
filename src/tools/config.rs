use std::net::{IpAddr, Ipv4Addr};

use crate::protocol::handshake::HandshakeCfg;

/// Synthetic Node Configuration.
#[derive(Clone)]
pub struct SynthNodeCfg {
    /// Whether or not to generate new keys for a handshake.
    pub generate_new_keys: bool,

    /// Handshake configuration.
    ///
    /// If not set, the handshake will be skipped.
    pub handshake: Option<HandshakeCfg>,

    /// Pea2Pea configuration.
    pub pea2pea_config: pea2pea::Config,
}

impl Default for SynthNodeCfg {
    fn default() -> Self {
        let ip_addr = IpAddr::V4(Ipv4Addr::LOCALHOST);
        Self {
            generate_new_keys: true,
            handshake: Some(Default::default()),
            pea2pea_config: pea2pea::Config {
                listener_ip: Some(ip_addr),
                ..Default::default()
            },
        }
    }
}
