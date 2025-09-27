use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
};

use crate::{
    Config, StreamId,
    alloc::{EVEN_START_STREAM_ID, ODD_START_STREAM_ID, StreamIdAllocator},
    consts::{CLIENT_MODE, SERVER_MODE, SessionMode},
    frame::Frame,
    msg::Message,
};

pub struct Session {
    config: Config,
    stream_id_allocator: StreamIdAllocator,
    streams: Arc<Mutex<HashMap<StreamId, StreamHandle>>>,

    shutdown_tx: broadcast::Sender<()>,

    // contains here to copy to new Stream
    msg_tx: mpsc::Sender<Message>,
    close_tx: mpsc::UnboundedSender<StreamId>,
}

struct StreamHandle {
    pub frame_tx: mpsc::Sender<Frame>,
}

impl Session {
    async fn new(
        conn: impl AsyncRead + AsyncWrite + Send + 'static,
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
    msg_rx: mpsc::Receiver<Message>,
    conn_writer: impl AsyncWrite,
    shutdown_rx: broadcast::Receiver<()>,
) {
}

async fn start_frame_dispatch_loop(
    conn_reader: impl AsyncRead,
    streams: Arc<Mutex<HashMap<StreamId, StreamHandle>>>,
    shutdown_rx: broadcast::Receiver<()>,
) {
}

async fn start_stream_close_listen(
    close_rx: mpsc::UnboundedReceiver<StreamId>,
    streams: Arc<Mutex<HashMap<StreamId, StreamHandle>>>,
    shutdown_rx: broadcast::Receiver<()>,
) {
}
