use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use openssl::ssl::{SslAcceptor, SslConnector, SslMethod, SslVerifyMode};
use pea2pea::{Node, Pea2Pea};
use secp256k1::{
    constants::{PUBLIC_KEY_SIZE, SECRET_KEY_SIZE},
    PublicKey, Secp256k1, SecretKey,
};
use tokio::{net::TcpSocket, sync::mpsc::Sender};

use crate::{
    protocol::{codecs::message::BinaryMessage, handshake::HandshakeCfg},
    setup::constants::{SYNTHETIC_NODE_PRIVATE_KEY, SYNTHETIC_NODE_PUBLIC_KEY},
    tools::{config::SynthNodeCfg, tls_cert},
};

// A synthetic node adhering to Ripple's network protocol.
#[derive(Clone)]
pub struct InnerNode {
    node: Node,
    pub(crate) sender: Sender<(SocketAddr, BinaryMessage)>,
    pub crypto: Arc<Crypto>,
    pub tls: Tls,
    pub handshake_cfg: Option<HandshakeCfg>,
}

// An object containing TLS handlers.
#[derive(Clone)]
pub struct Tls {
    pub acceptor: SslAcceptor,
    pub connector: SslConnector,
}

// An object dedicated to cryptographic functionalities.
pub struct Crypto {
    pub engine: Secp256k1<secp256k1::All>,
    pub private_key: SecretKey,
    pub public_key: PublicKey,
}

impl Pea2Pea for InnerNode {
    fn node(&self) -> &Node {
        &self.node
    }
}

impl InnerNode {
    pub async fn new(cfg: &SynthNodeCfg, sender: Sender<(SocketAddr, BinaryMessage)>) -> Self {
        // generate the keypair and prepare the crypto engine

        let engine = Secp256k1::new();
        let (private_key, public_key) = if cfg.generate_new_keys {
            engine.generate_keypair(&mut secp256k1::rand::thread_rng())
        } else {
            decode_predefined_keys().expect("invalid predefined keys")
        };
        let crypto = Arc::new(Crypto {
            engine,
            private_key,
            public_key,
        });

        // TLS acceptor

        let (_ca_cert, ca_key_pair) = tls_cert::mk_ca_cert().unwrap();
        let (cert, key_pair) = tls_cert::mk_ca_signed_cert(&ca_key_pair).unwrap();

        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_private_key(&key_pair).unwrap();
        acceptor.set_certificate(&cert).unwrap();
        let acceptor = acceptor.build();

        // TLS connector
        let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
        connector.set_verify(SslVerifyMode::NONE); // we might remove it once the keypair is solid
        let connector = connector.build();

        // the node
        Self {
            node: Node::new(cfg.pea2pea_config.clone()),
            sender,
            crypto,
            tls: Tls {
                acceptor,
                connector,
            },
            handshake_cfg: cfg.handshake.clone(),
        }
    }

    pub fn is_connected_ip(&self, ip: IpAddr) -> bool {
        self.node()
            .connected_addrs()
            .iter()
            .any(|addr| addr.ip() == ip)
    }

    /// Connects to the target address.
    pub async fn connect(&self, target: SocketAddr) -> io::Result<()> {
        self.node.connect(target).await?;
        Ok(())
    }

    /// Connects to the target address.
    pub async fn connect_from(&self, target: SocketAddr, socket: TcpSocket) -> io::Result<()> {
        self.node.connect_using_socket(target, socket).await
    }

    /// Gracefully shuts down the node.
    pub async fn shut_down(&self) {
        self.node.shut_down().await
    }
}

fn decode_to_vec(base58str: &str, size: usize) -> bs58::decode::Result<Vec<u8>> {
    let mut bytes = bs58::decode(base58str)
        .with_alphabet(bs58::Alphabet::RIPPLE)
        .into_vec()?;
    // Remove the first byte as it's an extra byte added before serialization to distinguish
    // hashes of different things (i.e. public/private keys, accounts, transactions and so on).
    bytes.remove(0);
    bytes.truncate(size);
    Ok(bytes)
}

fn decode_predefined_keys() -> Result<(SecretKey, PublicKey), secp256k1::Error> {
    let bytes = decode_to_vec(SYNTHETIC_NODE_PRIVATE_KEY, SECRET_KEY_SIZE)
        .expect("unable to decode the private key");
    let private_key = SecretKey::from_slice(bytes.as_slice())?;

    let bytes = decode_to_vec(SYNTHETIC_NODE_PUBLIC_KEY, PUBLIC_KEY_SIZE)
        .expect("unable to decode the public key");
    let public_key = PublicKey::from_slice(bytes.as_slice())?;

    Ok((private_key, public_key))
}
