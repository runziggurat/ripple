/// The Ripple handshake implementation.
use std::{io, pin::Pin};

use bytes::Bytes;
use futures_util::{sink::SinkExt, TryStreamExt};
use openssl::ssl::Ssl;
use pea2pea::{protocols::Handshake, Connection, ConnectionSide, Pea2Pea};
use sha2::{Digest, Sha512};
use tokio_openssl::SslStream;
use tokio_util::codec::Framed;
use tracing::*;

use crate::{
    protocol::codecs::http::{HttpCodec, HttpMsg},
    tools::inner_node::{Crypto, InnerNode},
};

#[repr(u8)]
enum NodeType {
    Public = 28,
    #[allow(dead_code)]
    Private = 32,
}

// Used to populate the Public-Key field.
fn encode_base58(node_type: NodeType, public_key: &[u8]) -> String {
    let mut payload = Vec::with_capacity(1 + public_key.len());

    payload.push(node_type as u8);
    payload.extend_from_slice(public_key);

    bs58::encode(payload)
        .with_alphabet(bs58::Alphabet::RIPPLE)
        .with_check()
        .into_string()
}

// Used to populate the Session-Signature field.
fn create_session_signature(crypto: &Crypto, shared_value: &[u8]) -> String {
    let message = secp256k1::Message::from_slice(shared_value).unwrap();
    let signature = crypto.engine.sign_ecdsa(&message, &crypto.private_key);
    let serialized = signature.serialize_der();

    base64::encode(serialized)
}

// Used as input for create_session_signature.
fn get_shared_value<S>(tls_stream: &SslStream<S>) -> io::Result<Vec<u8>> {
    const MAX_FINISHED_SIZE: usize = 64;

    let mut finished = [0u8; MAX_FINISHED_SIZE];
    let finished_size = tls_stream.ssl().finished(&mut finished);
    let mut hasher = Sha512::new();
    hasher.update(&finished[..finished_size]);
    let finished_hash = hasher.finalize();

    let mut peer_finished = [0u8; MAX_FINISHED_SIZE];
    let peer_finished_size = tls_stream.ssl().peer_finished(&mut peer_finished);
    let mut hasher = Sha512::new();
    hasher.update(&peer_finished[..peer_finished_size]);
    let peer_finished_hash = hasher.finalize();

    let mut anded = [0u8; 64];
    for i in 0..64 {
        anded[i] = finished_hash[i] ^ peer_finished_hash[i];
    }

    let mut hasher = Sha512::new();
    hasher.update(anded);
    let hash = hasher.finalize()[..32].to_vec(); // the hash gets halved

    Ok(hash)
}

#[async_trait::async_trait]
impl Handshake for InnerNode {
    async fn perform_handshake(&self, mut conn: Connection) -> io::Result<Connection> {
        let own_conn_side = !conn.side();
        let stream = self.take_stream(&mut conn);
        let addr = conn.addr();

        let tls_stream = match own_conn_side {
            ConnectionSide::Initiator => {
                let ssl = self
                    .tls
                    .connector
                    .configure()
                    .unwrap()
                    .into_ssl("domain") // is SNI and hostname verification enabled?
                    .unwrap();
                let mut tls_stream = SslStream::new(ssl, stream).unwrap();

                Pin::new(&mut tls_stream).connect().await.map_err(|e| {
                    error!(parent: self.node().span(), "TLS handshake error: {}", e);
                    io::ErrorKind::InvalidData
                })?;

                // get the shared value based on the TLS handshake
                let shared_value = get_shared_value(&tls_stream)?;

                // base58-encode the public key and create the session signature
                let base58_pk =
                    encode_base58(NodeType::Public, &self.crypto.public_key.serialize()[..]);
                let sig = create_session_signature(&self.crypto, &shared_value);

                // prepare the HTTP request message
                let mut request = Vec::new();
                request.extend_from_slice(b"GET / HTTP/1.1\r\n");
                request.extend_from_slice(b"User-Agent: rippled-1.9.1\r\n");
                request.extend_from_slice(b"Connection: Upgrade\r\n");
                request.extend_from_slice(b"Upgrade: XRPL/2.0, XRPL/2.1, XRPL/2.2\r\n"); // TODO: which ones should we handle?
                request.extend_from_slice(b"Connect-As: Peer\r\n");
                request.extend_from_slice(format!("Public-Key: {}\r\n", base58_pk).as_bytes());
                request.extend_from_slice(format!("Session-Signature: {}\r\n\r\n", sig).as_bytes());
                let request = Bytes::from(request);

                // use the HTTP codec to read/write the (post-TLS) handshake messages
                let codec = HttpCodec::new(self.node().span().clone(), HttpMsg::Response);
                let mut framed = Framed::new(&mut tls_stream, codec);

                trace!(parent: self.node().span(), "sending a request to {}: {:?}", addr, request);

                // send the handshake HTTP request message
                framed.send(request).await?;

                // read the HTTP request message (there should only be headers)
                let response_body = framed.try_next().await?.ok_or(io::ErrorKind::InvalidData)?;
                if !response_body.is_empty() {
                    warn!(parent: self.node().span(), "trailing bytes in the handshake response from {}: {:?}", addr, response_body);
                }

                tls_stream
            }
            ConnectionSide::Responder => {
                let ssl = Ssl::new(self.tls.acceptor.context()).unwrap();
                let mut tls_stream = SslStream::new(ssl, stream).unwrap();

                Pin::new(&mut tls_stream).accept().await.map_err(|e| {
                    error!(parent: self.node().span(), "TLS handshake error: {}", e);
                    io::ErrorKind::InvalidData
                })?;

                // get the shared value based on the TLS handshake
                let shared_value = get_shared_value(&tls_stream)?;

                // use the HTTP codec to read/write the (post-TLS) handshake messages
                let codec = HttpCodec::new(self.node().span().clone(), HttpMsg::Request);
                let mut framed = Framed::new(&mut tls_stream, codec);

                // read the HTTP request message (there should only be headers)
                let request_body = framed.try_next().await?.ok_or(io::ErrorKind::InvalidData)?;
                if !request_body.is_empty() {
                    warn!(parent: self.node().span(), "trailing bytes in the handshake request from {}: {:?}", addr, request_body);
                }

                // base58-encode the public key and create the session signature
                let base58_pk =
                    encode_base58(NodeType::Public, &self.crypto.public_key.serialize()[..]);
                let sig = create_session_signature(&self.crypto, &shared_value);

                // prepare the response
                let mut response = Vec::new();
                response.extend_from_slice(b"HTTP/1.1 101 Switching Protocols\r\n");
                response.extend_from_slice(b"Connection: Upgrade\r\n");
                response.extend_from_slice(b"Upgrade: XRPL/2.2\r\n");
                response.extend_from_slice(b"Connect-As: Peer\r\n");
                response.extend_from_slice(b"Server: rippled-1.9.1\r\n");
                response.extend_from_slice(format!("Public-Key: {}\r\n", base58_pk).as_bytes());
                response
                    .extend_from_slice(format!("Session-Signature: {}\r\n\r\n", sig).as_bytes());
                let response = Bytes::from(response);

                trace!(parent: self.node().span(), "responding to {} with {:?}", addr, response);

                // send the handshake HTTP response message
                framed.send(response).await?;

                tls_stream
            }
        };

        self.return_stream(&mut conn, tls_stream);

        Ok(conn)
    }
}
