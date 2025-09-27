use std::mem;

use crate::{StreamId, consts::Version, error::Error};

#[derive(Debug, Clone)]
#[repr(C)]
pub(crate) struct Header {
    pub version: Version,
    pub cmd: Cmd,
    pub data_length: DataLength,
    pub stream_id: StreamId,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Cmd {
    Syn = 0x01,
    Fin = 0x02,
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
    pub fn new(version: Version, cmd: Cmd, data_length: DataLength, stream_id: StreamId) -> Self {
        Self {
            version,
            cmd,
            data_length,
            stream_id,
        }
    }
}
