mod stream_manager;
mod task;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use parking_lot::Once;
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
};

use crate::{
    Config, Stream, StreamId,
    alloc::{EVEN_START_STREAM_ID, ODD_START_STREAM_ID, StreamIdAllocator},
    consts::{CLIENT_MODE, SERVER_MODE, SessionMode},
    error::Error,
    msg::{self, Message},
    session::stream_manager::StreamManager,
};

pub struct Session {
    config: Config,
    stream_id_allocator: StreamIdAllocator,
    stream_manager: Arc<StreamManager>,

    stream_creation_rx: mpsc::UnboundedReceiver<StreamId>,

    shutdown_tx: broadcast::Sender<()>,
    shutdown_once: Once,
    is_shutdown: AtomicBool,

    // contains here to copy to new Stream
    msg_tx: mpsc::Sender<Message>,
    close_tx: mpsc::UnboundedSender<StreamId>,
}

impl Session {
    fn new(
        conn: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
        config: Config,
        mode: SessionMode,
    ) -> Self {
        let (conn_reader, conn_writer) = io::split(conn);
        let (msg_tx, msg_rx) = mpsc::channel(config.conn_send_window_size);
        let (close_tx, close_rx) = mpsc::unbounded_channel();
        let (stream_creation_tx, stream_creation_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();

        let session = Self {
            config,
            stream_id_allocator: StreamIdAllocator::new(match mode {
                SERVER_MODE => ODD_START_STREAM_ID,
                CLIENT_MODE => EVEN_START_STREAM_ID,
            }),
            stream_manager: Arc::new(StreamManager::new(stream_creation_tx)),
            stream_creation_rx,
            shutdown_tx,
            shutdown_once: Once::new(),
            is_shutdown: AtomicBool::new(false),
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

    pub fn server(
        conn: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
        config: Config,
    ) -> Self {
        Self::new(conn, config, SERVER_MODE)
    }

    pub fn client(
        conn: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
        config: Config,
    ) -> Self {
        Self::new(conn, config, CLIENT_MODE)
    }

    pub async fn open(&self) -> Result<Stream, Error> {
        if self.is_shutdown.load(Ordering::SeqCst) {
            return Err(Error::SessionClosed);
        }
        let shutdown_rx = self.shutdown_tx.subscribe();
        let close_tx = self.close_tx.clone();
        let msg_tx = self.msg_tx.clone();
        let stream_id = self.stream_id_allocator.allocate();
        let (frame_tx, frame_rx) = mpsc::channel(self.config.stream_recv_window_size);

        let stream = Stream::new(stream_id, shutdown_rx, msg_tx.clone(), frame_rx, close_tx);
        msg::send_syn(msg_tx, stream_id).await?;
        self.stream_manager.add_stream(stream_id, frame_tx)?;

        Ok(stream)
    }

    pub async fn accept(&mut self) -> Result<Stream, Error> {
        let stream_id = self
            .stream_creation_rx
            .recv()
            .await
            .ok_or(Error::SessionClosed)?;

        let shutdown_rx = self.shutdown_tx.subscribe();
        let close_tx = self.close_tx.clone();
        let msg_tx = self.msg_tx.clone();
        let (frame_tx, frame_rx) = mpsc::channel(self.config.stream_recv_window_size);

        let stream = Stream::new(stream_id, shutdown_rx, msg_tx, frame_rx, close_tx);
        self.stream_manager.add_stream(stream_id, frame_tx)?;

        Ok(stream)
    }

    pub fn close(self) {
        self.shutdown_once.call_once(|| {
            self.is_shutdown.store(true, Ordering::SeqCst);
            let _ = self.shutdown_tx.send(());
        });
    }
}
