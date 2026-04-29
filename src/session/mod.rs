//! High-level session type.
//!
//! A [`Session`] wraps a single byte-oriented connection (typically a TCP
//! socket) and exposes [`Session::open`] / [`Session::accept`] to multiplex
//! many [`Stream`](crate::Stream)s over it.
//!
//! ```no_run
//! use net_mux::{Config, Session};
//! # async fn run(c: tokio::net::TcpStream) -> anyhow::Result<()> {
//! let session = Session::client(c, Config::default());
//! let mut stream = session.open().await?;
//! tokio::io::AsyncWriteExt::write_all(&mut stream, b"hello").await?;
//! session.close().await;
//! # Ok(()) }
//! ```

mod inner;
mod keepalive;
mod manager;
mod reader;
mod writer;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{self, AsyncRead, AsyncWrite};
use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::timeout;
use tracing::trace;

use crate::config::Config;
use crate::error::{Error, ErrorCode, Result};
use crate::protocol::Frame;
use crate::stream::{Origin, Stream, StreamInner};
use crate::util::id::{Role, StreamIdAllocator};

use inner::SessionInner;
use manager::StreamRegistry;

/// Multiplexed session over a single connection.
///
/// `Session` is cheap to clone — it is internally an `Arc` — so multiple
/// tasks can share the same session and concurrently call [`open`] / read
/// from streams. [`accept`], by contrast, expects a single owner.
///
/// [`open`]: Session::open
/// [`accept`]: Session::accept
#[derive(Clone)]
pub struct Session {
    inner: Arc<SessionInner>,
}

impl Session {
    /// Construct a session that initiates new streams (odd ids).
    pub fn client<C>(conn: C, config: Config) -> Self
    where
        C: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        Self::new(conn, config, Role::Client)
    }

    /// Construct a session that accepts streams from the peer (even ids).
    pub fn server<C>(conn: C, config: Config) -> Self
    where
        C: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        Self::new(conn, config, Role::Server)
    }

    fn new<C>(conn: C, config: Config, role: Role) -> Self
    where
        C: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let config = Arc::new(config);
        let (read_half, write_half) = io::split(conn);

        let (out_tx, out_rx) = mpsc::unbounded_channel::<Frame>();
        let (closer_tx, closer_rx) = mpsc::unbounded_channel::<u32>();
        let (accept_tx, accept_rx) = mpsc::unbounded_channel::<Arc<StreamInner>>();
        let (pong_tx, pong_rx) = mpsc::unbounded_channel::<u32>();
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);

        let inner = Arc::new(SessionInner {
            config: config.clone(),
            role,
            id_alloc: StreamIdAllocator::new(role),
            registry: StreamRegistry::new(),
            out_tx: out_tx.clone(),
            closer_tx,
            accept_tx,
            accept_rx: AsyncMutex::new(accept_rx),
            shutdown_tx,
            is_closing: AtomicBool::new(false),
            peer_gone: AtomicBool::new(false),
            tasks: AsyncMutex::new(None),
        });

        let mut joinset = JoinSet::new();

        joinset.spawn(writer::run(
            inner.clone(),
            out_rx,
            write_half,
            inner.shutdown_rx(),
        ));

        joinset.spawn(reader::run(
            inner.clone(),
            read_half,
            pong_tx,
            inner.shutdown_rx(),
        ));

        joinset.spawn(closer_task(inner.clone(), closer_rx, inner.shutdown_rx()));

        if let Some(interval) = config.keepalive_interval {
            joinset.spawn(keepalive::run(
                inner.clone(),
                interval,
                config.keepalive_timeout,
                pong_rx,
                inner.shutdown_rx(),
            ));
        } else {
            // Drain the pong receiver so the reader's send never errors.
            drop(pong_rx);
        }

        // We can stash the JoinSet without awaiting the lock because no one
        // else can access `inner.tasks` until `Session::new` returns.
        if let Ok(mut guard) = inner.tasks.try_lock() {
            *guard = Some(joinset);
        }

        Self { inner }
    }

    /// Open a new outbound stream.
    ///
    /// Sends `SYN` to the peer and resolves once the peer's `ACK` arrives or
    /// `Config::open_timeout` elapses, whichever comes first.
    pub async fn open(&self) -> Result<Stream> {
        if self.inner.is_closed() || self.inner.peer_gone.load(Ordering::Acquire) {
            return Err(Error::SessionClosed);
        }
        if self.inner.registry.len() >= self.inner.config.max_streams {
            return Err(Error::TooManyStreams(self.inner.config.max_streams));
        }
        let id = self
            .inner
            .id_alloc
            .allocate()
            .ok_or(Error::TooManyStreams(usize::MAX))?;

        let stream_inner = StreamInner::new(
            id,
            Origin::Local,
            self.inner.config.clone(),
            self.inner.out_tx.clone(),
            self.inner.closer_tx.clone(),
        );
        self.inner.registry.insert(id, stream_inner.clone())?;

        if self.inner.out_tx.send(Frame::syn(id)).is_err() {
            self.inner.registry.remove(id);
            return Err(Error::SessionClosed);
        }

        match timeout(self.inner.config.open_timeout, stream_inner.wait_acked()).await {
            Ok(Ok(())) => Ok(Stream::from_inner(stream_inner)),
            Ok(Err(e)) => {
                self.inner.registry.remove(id);
                Err(e)
            }
            Err(_) => {
                let _ = self.inner.out_tx.send(Frame::rst(id));
                self.inner.registry.remove(id);
                Err(Error::Timeout)
            }
        }
    }

    /// Wait for the next stream opened by the peer.
    ///
    /// Multiple concurrent calls to `accept()` are serialised internally;
    /// one task at a time observes a new stream. Returns
    /// [`Error::SessionClosed`] once the session has shut down.
    pub async fn accept(&self) -> Result<Stream> {
        if self.inner.is_closed() {
            return Err(Error::SessionClosed);
        }
        let mut shutdown = self.inner.shutdown_rx();
        let stream_inner = {
            let mut rx = self.inner.accept_rx.lock().await;
            tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Err(Error::SessionClosed);
                    }
                    return Err(Error::SessionClosed);
                }
                next = rx.recv() => next.ok_or(Error::SessionClosed)?,
            }
        };
        let id = stream_inner.id;
        if self.inner.out_tx.send(Frame::ack(id)).is_err() {
            return Err(Error::SessionClosed);
        }
        Ok(Stream::from_inner(stream_inner))
    }

    /// Whether `close` has been called or the peer has gone away.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Begin a graceful shutdown, blocking until all background tasks have
    /// exited. Subsequent calls are no-ops.
    pub async fn close(&self) {
        self.inner.initiate_shutdown(ErrorCode::Normal);

        let mut guard = self.inner.tasks.lock().await;
        if let Some(mut set) = guard.take() {
            while let Some(res) = set.join_next().await {
                if let Err(e) = res {
                    trace!(error = ?e, "background task panicked or was cancelled");
                }
            }
        }
    }
}

async fn closer_task(
    inner: Arc<SessionInner>,
    mut rx: mpsc::UnboundedReceiver<u32>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { break; }
            }
            maybe_id = rx.recv() => {
                match maybe_id {
                    Some(id) => { inner.registry.remove(id); }
                    None => break,
                }
            }
        }
    }
}
