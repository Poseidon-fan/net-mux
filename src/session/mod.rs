mod stream_manager;
mod task;

use std::sync::Arc;

use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
};

use crate::{
    Config, StreamId,
    alloc::{EVEN_START_STREAM_ID, ODD_START_STREAM_ID, StreamIdAllocator},
    consts::{CLIENT_MODE, SERVER_MODE, SessionMode},
    msg::Message,
    session::stream_manager::StreamManager,
};

pub struct Session {
    config: Config,
    stream_id_allocator: StreamIdAllocator,
    stream_manager: Arc<StreamManager>,

    shutdown_tx: broadcast::Sender<()>,

    // contains here to copy to new Stream
    msg_tx: mpsc::Sender<Message>,
    close_tx: mpsc::UnboundedSender<(StreamId, Option<()>, Option<()>)>,
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
            stream_manager: Arc::new(StreamManager::new()),
            shutdown_tx,
            msg_tx,
            close_tx,
        };

        tokio::spawn(task::start_msg_collect_loop(
            msg_rx,
            conn_writer,
            shutdown_rx1,
        ));
        tokio::spawn(task::start_frame_dispatch_loop(
            conn_reader,
            session.stream_manager.clone(),
            shutdown_rx2,
        ));
        tokio::spawn(task::start_stream_close_listen(
            close_rx,
            session.stream_manager.clone(),
            shutdown_rx3,
        ));

        session
    }
}
