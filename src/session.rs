use std::{collections::HashMap, sync::Arc};

use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tokio::{
    io::{self, AsyncRead, AsyncWrite, AsyncWriteExt},
    select,
    sync::{broadcast, mpsc},
};
use tokio_util::codec::{FramedRead, FramedWrite};

use crate::{
    Config, StreamId,
    alloc::{EVEN_START_STREAM_ID, ODD_START_STREAM_ID, StreamIdAllocator},
    consts::{CLIENT_MODE, SERVER_MODE, SessionMode},
    frame::{Frame, FrameCodec},
    msg::Message,
};

pub struct Session {
    config: Config,
    stream_id_allocator: StreamIdAllocator,
    streams: Arc<Mutex<HashMap<StreamId, StreamHandle>>>,

    shutdown_tx: broadcast::Sender<()>,

    // contains here to copy to new Stream
    msg_tx: mpsc::Sender<Message>,
    close_tx: mpsc::UnboundedSender<(StreamId, Option<()>, Option<()>)>,
}

struct StreamHandle {
    pub frame_tx: mpsc::Sender<Frame>,
    pub readable: bool,
    pub writable: bool,
}

impl Session {
    async fn new(
        conn: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
        config: Config,
        mode: SessionMode,
    ) -> Self {
        let (conn_reader, conn_writer) = io::split(conn);
        let (msg_tx, msg_rx) = mpsc::channel(config.send_window);
        let (close_tx, close_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();

        let session = Self {
            config,
            stream_id_allocator: StreamIdAllocator::new(match mode {
                SERVER_MODE => ODD_START_STREAM_ID,
                CLIENT_MODE => EVEN_START_STREAM_ID,
            }),
            streams: Arc::new(Mutex::new(HashMap::new())),
            shutdown_tx,
            msg_tx,
            close_tx,
        };

        tokio::spawn(start_msg_collect_loop(msg_rx, conn_writer, shutdown_rx1));
        tokio::spawn(start_frame_dispatch_loop(
            conn_reader,
            session.streams.clone(),
            shutdown_rx2,
        ));
        tokio::spawn(start_stream_close_listen(
            close_rx,
            session.streams.clone(),
            shutdown_rx3,
        ));

        session
    }
}

async fn start_msg_collect_loop(
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

async fn start_frame_dispatch_loop(
    mut conn_reader: impl AsyncRead + Unpin,
    streams: Arc<Mutex<HashMap<StreamId, StreamHandle>>>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut frame_reader = FramedRead::new(&mut conn_reader, FrameCodec);
    loop {
        select! {
            frame = frame_reader.next() => {
                match frame {
                    Some(Ok(frame)) => {
                        let frame_tx = streams.lock().get(&frame.header.stream_id).map(|s| s.frame_tx.clone());
                        if let Some(tx) = frame_tx {
                            let _ = tx.send(frame).await;
                        }
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

async fn start_stream_close_listen(
    mut close_rx: mpsc::UnboundedReceiver<(StreamId, Option<()>, Option<()>)>,
    streams: Arc<Mutex<HashMap<StreamId, StreamHandle>>>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    loop {
        select! {
            close = close_rx.recv() => {
                match close {
                    Some((stream_id, close_read, close_write)) => {
                        let should_remove = {
                            let mut streams_guard = streams.lock();
                            if let Some(stream) = streams_guard.get_mut(&stream_id) {
                                if close_read.is_some() {
                                    stream.readable = false;
                                }
                                if close_write.is_some() {
                                    stream.writable = false;
                                }
                                !stream.readable && !stream.writable
                            } else {
                                false
                            }
                        };

                        if should_remove {
                            let _ = streams.lock().remove(&stream_id);
                        }
                    }
                    None => {
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
