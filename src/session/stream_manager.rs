use std::collections::HashMap;

use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::{
    alloc::StreamId,
    error::Error,
    frame::{Cmd, Frame},
};

// Internal stream manager that maintains active stream registry channel
pub(crate) struct StreamManager {
    streams: Mutex<HashMap<StreamId, StreamHandle>>,
    stream_creation_tx: mpsc::UnboundedSender<StreamId>,
}

pub(crate) struct StreamHandle {
    frame_tx: mpsc::Sender<Frame>,
    remote_fin_tx: Option<oneshot::Sender<()>>,
}

impl StreamManager {
    pub fn new(stream_creation_tx: mpsc::UnboundedSender<StreamId>) -> Self {
        Self {
            streams: Mutex::new(HashMap::new()),
            stream_creation_tx,
        }
    }

    pub fn add_stream(
        &self,
        stream_id: StreamId,
        frame_tx: mpsc::Sender<Frame>,
        remote_fin_tx: oneshot::Sender<()>,
    ) -> Result<(), Error> {
        let stream_handle = StreamHandle {
            frame_tx,
            remote_fin_tx: Some(remote_fin_tx),
        };

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
        let stream_id = frame.header.stream_id;
        match frame.header.cmd {
            Cmd::Syn => self
                .stream_creation_tx
                .send(stream_id)
                .map_err(|_| Error::SendFrameFailed(stream_id)),
            Cmd::Fin => self
                .streams
                .lock()
                .get_mut(&stream_id)
                .ok_or(Error::SendMessageFailed)?
                .remote_fin_tx
                .take()
                .ok_or(Error::Internal("remote fin tx not found".to_string()))?
                .send(())
                .map_err(|_| Error::SendFrameFailed(stream_id)),
            Cmd::Psh => {
                let frame_tx = self
                    .streams
                    .lock()
                    .get(&stream_id)
                    .map(|s| s.frame_tx.clone())
                    .ok_or(Error::StreamNotFound(stream_id))?;
                frame_tx
                    .send(frame)
                    .await
                    .map_err(|_| Error::SendFrameFailed(stream_id))
            }
        }
    }
}
