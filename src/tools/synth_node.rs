use std::{
    io,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use pea2pea::{protocols::Writing, Pea2Pea};
use tokio::{
    sync::{mpsc, mpsc::Receiver, oneshot},
    time::timeout,
};

use crate::{
    protocol::codecs::binary::{BinaryMessage, Payload},
    tools::inner_node::InnerNode,
};

pub struct SyntheticNode {
    inner: InnerNode,
    receiver: Receiver<(SocketAddr, BinaryMessage)>,
}

impl SyntheticNode {
    pub async fn new(config: pea2pea::Config) -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let inner = InnerNode::new(config, sender).await;
        Self { inner, receiver }
    }

    /// Connects to the target address.
    pub async fn connect(&self, target: SocketAddr) -> io::Result<()> {
        self.inner.connect(target).await
    }

    pub fn unicast(
        &self,
        addr: SocketAddr,
        message: Payload,
    ) -> io::Result<oneshot::Receiver<io::Result<()>>> {
        self.inner.unicast(addr, message)
    }

    /// Reads a message from the inbound (internal) queue of the node.
    ///
    /// Messages are sent to the queue when unfiltered by the message filter.
    pub async fn recv_message(&mut self) -> (SocketAddr, BinaryMessage) {
        match self.receiver.recv().await {
            Some(message) => message,
            None => panic!("all senders dropped!"),
        }
    }

    /// Gracefully shuts down the node.
    pub async fn shut_down(&self) {
        self.inner.shut_down().await
    }

    pub fn listening_addr(&self) -> io::Result<SocketAddr> {
        self.inner.node().listening_addr()
    }

    pub fn is_connected(&self, addr: SocketAddr) -> bool {
        self.inner.node().is_connected(addr)
    }

    pub fn num_connected(&self) -> usize {
        self.inner.node().num_connected()
    }

    pub fn is_connected_ip(&self, addr: IpAddr) -> bool {
        self.inner.is_connected_ip(addr)
    }

    pub async fn expect_message(&mut self, check: fn(&BinaryMessage) -> bool) -> bool {
        let d = Duration::from_secs(20);
        timeout(d, async {
            loop {
                let (_, message) = self.recv_message().await;
                if check(&message) {
                    return true;
                }
            }
        })
        .await
        .is_ok()
    }
}
