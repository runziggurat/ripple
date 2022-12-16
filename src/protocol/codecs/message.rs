use std::io;

use bytes::{Buf, BufMut, BytesMut};
use prost::Message;
use tokio_util::codec::{Decoder, Encoder};
use tracing::*;

use crate::protocol::proto::*;

const HEADER_LEN_COMPRESSED: u32 = 10;

const HEADER_LEN_UNCOMPRESSED: u32 = 6;

const COMPRESSION_ALGO: u8 = 0xf0;

const COMPRESSION_LZ4: u8 = 0x90;

const COMPRESSED_TRUE: u8 = 0x80;

const COMPRESSED_FALSE: u8 = 0xfc;

const PROTOCOL_ERROR: u8 = 0x0c;

#[derive(Debug)]
enum Compression {
    None,
    LZ4,
}

#[derive(Debug)]
pub struct Header {
    #[allow(dead_code)]
    total_wire_size: u32,
    #[allow(dead_code)]
    header_size: u32,
    payload_wire_size: u32,
    #[allow(dead_code)]
    uncompressed_size: u32,
    message_type: u16,
    #[allow(dead_code)]
    compression: Compression,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Payload {
    TmManifests(TmManifests),
    TmPing(TmPing),
    TmCluster(TmCluster),
    TmEndpoints(TmEndpoints),
    TmTransaction(TmTransaction),
    TmGetLedger(TmGetLedger),
    TmLedgerData(TmLedgerData),
    TmProposeLedger(TmProposeSet),
    TmStatusChange(TmStatusChange),
    TmHaveTransactions(TmHaveTransactions),
    TmHaveSet(TmHaveTransactionSet),
    TmValidation(TmValidation),
    TmGetObjectByHash(TmGetObjectByHash),
    TmValidatorList(TmValidatorList),
    TmSquelch(TmSquelch),
    TmValidatorListCollection(TmValidatorListCollection),
    TmProofPathRequest(TmProofPathRequest),
    TmProofPathResponse(TmProofPathResponse),
    TmReplayDeltaRequest(TmReplayDeltaRequest),
    TmReplayDeltaResponse(TmReplayDeltaResponse),
    TmGetPeerShardInfoV2(TmGetPeerShardInfoV2),
    TmPeerShardInfoV2(TmPeerShardInfoV2),
    TmTransactions(TmTransactions),
}

#[derive(Debug)]
pub struct BinaryMessage {
    pub header: Header,
    pub payload: Payload,
}

pub struct MessageCodec {
    current_msg_header: Option<Header>,
    // The associated node's span.
    span: Span,
}

impl MessageCodec {
    pub fn new(span: Span) -> Self {
        Self {
            current_msg_header: None,
            span,
        }
    }
}

impl Decoder for MessageCodec {
    type Item = BinaryMessage;
    type Error = io::Error;

    // Based on Ripple's `invokeProtocolMessage` (ripple/overlay/impl/ProtocolMessage.cpp)
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        if self.current_msg_header.is_none() {
            if src[0] & COMPRESSED_TRUE != 0 {
                trace!(parent: &self.span, "processing a compressed message");

                let header_size = HEADER_LEN_COMPRESSED;
                if src.remaining() < header_size as usize {
                    return Ok(None);
                }

                // protocol error
                if (src[0] & PROTOCOL_ERROR) != 0 {
                    unimplemented!();
                }

                let header_bytes = src.split_to(header_size as usize);
                let mut iter = header_bytes.into_iter();

                let compression = src[0] & COMPRESSION_ALGO;
                trace!(parent: &self.span, "compression: {:x}", compression);

                // only LZ4 is currently supported
                if compression != COMPRESSION_LZ4 {
                    unimplemented!();
                }

                let mut payload_wire_size = 0;
                for _ in 0..4 {
                    payload_wire_size = (payload_wire_size << 8u32) + iter.next().unwrap() as u32;
                }
                payload_wire_size &= 0x0FFFFFFF; // clear the top four bits (the compression bits)

                let total_wire_size = header_size + payload_wire_size;

                let mut message_type = 0;
                for _ in 0..2 {
                    message_type = (message_type << 8u16) + iter.next().unwrap() as u16;
                }

                let mut uncompressed_size = 0;
                for _ in 0..4 {
                    uncompressed_size = (uncompressed_size << 8u32) + iter.next().unwrap() as u32;
                }

                let header = Header {
                    total_wire_size,
                    header_size,
                    payload_wire_size,
                    uncompressed_size,
                    message_type,
                    compression: Compression::LZ4,
                };

                self.current_msg_header = Some(header);
            } else if src[0] & COMPRESSED_FALSE == 0 {
                trace!(parent: &self.span, "processing an uncompressed message");

                let header_size = HEADER_LEN_UNCOMPRESSED;
                if src.remaining() < header_size as usize {
                    return Ok(None);
                }

                let header_bytes = src.split_to(header_size as usize);
                let mut iter = header_bytes.into_iter();

                let mut payload_wire_size = 0;
                for _ in 0..4 {
                    payload_wire_size = (payload_wire_size << 8u32) + iter.next().unwrap() as u32;
                }

                let uncompressed_size = payload_wire_size;
                let total_wire_size = header_size + payload_wire_size;

                let mut message_type = 0;
                for _ in 0..2 {
                    message_type = (message_type << 8u16) + iter.next().unwrap() as u16;
                }

                let header = Header {
                    total_wire_size,
                    header_size,
                    payload_wire_size,
                    uncompressed_size,
                    message_type,
                    compression: Compression::None,
                };
                trace!(parent: &self.span, "header: {:?}", header);
                self.current_msg_header = Some(header);
            } else {
                error!(parent: &self.span, "invalid compression indicator");

                return Err(io::ErrorKind::InvalidData.into());
            }
        }

        if let Some(Header {
            payload_wire_size, ..
        }) = self.current_msg_header
        {
            if src.remaining() < payload_wire_size as usize {
                return Ok(None);
            }

            let header = self.current_msg_header.take().unwrap();
            let mut payload = src.split_to(payload_wire_size as usize);

            let payload = match header.message_type {
                2 => Payload::TmManifests(Message::decode(&mut payload)?),
                3 => Payload::TmPing(Message::decode(&mut payload)?),
                5 => Payload::TmCluster(Message::decode(&mut payload)?),
                15 => Payload::TmEndpoints(Message::decode(&mut payload)?),
                30 => Payload::TmTransaction(Message::decode(&mut payload)?),
                31 => Payload::TmGetLedger(Message::decode(&mut payload)?),
                32 => Payload::TmLedgerData(Message::decode(&mut payload)?),
                33 => Payload::TmProposeLedger(Message::decode(&mut payload)?),
                34 => Payload::TmStatusChange(Message::decode(&mut payload)?),
                35 => Payload::TmHaveSet(Message::decode(&mut payload)?),
                41 => Payload::TmValidation(Message::decode(&mut payload)?),
                42 => Payload::TmGetObjectByHash(Message::decode(&mut payload)?),
                54 => Payload::TmValidatorList(Message::decode(&mut payload)?),
                55 => Payload::TmSquelch(Message::decode(&mut payload)?),
                56 => Payload::TmValidatorListCollection(Message::decode(&mut payload)?),
                57 => Payload::TmProofPathRequest(Message::decode(&mut payload)?),
                58 => Payload::TmProofPathResponse(Message::decode(&mut payload)?),
                59 => Payload::TmReplayDeltaRequest(Message::decode(&mut payload)?),
                60 => Payload::TmReplayDeltaResponse(Message::decode(&mut payload)?),
                61 => Payload::TmGetPeerShardInfoV2(Message::decode(&mut payload)?),
                62 => Payload::TmPeerShardInfoV2(Message::decode(&mut payload)?),
                63 => Payload::TmHaveTransactions(Message::decode(&mut payload)?),
                64 => Payload::TmTransactions(Message::decode(&mut payload)?),
                _ => unimplemented!(),
            };

            let message = BinaryMessage { header, payload };

            debug!(parent: &self.span, "decoded a header: {:?}", message.header);

            Ok(Some(message))
        } else {
            unreachable!();
        }
    }
}

// Based on `pack` from Ripple's `Message::setHeader` (ripple/overlay/impl/Message.cpp)
fn pack(dst: &mut [u8], size: u32) {
    dst[0] = ((size >> 24) & 0x0f) as u8;
    dst[1] = ((size >> 16) & 0xff) as u8;
    dst[2] = ((size >> 8) & 0xff) as u8;
    dst[3] = (size & 0xff) as u8;
}

impl Encoder<Payload> for MessageCodec {
    type Error = io::Error;

    // Based on Ripple's `Message::Message` (ripple/overlay/impl/Message.cpp)
    fn encode(&mut self, message: Payload, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let (payload_len, msg_type) = match &message {
            Payload::TmManifests(msg) => {
                (msg.encoded_len() as u32, MessageType::MtManifests as i32)
            }
            Payload::TmPing(msg) => (msg.encoded_len() as u32, MessageType::MtPing as i32),
            Payload::TmCluster(msg) => (msg.encoded_len() as u32, MessageType::MtCluster as i32),
            Payload::TmEndpoints(msg) => {
                (msg.encoded_len() as u32, MessageType::MtEndpoints as i32)
            }
            Payload::TmTransaction(msg) => {
                (msg.encoded_len() as u32, MessageType::MtTransaction as i32)
            }
            Payload::TmGetLedger(msg) => {
                (msg.encoded_len() as u32, MessageType::MtGetLedger as i32)
            }
            Payload::TmLedgerData(msg) => {
                (msg.encoded_len() as u32, MessageType::MtLedgerData as i32)
            }
            Payload::TmProposeLedger(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtProposeLedger as i32,
            ),
            Payload::TmStatusChange(msg) => {
                (msg.encoded_len() as u32, MessageType::MtStatusChange as i32)
            }
            Payload::TmValidation(msg) => {
                (msg.encoded_len() as u32, MessageType::MtValidation as i32)
            }
            Payload::TmGetObjectByHash(msg) => {
                (msg.encoded_len() as u32, MessageType::MtGetObjects as i32)
            }
            Payload::TmValidatorList(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtValidatorlist as i32,
            ),
            Payload::TmSquelch(msg) => (msg.encoded_len() as u32, MessageType::MtSquelch as i32),
            Payload::TmHaveSet(msg) => (msg.encoded_len() as u32, MessageType::MtHaveSet as i32),
            Payload::TmValidatorListCollection(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtValidatorlistcollection as i32,
            ),
            Payload::TmProofPathRequest(msg) => {
                (msg.encoded_len() as u32, MessageType::MtProofPathReq as i32)
            }
            Payload::TmProofPathResponse(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtProofPathResponse as i32,
            ),
            Payload::TmReplayDeltaRequest(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtReplayDeltaReq as i32,
            ),
            Payload::TmReplayDeltaResponse(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtReplayDeltaResponse as i32,
            ),
            Payload::TmGetPeerShardInfoV2(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtGetPeerShardInfoV2 as i32,
            ),
            Payload::TmPeerShardInfoV2(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtPeerShardInfoV2 as i32,
            ),
            Payload::TmHaveTransactions(msg) => (
                msg.encoded_len() as u32,
                MessageType::MtHaveTransactions as i32,
            ),
            Payload::TmTransactions(msg) => {
                (msg.encoded_len() as u32, MessageType::MtTransactions as i32)
            }
        };

        let header_size = HEADER_LEN_UNCOMPRESSED;
        let _header = Header {
            total_wire_size: header_size + payload_len,
            header_size,
            payload_wire_size: payload_len,
            uncompressed_size: payload_len,
            message_type: msg_type as u16,
            compression: Compression::None, // TODO: are compressed messages used?
        };

        let mut header_bytes = [0u8; HEADER_LEN_UNCOMPRESSED as usize];

        pack(&mut header_bytes, payload_len);

        header_bytes[4] = ((msg_type >> 8) & 0xff) as u8;
        header_bytes[5] = (msg_type & 0xff) as u8;

        let mut bytes = BytesMut::new();

        bytes.put(&header_bytes[..]);

        match message {
            Payload::TmManifests(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmPing(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmCluster(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmEndpoints(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmTransaction(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmGetLedger(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmLedgerData(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmProposeLedger(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmStatusChange(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmValidation(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmGetObjectByHash(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmValidatorList(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmSquelch(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmHaveSet(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmValidatorListCollection(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmProofPathResponse(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmProofPathRequest(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmReplayDeltaRequest(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmReplayDeltaResponse(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmGetPeerShardInfoV2(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmPeerShardInfoV2(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmTransactions(msg) => (msg.encode(&mut bytes).unwrap(),),
            Payload::TmHaveTransactions(msg) => (msg.encode(&mut bytes).unwrap(),),
        };

        dst.put(&*bytes);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_and_encode() {
        // a sample raw message
        let raw = BytesMut::from(
            &b"\0\0\0\xeb\0)\n\xe8\x01\"\x80\0\0\x01&\x01\xbb\xef\xd0)*Qj\":r\
            \x8b\x90\xe3fz`[Q\x1c5@P\xcd\\\x1e0\x0c5\xdd\xe9A\xe2\xf62\xa4\xbev\x81jD)Z~\xda6\xa8\x96\
            \x81H\xa6P\x17\x81\xcc\xe2\x81\x12\xdeF\xba\x9fCj \xc7\xce\x9b\xcb\xf7\x8f\x90$\x1f\x9b\xfb\
            \xc3W\xcd\x1c\\\xb5\xe2x\x12P\x19\x14\xcb1\x9bh\x9cY\\\x02RDF(\x17\xcc\xa8\xf3>\x05\x83\xd8\
            \x14y\xfa\xb6\xdb\xa4\xe0\x1e\x96M4s!\x03f\x98Z*X\xfc\xddd\0J\n\x1b\x0f\xe5\xc7U\x08\x91\
            Cgu\xadPV-\xa6\xdf\xac\xe1:\xe6/vF0D\x02 5(s\x01\x17\x94\x07\x9a\xe9\x9c\x1c\xe9g\x02Y\
            \x9fZ!\x1c\xecg+\\\x11NS\xb8g\x05\x8c;b\x02 {\xe0\x14\xe6\xc7\x91M\xd0#\xf6;'i\xc5\
            \xad\"~\xb2\xdd\xb93\xf7V\xa1Zc\xe2D\xf8\x8bf\xd3"[..],
        );

        let mut codec = MessageCodec::new(Span::none());
        let msg = codec.decode(&mut raw.clone()).unwrap().unwrap();

        let mut encoded = BytesMut::new();
        codec.encode(msg.payload, &mut encoded).unwrap();

        assert_eq!(raw, encoded);
    }
}
