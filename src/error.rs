//! Error types for the library.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid cmd: {0}")]
    InvalidCmd(u8),
    #[error("invalid version: {0}")]
    InvalidVersion(u8),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("data length too large (>{})", u16::MAX)]
    DataLengthTooLarge,
    #[error("duplicate stream id {0}")]
    DuplicateStream(u32),
    #[error("stream not found: {0}")]
    StreamNotFound(u32),
    #[error("stream {0} not writable")]
    StreamNotWritable(u32),
    #[error("stream {0} not readable")]
    StreamNotReadable(u32),
    #[error("failed to send frame to stream {0}")]
    SendFrameFailed(u32),
    #[error("session is closed")]
    SessionClosed,
    #[error("failed to send message to session")]
    SendMessageFailed,
    #[error("internal error: {0}")]
    Internal(String),
}
