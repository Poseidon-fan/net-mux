use std::sync::atomic::{AtomicU32, Ordering};

pub type StreamId = u32;

pub(crate) const ODD_START_STREAM_ID: StreamId = 0x01;
pub(crate) const EVEN_START_STREAM_ID: StreamId = 0x02;

pub(crate) struct StreamIdAllocator(AtomicU32);

impl StreamIdAllocator {
    pub fn new(start: StreamId) -> Self {
        Self(AtomicU32::new(start))
    }

    pub fn allocate(&self) -> StreamId {
        self.0.fetch_add(2, Ordering::Relaxed)
    }
}
