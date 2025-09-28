use thiserror::Error;

use crate::frame::Frame;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid cmd: {0}")]
    InvalidCmd(u8),
    #[error("invalid version: {0}")]
    InvalidVersion(u8),
    #[error("io error: {0}")]
    Tmp(#[from] std::io::Error),
    #[error("data length too large (>{})", u16::MAX)]
    DataLengthTooLarge,
    #[error("duplicate stream id {0}")]
    DuplicateStream(u32),
    #[error("stream not found: {0}")]
    StreamNotFound(u32),
    #[error("failed to send frame to stream: {0}")]
    SendFrameFailed(#[from] tokio::sync::mpsc::error::SendError<Frame>),
    #[error("stream {0} not writable")]
    StreamNotWritable(u32),
    #[error("stream {0} not readable")]
    StreamNotReadable(u32),
}
