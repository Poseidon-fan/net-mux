use std::sync::atomic::{AtomicU32, Ordering};

// Unique identifier for a stream in the multiplexed connection.
pub type StreamId = u32;

pub(crate) const ODD_START_STREAM_ID: StreamId = 0x01;
pub(crate) const EVEN_START_STREAM_ID: StreamId = 0x02;

// Thread-safe stream ID allocator.
//
// Provides a simple interface for allocating unique stream IDs using an atomic counter.
// The allocator increments by 2 to ensure that client and server streams use
// different ID ranges, preventing collisions.
pub(crate) struct StreamIdAllocator(AtomicU32);

impl StreamIdAllocator {
    pub fn new(start: StreamId) -> Self {
        Self(AtomicU32::new(start))
    }

    // Allocates a new stream ID.
    //
    // Returns the current value and atomically increments the counter by 2.
    // This ensures that client and server streams use different ID ranges.
    pub fn allocate(&self) -> StreamId {
        self.0.fetch_add(2, Ordering::Relaxed)
    }
}
