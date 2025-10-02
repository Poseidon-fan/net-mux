use std::mem;

use crate::{StreamId, consts::Version, error::Error};

// Frame header structure for the net-mux protocol.
//
// The header contains metadata about the frame including protocol version,
// command type, data length, and stream identification. The structure is
// designed to be compact and efficient for network transmission.
//
// # Layout
//
// The header is 8 bytes total:
// - Version: 1 byte
// - Command: 1 byte
// - Data Length: 2 bytes
// - Stream ID: 4 bytes
#[derive(Debug, Clone)]
#[repr(C)]
pub(crate) struct Header {
    pub version: Version,
    pub cmd: Cmd,
    pub data_length: DataLength,
    pub stream_id: StreamId,
}

// Control commands for frame types.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Cmd {
    // SYN (Synchronize) - Establish a new stream
    Syn = 0x01,
    // FIN (Finish) - Close a stream
    Fin = 0x02,
    // PSH (Push) - Send data payload
    Psh = 0x03,
}

pub(crate) type DataLength = u16;

pub(crate) const HEADER_LENGTH: usize = mem::size_of::<Header>();

impl TryFrom<u8> for Cmd {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Cmd::Syn),
            0x02 => Ok(Cmd::Fin),
            0x03 => Ok(Cmd::Psh),
            _ => Err(Error::InvalidCmd(value)),
        }
    }
}

impl Header {
    // Creates a new frame header with the specified parameters.
    pub fn new(version: Version, cmd: Cmd, data_length: DataLength, stream_id: StreamId) -> Self {
        Self {
            version,
            cmd,
            data_length,
            stream_id,
        }
    }
}
