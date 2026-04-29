//! Per-stream flow-control primitives.

mod window;

pub(crate) use window::{AcquireOutcome, RecvWindow, SendWindow};
