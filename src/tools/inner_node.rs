use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use openssl::ssl::{SslAcceptor, SslConnector, SslMethod, SslVerifyMode};
use pea2pea::{
    protocols::{Handshake, Reading, Writing},
    Node, Pea2Pea,
};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use tokio::sync::mpsc::Sender;

use crate::{protocol::codecs::binary::BinaryMessage, tools::tls_cert};

/// Enables tracing for all [`SyntheticNode`] instances (usually scoped by test).
pub fn enable_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    fmt()
        .with_test_writer()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

// A synthetic node adhering to Ripple's network protocol.
#[derive(Clone)]
pub struct InnerNode {
    node: Node,
    pub(crate) sender: Sender<(SocketAddr, BinaryMessage)>,
    pub crypto: Arc<Crypto>,
    pub tls: Tls,
}

// An object cointaining TLS handlers.
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
    pub async fn new(config: pea2pea::Config, sender: Sender<(SocketAddr, BinaryMessage)>) -> Self {
        // generate the keypair and prepare the crypto engine

        let engine = Secp256k1::new();
        let (private_key, public_key) = engine.generate_keypair(&mut secp256k1::rand::thread_rng());
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
        let node = Self {
            node: Node::new(config).await.unwrap(),
            sender,
            crypto,
            tls: Tls {
                acceptor,
                connector,
            },
        };
        node.enable_handshake().await;
        node.enable_reading().await;
        node.enable_writing().await;
        node
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

    /// Gracefully shuts down the node.
    pub async fn shut_down(&self) {
        self.node.shut_down().await
    }
}
