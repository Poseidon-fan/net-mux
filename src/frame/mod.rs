mod header;

use crate::{alloc::StreamId, consts::PROTOCOL_V0, error::Error};
use tokio_util::{
    bytes::{Buf, BufMut, BytesMut},
    codec::{Decoder, Encoder},
};

pub(crate) use header::*;

// Network frame structure for the net-mux protocol.
//
// A frame is the basic unit of data transmission in the net-mux protocol.
// It consists of a fixed-size header containing metadata and an optional
// variable-length data payload. Frames are used to multiplex multiple
// streams over a single network connection.
//
// # Structure
//
// - **Header**: 8 bytes containing version, command, data length, and stream ID
// - **Data**: Variable-length payload (0-65,535 bytes)
//
// # Frame Types
//
// - **SYN**: Establish a new stream (no data payload)
// - **PSH**: Send data payload
// - **FIN**: Close a stream (no data payload)
#[derive(Debug)]
pub(crate) struct Frame {
    pub header: Header,
    pub data: Vec<u8>,
}

// Codec for encoding and decoding `Frame` structures.
//
// `FrameCodec` implements Tokio's `Decoder` and `Encoder` traits, allowing
// seamless integration with async I/O operations. It handles the conversion
// between `Frame` structures and raw byte streams for network transmission.
//
// # Features
//
// - Automatic buffer management for partial frames
// - Error handling for malformed data
// - Efficient binary serialization/deserialization
pub(crate) struct FrameCodec;

impl Frame {
    // Creates a new SYN (synchronize) frame for establishing a stream
    pub fn new_syn(stream_id: StreamId) -> Self {
        Self {
            header: Header::new(PROTOCOL_V0, Cmd::Syn, 0, stream_id),
            data: vec![],
        }
    }

    // Creates a new PSH (push) frame containing data
    pub fn new_psh(stream_id: StreamId, data: &[u8]) -> Self {
        Self {
            header: Header::new(PROTOCOL_V0, Cmd::Psh, data.len() as u16, stream_id),
            data: data.to_vec(),
        }
    }

    // Creates a new FIN (finish) frame for closing a stream
    pub fn new_fin(stream_id: StreamId) -> Self {
        Self {
            header: Header::new(PROTOCOL_V0, Cmd::Fin, 0, stream_id),
            data: vec![],
        }
    }

    // Creates a new ACK (acknowledgment) frame
    pub fn new_ack(stream_id: StreamId) -> Self {
        Self {
            header: Header::new(PROTOCOL_V0, Cmd::Ack, 0, stream_id),
            data: vec![],
        }
    }

    // Calculates the total length of the frame in bytes
    pub fn frame_len(&self) -> usize {
        HEADER_LENGTH + self.header.data_length as usize
    }
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Check if buffer contains enough data for the header
        if buf.len() < HEADER_LENGTH {
            return Ok(None);
        }

        // Parse header fields without consuming the buffer yet
        let version = match buf[0] {
            0x0 => PROTOCOL_V0,
            x => return Err(Error::InvalidVersion(x)),
        };
        let cmd = buf[1].try_into()?;
        let data_length = (&buf[2..4]).get_u16();
        let frame_length = HEADER_LENGTH + data_length as usize;

        // Check if buffer contains enough data for the complete frame
        if buf.len() < frame_length {
            buf.reserve(frame_length - buf.len());
            return Ok(None);
        }

        // Consume header fields and extract stream ID
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

        // Write header fields
        buf.put_u8(frame.header.version);
        buf.put_u8(frame.header.cmd as u8);
        buf.put_u16(data_length as u16);
        buf.put_u32(frame.header.stream_id);

        // Write data payload
        buf.put_slice(&frame.data);

        Ok(())
    }
}
