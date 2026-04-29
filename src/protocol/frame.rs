//! Strongly-typed in-memory frame representation.

use bitflags::bitflags;
use bytes::Bytes;

use crate::error::ErrorCode;

bitflags! {
    /// Bit flags carried in the header `flags` field.
    ///
    /// Multiple flags may be combined in a single frame; for example `SYN |
    /// ACK` on the first `Data` frame echoed back to confirm a stream open.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub(crate) struct Flags: u16 {
        /// New stream is being opened.
        const SYN = 1 << 0;
        /// Acknowledgement of a SYN.
        const ACK = 1 << 1;
        /// Sender will not transmit any more data on this stream.
        const FIN = 1 << 2;
        /// Stream is being abruptly reset.
        const RST = 1 << 3;
    }
}

/// In-memory representation of a single wire frame.
#[derive(Debug, Clone)]
pub(crate) enum Frame {
    /// User-payload data frame; may also carry stream lifecycle flags.
    Data {
        stream_id: u32,
        flags: Flags,
        payload: Bytes,
    },
    /// Credit increment for a stream's send window.
    WindowUpdate {
        stream_id: u32,
        flags: Flags,
        delta: u32,
    },
    /// Liveness probe / RTT measurement. `flags` carries `ACK` for replies.
    Ping { flags: Flags, opaque: u32 },
    /// Final frame of a session; recipient stops opening new streams.
    GoAway { error_code: ErrorCode },
}

impl Frame {
    pub(crate) fn data(stream_id: u32, flags: Flags, payload: Bytes) -> Self {
        Frame::Data {
            stream_id,
            flags,
            payload,
        }
    }

    pub(crate) fn syn(stream_id: u32) -> Self {
        Frame::Data {
            stream_id,
            flags: Flags::SYN,
            payload: Bytes::new(),
        }
    }

    pub(crate) fn ack(stream_id: u32) -> Self {
        Frame::Data {
            stream_id,
            flags: Flags::ACK,
            payload: Bytes::new(),
        }
    }

    pub(crate) fn fin(stream_id: u32) -> Self {
        Frame::Data {
            stream_id,
            flags: Flags::FIN,
            payload: Bytes::new(),
        }
    }

    pub(crate) fn rst(stream_id: u32) -> Self {
        Frame::Data {
            stream_id,
            flags: Flags::RST,
            payload: Bytes::new(),
        }
    }

    pub(crate) fn window_update(stream_id: u32, delta: u32) -> Self {
        Frame::WindowUpdate {
            stream_id,
            flags: Flags::empty(),
            delta,
        }
    }

    pub(crate) fn ping(opaque: u32) -> Self {
        Frame::Ping {
            flags: Flags::empty(),
            opaque,
        }
    }

    pub(crate) fn pong(opaque: u32) -> Self {
        Frame::Ping {
            flags: Flags::ACK,
            opaque,
        }
    }

    pub(crate) fn go_away(error_code: ErrorCode) -> Self {
        Frame::GoAway { error_code }
    }
}
