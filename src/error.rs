use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid cmd: {0}")]
    InvalidCmd(u8),
    #[error("invalid version: {0}")]
    InvalidVersion(u8),
    #[error("frame decode error: {0}")]
    Tmp(#[from] std::io::Error),
    #[error("data length too large (>{})", u16::MAX)]
    DataLengthTooLarge,
}
