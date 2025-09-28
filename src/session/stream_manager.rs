use std::collections::HashMap;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::{StreamId, error::Error, frame::Frame};

pub(crate) struct StreamManager {
    streams: Mutex<HashMap<StreamId, StreamHandle>>,
}

pub(crate) struct StreamHandle {
    pub frame_tx: mpsc::Sender<Frame>,
}

impl StreamManager {
    pub fn new() -> Self {
        Self {
            streams: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_stream(
        &self,
        stream_id: StreamId,
        frame_tx: mpsc::Sender<Frame>,
    ) -> Result<(), Error> {
        let stream_handle = StreamHandle { frame_tx };

        let mut streams_guard = self.streams.lock();
        if streams_guard.contains_key(&stream_id) {
            return Err(Error::DuplicateStream(stream_id));
        }

        streams_guard.insert(stream_id, stream_handle);
        Ok(())
    }

    pub fn remove_stream(&self, stream_id: StreamId) -> Result<(), Error> {
        let mut streams_guard = self.streams.lock();
        if !streams_guard.contains_key(&stream_id) {
            return Err(Error::StreamNotFound(stream_id));
        }

        streams_guard.remove(&stream_id);
        Ok(())
    }

    pub async fn dispatch_frame_to_stream(&self, frame: Frame) -> Result<(), Error> {
        let frame_tx = self
            .streams
            .lock()
            .get(&frame.header.stream_id)
            .map(|s| s.frame_tx.clone());
        if let Some(frame_tx) = frame_tx {
            frame_tx.send(frame).await.map_err(Error::SendFrameFailed)
        } else {
            Err(Error::StreamNotFound(frame.header.stream_id))
        }
    }
}
