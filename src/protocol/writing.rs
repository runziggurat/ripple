use std::net::SocketAddr;

use pea2pea::{protocols::Writing, ConnectionSide, Pea2Pea};

use crate::{
    protocol::codecs::binary::{BinaryCodec, Payload},
    tools::synthetic_node::SyntheticNode,
};

impl Writing for SyntheticNode {
    type Message = Payload;
    type Codec = BinaryCodec;

    fn codec(&self, _addr: SocketAddr, _side: ConnectionSide) -> Self::Codec {
        Self::Codec::new(self.node().span().clone())
    }
}
