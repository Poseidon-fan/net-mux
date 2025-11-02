//! Session module provides session management functionality for network multiplexing
//!
//! A `Session` allows you to create multiple independent streams over a single underlying
//! connection, enabling efficient multiplexing of network traffic. Each stream has its
//! own unique ID and can be used for bidirectional data transmission.
//!
//! # Example
//!
//! ```no_run
//! use net_mux::{Session, Config};
//! use tokio::net::TcpStream;
//!
//! #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a client session
//! let tcp_stream = TcpStream::connect("127.0.0.1:8080").await?;
//! let config = Config::default();
//! let session = Session::client(tcp_stream, config);
//!
//! // Open a new stream
//! let stream = session.open().await?;
//! // Use the stream for data transmission...
//! # Ok(())
//! # }
//! ```

mod stream_manager;
mod task;

use std::marker::PhantomData;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use parking_lot::Once;
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc, oneshot},
};

use crate::{
    Config, Stream,
    alloc::{EVEN_START_STREAM_ID, ODD_START_STREAM_ID, StreamId, StreamIdAllocator},
    consts::{CLIENT_MODE, SERVER_MODE, SessionMode},
    error::Error,
    msg::{self, Message},
    session::stream_manager::StreamManager,
};

/// Network multiplexing session
///
/// `Session` is the core component of network multiplexing, managing multiple independent streams
/// over a single underlying connection. Each session can handle multiple streams simultaneously,
/// with each stream having its own unique stream ID and lifecycle.
pub struct Session<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    _config: Config,
    stream_id_allocator: StreamIdAllocator,
    stream_manager: Arc<StreamManager>,

    stream_creation_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<StreamId>>,

    shutdown_tx: broadcast::Sender<()>,
    shutdown_once: Once,
    is_shutdown: AtomicBool,

    // contains here to copy to new Stream
    msg_tx: mpsc::UnboundedSender<Message>,
    close_tx: mpsc::UnboundedSender<StreamId>,

    _phantom: PhantomData<T>,
}

impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> Session<T> {
    fn new(conn: T, config: Config, mode: SessionMode) -> Self {
        let (conn_reader, conn_writer) = io::split(conn);
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (close_tx, close_rx) = mpsc::unbounded_channel();
        let (stream_creation_tx, stream_creation_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();

        let session = Self {
            _config: config,
            stream_id_allocator: StreamIdAllocator::new(match mode {
                SERVER_MODE => ODD_START_STREAM_ID,
                CLIENT_MODE => EVEN_START_STREAM_ID,
            }),
            stream_manager: Arc::new(StreamManager::new(stream_creation_tx)),
            stream_creation_rx: tokio::sync::Mutex::new(stream_creation_rx),
            shutdown_tx,
            shutdown_once: Once::new(),
            is_shutdown: AtomicBool::new(false),
            msg_tx,
            close_tx,
            _phantom: PhantomData,
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

    /// Create a server session.
    pub fn server(conn: T, config: Config) -> Self {
        Self::new(conn, config, SERVER_MODE)
    }

    /// Create a client session.
    pub fn client(conn: T, config: Config) -> Self {
        Self::new(conn, config, CLIENT_MODE)
    }

    /// Open a new stream
    ///
    /// This method allocates a new stream ID, creates the corresponding stream object,
    /// and sends a SYN frame to establish the connection.
    /// The returned stream can be used for data transmission.
    pub async fn open(&self) -> Result<Stream, Error> {
        if self.is_shutdown.load(Ordering::SeqCst) {
            return Err(Error::SessionClosed);
        }
        let shutdown_rx = self.shutdown_tx.subscribe();
        let close_tx = self.close_tx.clone();
        let msg_tx = self.msg_tx.clone();
        let stream_id = self.stream_id_allocator.allocate();
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let (remote_fin_tx, remote_fin_rx) = oneshot::channel();

        let stream = Stream::new(
            stream_id,
            shutdown_rx,
            msg_tx.clone(),
            frame_rx,
            close_tx,
            remote_fin_rx,
        );
        let (remote_ack_tx, remote_ack_rx) = oneshot::channel();
        self.stream_manager
            .add_stream(stream_id, frame_tx, remote_fin_tx, Some(remote_ack_tx))?;
        msg::send_syn(msg_tx, stream_id).await?;
        remote_ack_rx
            .await
            .map_err(|_| Error::Internal("remote ack rx not found".to_string()))?;

        Ok(stream)
    }

    /// Accept a new stream connection
    ///
    /// This method waits for stream creation requests from the remote end,
    /// then creates the corresponding stream object.
    /// Typically used by servers to accept connection requests from clients.
    pub async fn accept(&self) -> Result<Stream, Error> {
        let stream_id = self
            .stream_creation_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or(Error::SessionClosed)?;

        let shutdown_rx = self.shutdown_tx.subscribe();
        let close_tx = self.close_tx.clone();
        let msg_tx = self.msg_tx.clone();
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let (remote_fin_tx, remote_fin_rx) = oneshot::channel();

        let stream = Stream::new(
            stream_id,
            shutdown_rx,
            msg_tx,
            frame_rx,
            close_tx,
            remote_fin_rx,
        );
        self.stream_manager
            .add_stream(stream_id, frame_tx, remote_fin_tx, None)?;
        msg::send_ack(self.msg_tx.clone(), stream_id).await?;

        Ok(stream)
    }

    /// Close the session
    ///
    /// This method gracefully closes the session, including:
    /// - Setting the shutdown flag
    /// - Notifying all background tasks to stop
    /// - Closing the underlying connection
    ///
    /// Note: After calling this method, the session can no longer be used to send data.
    pub fn close(self) {
        self.shutdown_once.call_once(|| {
            self.is_shutdown.store(true, Ordering::SeqCst);
            let _ = self.shutdown_tx.send(());
        });
    }
}
