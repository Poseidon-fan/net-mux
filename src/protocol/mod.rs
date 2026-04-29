//! Wire protocol: frame definitions and codec.
//!
//! All on-the-wire framing is defined here. Higher layers only deal in
//! [`Frame`] values; the binary format is encapsulated in [`FrameCodec`].

mod codec;
mod frame;
mod header;

pub(crate) use codec::FrameCodec;
pub(crate) use frame::{Flags, Frame};
