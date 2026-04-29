//! 12-byte fixed frame header.

use crate::error::Error;

use super::frame::Flags;

/// Current protocol version.
pub(crate) const PROTOCOL_VERSION: u8 = 1;

/// Length of the wire header in bytes.
pub(crate) const HEADER_LEN: usize = 12;

/// Frame type discriminator carried in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum FrameType {
    Data = 0,
    WindowUpdate = 1,
    Ping = 2,
    GoAway = 3,
}

impl TryFrom<u8> for FrameType {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(FrameType::Data),
            1 => Ok(FrameType::WindowUpdate),
            2 => Ok(FrameType::Ping),
            3 => Ok(FrameType::GoAway),
            _ => Err(Error::Protocol("unknown frame type")),
        }
    }
}

/// Decoded header fields.
///
/// Wire layout (big-endian):
/// ```text
/// 0       1       2               4               8               12
/// +-------+-------+---------------+---------------+---------------+
/// |  Ver  | Type  |     Flags     |   StreamId    |    Length     |
/// +-------+-------+---------------+---------------+---------------+
///   u8      u8         u16              u32             u32
/// ```
///
/// `Length` carries different semantics depending on `FrameType`:
/// * `Data` — payload byte count
/// * `WindowUpdate` — credit increment, in bytes
/// * `Ping` — opaque echo payload (typically a nonce)
/// * `GoAway` — error code (see [`crate::ErrorCode`])
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    pub version: u8,
    pub frame_type: FrameType,
    pub flags: Flags,
    pub stream_id: u32,
    pub length: u32,
}

impl Header {
    pub(crate) fn encode(&self, out: &mut [u8; HEADER_LEN]) {
        out[0] = self.version;
        out[1] = self.frame_type as u8;
        out[2..4].copy_from_slice(&self.flags.bits().to_be_bytes());
        out[4..8].copy_from_slice(&self.stream_id.to_be_bytes());
        out[8..12].copy_from_slice(&self.length.to_be_bytes());
    }

    pub(crate) fn decode(bytes: &[u8; HEADER_LEN]) -> Result<Self, Error> {
        let version = bytes[0];
        if version != PROTOCOL_VERSION {
            return Err(Error::Protocol("unsupported protocol version"));
        }
        let frame_type = FrameType::try_from(bytes[1])?;
        let flags_bits = u16::from_be_bytes([bytes[2], bytes[3]]);
        let flags = Flags::from_bits(flags_bits).ok_or(Error::Protocol("unknown flag bits"))?;
        let stream_id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let length = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        Ok(Self {
            version,
            frame_type,
            flags,
            stream_id,
            length,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let h = Header {
            version: PROTOCOL_VERSION,
            frame_type: FrameType::Data,
            flags: Flags::SYN | Flags::ACK,
            stream_id: 0xDEAD_BEEF,
            length: 12345,
        };
        let mut buf = [0u8; HEADER_LEN];
        h.encode(&mut buf);
        let decoded = Header::decode(&buf).unwrap();
        assert_eq!(h, decoded);
    }

    #[test]
    fn rejects_bad_version() {
        let mut buf = [0u8; HEADER_LEN];
        buf[0] = 99;
        assert!(matches!(
            Header::decode(&buf),
            Err(Error::Protocol("unsupported protocol version"))
        ));
    }

    #[test]
    fn rejects_unknown_type() {
        let mut buf = [0u8; HEADER_LEN];
        buf[0] = PROTOCOL_VERSION;
        buf[1] = 99;
        assert!(matches!(
            Header::decode(&buf),
            Err(Error::Protocol("unknown frame type"))
        ));
    }
}
