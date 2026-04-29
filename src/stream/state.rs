//! Lock-free read/write state for a stream.
//!
//! A `Stream` lives on three independent boolean axes:
//!
//! * `read_open` — we may still observe inbound `Data`. Cleared by remote
//!   `FIN` / `RST`, by session shutdown, or by local reset.
//! * `write_open` — we may still emit outbound `Data`. Cleared by local
//!   `FIN` / `RST` or by session shutdown.
//! * `reset` — sticky bit indicating an abnormal termination. When set,
//!   read/write both surface `ConnectionReset` regardless of the open flags.
//!
//! Combining the axes covers the half-closed states without explicitly
//! enumerating them.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub(crate) struct StreamState {
    read_open: AtomicBool,
    write_open: AtomicBool,
    reset: AtomicBool,
}

impl StreamState {
    pub(crate) fn new() -> Self {
        Self {
            read_open: AtomicBool::new(true),
            write_open: AtomicBool::new(true),
            reset: AtomicBool::new(false),
        }
    }

    pub(crate) fn read_open(&self) -> bool {
        self.read_open.load(Ordering::Acquire)
    }

    pub(crate) fn write_open(&self) -> bool {
        self.write_open.load(Ordering::Acquire)
    }

    pub(crate) fn is_reset(&self) -> bool {
        self.reset.load(Ordering::Acquire)
    }

    /// Close the read half. Returns `true` the first time it transitions.
    pub(crate) fn close_read(&self) -> bool {
        self.read_open.swap(false, Ordering::AcqRel)
    }

    /// Close the write half. Returns `true` the first time it transitions.
    pub(crate) fn close_write(&self) -> bool {
        self.write_open.swap(false, Ordering::AcqRel)
    }

    /// Mark the stream as reset and close both halves. Returns `true` the
    /// first time the reset bit transitions from `false` to `true` so the
    /// caller can emit exactly one `RST` frame.
    pub(crate) fn mark_reset(&self) -> bool {
        let first = !self.reset.swap(true, Ordering::AcqRel);
        self.read_open.store(false, Ordering::Release);
        self.write_open.store(false, Ordering::Release);
        first
    }
}
