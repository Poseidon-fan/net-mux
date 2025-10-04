use std::sync::Arc;

use crate::{
    alloc::StreamId, frame::FrameCodec, msg::Message, session::stream_manager::StreamManager,
};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    select,
    sync::{broadcast, mpsc},
};
use tokio_util::codec::{FramedRead, FramedWrite};

// Collects the `Message` from `Stream`s,
// send the inner `Frame` out and write back the result `Stream`.
pub(crate) async fn start_msg_collect_loop(
    mut msg_rx: mpsc::Receiver<Message>,
    mut conn_writer: impl AsyncWrite + Unpin,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut frame_writer = FramedWrite::new(&mut conn_writer, FrameCodec);
    loop {
        select! {
            msg = msg_rx.recv() => {
                match msg {
                    Some(msg) => {
                        let bytes_written = msg.frame.frame_len();
                        let _ = msg.res_tx.send(frame_writer.send(msg.frame).await.map(|_| bytes_written));
                    }
                    None => {
                        // TODO(Poseidon): handle this case
                        return;
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                drop(msg_rx);
                let _ = conn_writer.shutdown().await;
                return;
            }
        }
    }
}

// Dispatches the `Frame` to `Streams` by `StreamManager`.
pub(crate) async fn start_frame_dispatch_loop(
    mut conn_reader: impl AsyncRead + Unpin,
    stream_manager: Arc<StreamManager>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut frame_reader = FramedRead::new(&mut conn_reader, FrameCodec);
    loop {
        select! {
            frame = frame_reader.next() => {
                match frame {
                    Some(Ok(frame)) => {
                        // TODO(Poseidon): handle error
                        let _ = stream_manager.dispatch_frame_to_stream(frame).await;
                    }
                    None => {
                        return;
                    }
                    Some(Err(e)) => {
                        return;
                    }
                }
            }

            _ = shutdown_rx.recv() => {
                return;
            }
        }
    }
}

// Listens the `Stream`s to be closed,
// and remove the `Stream` from `StreamManager`
pub(crate) async fn start_stream_close_listen(
    mut close_rx: mpsc::UnboundedReceiver<StreamId>,
    stream_manager: Arc<StreamManager>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    loop {
        select! {
            Some(stream_id) = close_rx.recv() => {
                let _ = stream_manager.remove_stream(stream_id);
            }

            _ = shutdown_rx.recv() => {
                return;
            }
        }
    }
}
