//! Per-session stream registry.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::error::Error;
use crate::stream::StreamInner;
use crate::util::id::StreamId;

/// Thread-safe map of stream id to its shared state.
///
/// The registry owns a strong reference for as long as the stream is alive,
/// so the session reader can dispatch frames concurrently with the user's
/// `Stream` handle. When [`Self::remove`] is called (typically from the
/// closer task) the entry is dropped; once the user's `Stream` is also
/// dropped the underlying `StreamInner` is freed.
#[derive(Default)]
pub(crate) struct StreamRegistry {
    inner: Mutex<HashMap<StreamId, Arc<StreamInner>>>,
}

impl std::fmt::Debug for StreamRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamRegistry")
            .field("len", &self.inner.lock().len())
            .finish()
    }
}

impl StreamRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Insert a new stream. Errors if the id is already in use, which would
    /// indicate either a peer protocol violation or an allocator bug.
    pub(crate) fn insert(&self, id: StreamId, stream: Arc<StreamInner>) -> Result<(), Error> {
        let mut g = self.inner.lock();
        if g.contains_key(&id) {
            return Err(Error::Protocol("duplicate stream id"));
        }
        g.insert(id, stream);
        Ok(())
    }

    pub(crate) fn get(&self, id: StreamId) -> Option<Arc<StreamInner>> {
        self.inner.lock().get(&id).cloned()
    }

    pub(crate) fn remove(&self, id: StreamId) -> Option<Arc<StreamInner>> {
        self.inner.lock().remove(&id)
    }

    pub(crate) fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Drain the entire registry, returning every contained stream.
    pub(crate) fn drain(&self) -> Vec<Arc<StreamInner>> {
        let mut g = self.inner.lock();
        g.drain().map(|(_, v)| v).collect()
    }
}
