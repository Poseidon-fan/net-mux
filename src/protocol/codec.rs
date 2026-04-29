//! `tokio_util::codec` adapter for [`Frame`].

use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::error::Error;

use super::frame::{Flags, Frame};
use super::header::{FrameType, HEADER_LEN, Header, PROTOCOL_VERSION};

/// Default cap on a single decoded payload. The session layer always passes
/// in a tighter limit derived from `Config::max_frame_size`; this constant
/// is just a hard ceiling to guard against malicious peers when the codec
/// is used standalone.
const ABSOLUTE_MAX_PAYLOAD: u32 = 16 * 1024 * 1024;

/// Codec converting between byte streams and [`Frame`]s.
#[derive(Debug)]
pub(crate) struct FrameCodec {
    max_payload: u32,
}

impl FrameCodec {
    pub(crate) fn new(max_payload: u32) -> Self {
        Self {
            max_payload: max_payload.min(ABSOLUTE_MAX_PAYLOAD),
        }
    }
}

impl Default for FrameCodec {
    fn default() -> Self {
        Self::new(ABSOLUTE_MAX_PAYLOAD)
    }
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < HEADER_LEN {
            return Ok(None);
        }
        let mut header_bytes = [0u8; HEADER_LEN];
        header_bytes.copy_from_slice(&src[..HEADER_LEN]);
        let header = Header::decode(&header_bytes)?;

        let payload_len = match header.frame_type {
            FrameType::Data => header.length,
            // Non-Data frames carry no payload; `length` is repurposed as
            // metadata (delta, opaque, error code).
            FrameType::WindowUpdate | FrameType::Ping | FrameType::GoAway => 0,
        };

        if payload_len > self.max_payload {
            return Err(Error::Protocol("frame payload exceeds limit"));
        }

        let total = HEADER_LEN + payload_len as usize;
        if src.len() < total {
            src.reserve(total - src.len());
            return Ok(None);
        }

        src.advance(HEADER_LEN);
        let payload = if payload_len == 0 {
            Bytes::new()
        } else {
            src.split_to(payload_len as usize).freeze()
        };

        let frame = match header.frame_type {
            FrameType::Data => Frame::Data {
                stream_id: header.stream_id,
                flags: header.flags,
                payload,
            },
            FrameType::WindowUpdate => Frame::WindowUpdate {
                stream_id: header.stream_id,
                flags: header.flags,
                delta: header.length,
            },
            FrameType::Ping => Frame::Ping {
                flags: header.flags,
                opaque: header.length,
            },
            FrameType::GoAway => Frame::GoAway {
                error_code: header.length.into(),
            },
        };

        Ok(Some(frame))
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = Error;

    fn encode(&mut self, frame: Frame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let (frame_type, flags, stream_id, length, payload): (
            FrameType,
            Flags,
            u32,
            u32,
            Option<Bytes>,
        ) = match frame {
            Frame::Data {
                stream_id,
                flags,
                payload,
            } => {
                if payload.len() as u64 > u64::from(self.max_payload) {
                    return Err(Error::Protocol("data payload exceeds limit"));
                }
                let len = payload.len() as u32;
                (FrameType::Data, flags, stream_id, len, Some(payload))
            }
            Frame::WindowUpdate {
                stream_id,
                flags,
                delta,
            } => (FrameType::WindowUpdate, flags, stream_id, delta, None),
            Frame::Ping { flags, opaque } => (FrameType::Ping, flags, 0, opaque, None),
            Frame::GoAway { error_code } => (
                FrameType::GoAway,
                Flags::empty(),
                0,
                error_code.as_u32(),
                None,
            ),
        };

        let header = Header {
            version: PROTOCOL_VERSION,
            frame_type,
            flags,
            stream_id,
            length,
        };

        let total = HEADER_LEN + payload.as_ref().map_or(0, Bytes::len);
        dst.reserve(total);
        let mut header_bytes = [0u8; HEADER_LEN];
        header.encode(&mut header_bytes);
        dst.put_slice(&header_bytes);
        if let Some(p) = payload {
            dst.put_slice(&p);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use bytes::Bytes;

    fn roundtrip_one(frame: Frame) -> Frame {
        let mut codec = FrameCodec::default();
        let mut buf = BytesMut::new();
        codec.encode(frame, &mut buf).unwrap();
        codec.decode(&mut buf).unwrap().unwrap()
    }

    #[test]
    fn data_roundtrip() {
        let payload = Bytes::from_static(b"hello, world");
        let frame = Frame::Data {
            stream_id: 7,
            flags: Flags::SYN | Flags::ACK,
            payload: payload.clone(),
        };
        match roundtrip_one(frame) {
            Frame::Data {
                stream_id,
                flags,
                payload: out,
            } => {
                assert_eq!(stream_id, 7);
                assert_eq!(flags, Flags::SYN | Flags::ACK);
                assert_eq!(out, payload);
            }
            _ => panic!("wrong frame type"),
        }
    }

    #[test]
    fn empty_data_roundtrip() {
        let frame = Frame::fin(11);
        match roundtrip_one(frame) {
            Frame::Data {
                stream_id,
                flags,
                payload,
            } => {
                assert_eq!(stream_id, 11);
                assert_eq!(flags, Flags::FIN);
                assert!(payload.is_empty());
            }
            _ => panic!("wrong frame type"),
        }
    }

    #[test]
    fn window_update_roundtrip() {
        match roundtrip_one(Frame::window_update(3, 64 * 1024)) {
            Frame::WindowUpdate {
                stream_id, delta, ..
            } => {
                assert_eq!(stream_id, 3);
                assert_eq!(delta, 64 * 1024);
            }
            _ => panic!("wrong frame type"),
        }
    }

    #[test]
    fn ping_roundtrip() {
        match roundtrip_one(Frame::ping(0xCAFEBABE)) {
            Frame::Ping { flags, opaque } => {
                assert_eq!(flags, Flags::empty());
                assert_eq!(opaque, 0xCAFE_BABE);
            }
            _ => panic!("wrong frame type"),
        }
        match roundtrip_one(Frame::pong(0xCAFEBABE)) {
            Frame::Ping { flags, opaque } => {
                assert_eq!(flags, Flags::ACK);
                assert_eq!(opaque, 0xCAFE_BABE);
            }
            _ => panic!("wrong frame type"),
        }
    }

    #[test]
    fn goaway_roundtrip() {
        match roundtrip_one(Frame::go_away(ErrorCode::FlowControlError)) {
            Frame::GoAway { error_code } => assert_eq!(error_code, ErrorCode::FlowControlError),
            _ => panic!("wrong frame type"),
        }
    }

    #[test]
    fn partial_decode_returns_none() {
        let mut codec = FrameCodec::default();
        let mut buf = BytesMut::new();
        // only 4 bytes of a header
        buf.extend_from_slice(&[1, 0, 0, 0]);
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn enforces_max_payload() {
        let mut codec = FrameCodec::new(8);
        let frame = Frame::Data {
            stream_id: 1,
            flags: Flags::empty(),
            payload: Bytes::from_static(b"too long payload"),
        };
        let mut buf = BytesMut::new();
        let err = codec.encode(frame, &mut buf).unwrap_err();
        assert!(matches!(err, Error::Protocol(_)));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use bytes::Bytes;
    use proptest::prelude::*;

    fn arb_flags() -> impl Strategy<Value = Flags> {
        any::<u8>().prop_map(|bits| Flags::from_bits_truncate(u16::from(bits) & 0x000F))
    }

    fn arb_data_frame() -> impl Strategy<Value = Frame> {
        (
            any::<u32>(),
            arb_flags(),
            proptest::collection::vec(any::<u8>(), 0..1024),
        )
            .prop_map(|(id, flags, data)| Frame::Data {
                stream_id: id,
                flags,
                payload: Bytes::from(data),
            })
    }

    fn arb_window_update() -> impl Strategy<Value = Frame> {
        (any::<u32>(), any::<u32>()).prop_map(|(id, delta)| Frame::window_update(id, delta))
    }

    fn arb_ping() -> impl Strategy<Value = Frame> {
        (any::<bool>(), any::<u32>()).prop_map(|(is_ack, op)| {
            if is_ack {
                Frame::pong(op)
            } else {
                Frame::ping(op)
            }
        })
    }

    fn arb_goaway() -> impl Strategy<Value = Frame> {
        any::<u32>().prop_map(|c| Frame::go_away(c.into()))
    }

    fn arb_frame() -> impl Strategy<Value = Frame> {
        prop_oneof![
            arb_data_frame(),
            arb_window_update(),
            arb_ping(),
            arb_goaway(),
        ]
    }

    proptest! {
        #[test]
        fn frame_codec_roundtrip(frames in proptest::collection::vec(arb_frame(), 0..32)) {
            let mut codec = FrameCodec::new(ABSOLUTE_MAX_PAYLOAD);
            let mut buf = BytesMut::new();
            for f in &frames {
                codec.encode(f.clone(), &mut buf).unwrap();
            }
            let mut decoded = Vec::new();
            while let Some(f) = codec.decode(&mut buf).unwrap() {
                decoded.push(f);
            }
            prop_assert_eq!(buf.len(), 0);
            prop_assert_eq!(decoded.len(), frames.len());
            for (a, b) in decoded.iter().zip(frames.iter()) {
                match (a, b) {
                    (
                        Frame::Data { stream_id: ai, flags: af, payload: ap },
                        Frame::Data { stream_id: bi, flags: bf, payload: bp },
                    ) => {
                        prop_assert_eq!(ai, bi);
                        prop_assert_eq!(af, bf);
                        prop_assert_eq!(ap, bp);
                    }
                    (
                        Frame::WindowUpdate { stream_id: ai, delta: ad, .. },
                        Frame::WindowUpdate { stream_id: bi, delta: bd, .. },
                    ) => {
                        prop_assert_eq!(ai, bi);
                        prop_assert_eq!(ad, bd);
                    }
                    (
                        Frame::Ping { flags: af, opaque: ao },
                        Frame::Ping { flags: bf, opaque: bo },
                    ) => {
                        prop_assert_eq!(af, bf);
                        prop_assert_eq!(ao, bo);
                    }
                    (
                        Frame::GoAway { error_code: a },
                        Frame::GoAway { error_code: b },
                    ) => prop_assert_eq!(a, b),
                    _ => prop_assert!(false, "frame variant mismatch"),
                }
            }
        }
    }
}
