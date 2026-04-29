//! Error and result types.

use std::io;

use thiserror::Error;

/// Crate-wide `Result` alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error type.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The session is closed and no further operations can be performed.
    #[error("session is closed")]
    SessionClosed,

    /// The remote peer sent a `GoAway` frame and is no longer accepting new
    /// streams.
    #[error("remote sent GoAway: {0}")]
    GoAway(ErrorCode),

    /// The peer reset the stream.
    #[error("stream {0} was reset by peer")]
    StreamReset(u32),

    /// `Config::max_streams` would be exceeded.
    #[error("too many concurrent streams (limit {0})")]
    TooManyStreams(usize),

    /// An operation timed out.
    #[error("operation timed out")]
    Timeout,

    /// Wire-format violation while decoding a frame.
    #[error("protocol error: {0}")]
    Protocol(&'static str),

    /// Underlying I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

impl From<Error> for io::Error {
    fn from(err: Error) -> io::Error {
        match err {
            Error::Io(e) => e,
            Error::SessionClosed | Error::GoAway(_) => {
                io::Error::new(io::ErrorKind::NotConnected, err)
            }
            Error::StreamReset(_) => io::Error::new(io::ErrorKind::ConnectionReset, err),
            Error::Timeout => io::Error::new(io::ErrorKind::TimedOut, err),
            Error::Protocol(_) => io::Error::new(io::ErrorKind::InvalidData, err),
            Error::TooManyStreams(_) => io::Error::other(err),
        }
    }
}

/// Wire-level error code, transmitted in `GoAway` frames as a `u32`.
///
/// Unknown codes coming from the peer are preserved as [`ErrorCode::Unknown`]
/// so applications can still observe them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Graceful shutdown initiated locally or remotely.
    Normal,
    /// Wire-format / framing violation.
    ProtocolError,
    /// Internal bug in either side.
    InternalError,
    /// Peer violated flow control (e.g. sent more bytes than advertised).
    FlowControlError,
    /// Maximum concurrent streams would be exceeded.
    StreamLimit,
    /// Protocol version mismatch.
    InvalidVersion,
    /// Idle / keepalive timeout fired.
    Timeout,
    /// Code not recognised by this version of the library.
    Unknown(u32),
}

impl ErrorCode {
    /// Encode the variant as the raw `u32` carried on the wire.
    pub const fn as_u32(self) -> u32 {
        match self {
            ErrorCode::Normal => 0,
            ErrorCode::ProtocolError => 1,
            ErrorCode::InternalError => 2,
            ErrorCode::FlowControlError => 3,
            ErrorCode::StreamLimit => 4,
            ErrorCode::InvalidVersion => 5,
            ErrorCode::Timeout => 6,
            ErrorCode::Unknown(v) => v,
        }
    }
}

impl From<u32> for ErrorCode {
    fn from(v: u32) -> Self {
        match v {
            0 => ErrorCode::Normal,
            1 => ErrorCode::ProtocolError,
            2 => ErrorCode::InternalError,
            3 => ErrorCode::FlowControlError,
            4 => ErrorCode::StreamLimit,
            5 => ErrorCode::InvalidVersion,
            6 => ErrorCode::Timeout,
            other => ErrorCode::Unknown(other),
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::Normal => f.write_str("normal"),
            ErrorCode::ProtocolError => f.write_str("protocol error"),
            ErrorCode::InternalError => f.write_str("internal error"),
            ErrorCode::FlowControlError => f.write_str("flow control error"),
            ErrorCode::StreamLimit => f.write_str("stream limit"),
            ErrorCode::InvalidVersion => f.write_str("invalid version"),
            ErrorCode::Timeout => f.write_str("timeout"),
            ErrorCode::Unknown(v) => write!(f, "unknown error code {v}"),
        }
    }
}
