//! User-facing logical stream and its internal state.

mod inner;
mod recv;
mod state;

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub(crate) use inner::{Origin, StreamInner};

/// One bidirectional logical stream multiplexed over a session.
///
/// `Stream` implements [`AsyncRead`] and [`AsyncWrite`] and otherwise
/// behaves like an ordinary connected byte-pipe. Dropping it always sends
/// `FIN` to the peer and unregisters the stream from its session.
pub struct Stream {
    inner: Arc<StreamInner>,
}

impl Stream {
    pub(crate) fn from_inner(inner: Arc<StreamInner>) -> Self {
        Self { inner }
    }

    /// Identifier assigned to this stream by its session.
    pub fn id(&self) -> u32 {
        self.inner.id
    }

    /// Abruptly terminate the stream. Sends a `RST` frame, drops any pending
    /// inbound data, and prevents further reads / writes.
    ///
    /// Use this for error paths where you cannot drain the stream cleanly.
    /// In normal control flow prefer [`AsyncWrite::poll_shutdown`] / dropping
    /// the stream so that the peer observes a graceful `FIN`.
    pub fn reset(&self) {
        self.inner.local_reset();
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.inner.poll_read(cx, buf)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // The writer task drains the outbound queue continuously; once
        // `poll_write` reports success the frame is enqueued, so flush is
        // a no-op at the protocol layer. (We do not synchronously confirm
        // delivery, mirroring the semantics of `BufWriter::flush` over a
        // socket.)
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.inner.local_fin();
        Poll::Ready(Ok(()))
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.inner.on_user_drop();
    }
}
