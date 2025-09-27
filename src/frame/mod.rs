mod header;

use crate::{
    consts::PROTOCOL_V0,
    error::Error,
    frame::header::{HEADER_LENGTH, Header},
};
use tokio_util::{
    bytes::{Buf, BufMut, BytesMut},
    codec::{Decoder, Encoder},
};

pub(crate) struct Frame {
    pub header: Header,
    pub data: Vec<u8>,
}

pub(crate) struct FrameCodec;

impl Decoder for FrameCodec {
    type Item = Frame;

    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // check if buffer is enough for header
        if buf.len() < HEADER_LENGTH {
            return Ok(None);
        }

        // peek header
        let version = match buf[0] {
            0x0 => PROTOCOL_V0,
            x => return Err(Error::InvalidVersion(x)),
        };
        let cmd = buf[1].try_into()?;
        let data_length = (&buf[2..4]).get_u16();
        let frame_length = HEADER_LENGTH + data_length as usize;

        if buf.len() < frame_length {
            buf.reserve(frame_length - buf.len());
            return Ok(None);
        }

        // consume Version, Cmd, DataLength, and read StreamId
        buf.advance(4);
        let stream_id = buf.get_u32();
        let data = buf.split_to(data_length.into()).to_vec();
        Ok(Some(Frame {
            header: Header::new(version, cmd, data_length, stream_id),
            data,
        }))
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = Error;

    fn encode(&mut self, frame: Frame, buf: &mut BytesMut) -> Result<(), Self::Error> {
        let data_length = frame.data.len();
        if data_length > u16::MAX as usize {
            return Err(Error::DataLengthTooLarge);
        }
        let frame_length = HEADER_LENGTH + data_length;
        buf.reserve(frame_length);
        buf.put_u8(frame.header.version);
        buf.put_u8(frame.header.cmd as u8);
        buf.put_u16(data_length as u16);
        buf.put_u32(frame.header.stream_id);
        buf.put_slice(&frame.data);
        Ok(())
    }
}
