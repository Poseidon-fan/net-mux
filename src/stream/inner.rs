//! Internal shared state of a [`Stream`](super::Stream).
//!
//! `StreamInner` is held by both the user's `Stream` handle and the
//! session's stream registry inside an `Arc`, so its API is `&self` only.

use std::future::poll_fn;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::task::AtomicWaker;
use parking_lot::Mutex;
use tokio::io::ReadBuf;
use tokio::sync::mpsc;
use tracing::trace;

use crate::config::Config;
use crate::error::Error;
use crate::flow::{AcquireOutcome, RecvWindow, SendWindow};
use crate::protocol::{Flags, Frame};
use crate::util::id::StreamId;

use super::recv::RecvBuffer;
use super::state::StreamState;

/// Whether the stream was created locally (and therefore owes a `SYN` to
/// the peer) or remotely (and therefore owes an `ACK`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Local,
    Remote,
}

pub(crate) struct StreamInner {
    pub(crate) id: StreamId,
    pub(crate) config: Arc<Config>,

    state: StreamState,
    send_window: SendWindow,
    recv_window: RecvWindow,

    recv: Mutex<RecvBuffer>,
    recv_waker: AtomicWaker,

    out_tx: mpsc::UnboundedSender<Frame>,
    closer_tx: mpsc::UnboundedSender<StreamId>,

    ack_received: AtomicBool,
    ack_waker: AtomicWaker,

    /// Suppresses repeated FIN / RST emission once we've already sent one.
    fin_sent: AtomicBool,
}

impl StreamInner {
    pub(crate) fn new(
        id: StreamId,
        origin: Origin,
        config: Arc<Config>,
        out_tx: mpsc::UnboundedSender<Frame>,
        closer_tx: mpsc::UnboundedSender<StreamId>,
    ) -> Arc<Self> {
        let initial_window = config.initial_stream_window;
        Arc::new(Self {
            id,
            config,
            state: StreamState::new(),
            send_window: SendWindow::new(initial_window),
            recv_window: RecvWindow::new(initial_window),
            recv: Mutex::new(RecvBuffer::new()),
            recv_waker: AtomicWaker::new(),
            out_tx,
            closer_tx,
            // Streams created in response to a peer SYN are immediately
            // established; locally-opened streams must await ACK.
            ack_received: AtomicBool::new(matches!(origin, Origin::Remote)),
            ack_waker: AtomicWaker::new(),
            fin_sent: AtomicBool::new(false),
        })
    }

    /// Whether the inbound half is still open.
    pub(crate) fn can_recv(&self) -> bool {
        self.state.read_open() && !self.state.is_reset()
    }

    // ------------------------------------------------------------------
    // Inbound dispatch (driven by the session's reader task)
    // ------------------------------------------------------------------

    /// Append a payload chunk delivered by the peer.
    pub(crate) fn push_data(&self, payload: Bytes) {
        if payload.is_empty() {
            return;
        }
        self.recv.lock().push(payload);
        self.recv_waker.wake();
    }

    /// Mark that the peer will send no further data.
    pub(crate) fn remote_fin(&self) {
        if self.state.close_read() {
            trace!(stream = self.id, "remote FIN");
        }
        self.recv_waker.wake();
    }

    /// Apply a peer-initiated reset.
    pub(crate) fn remote_reset(&self) {
        if self.state.mark_reset() {
            trace!(stream = self.id, "remote RST");
        }
        self.send_window.close();
        self.recv_waker.wake();
        self.ack_waker.wake();
        let _ = self.closer_tx.send(self.id);
    }

    /// Increase send credit by `delta`.
    pub(crate) fn grant_send_credit(&self, delta: u32) {
        self.send_window.grant(delta);
    }

    /// Mark the local stream as ACKed (peer accepted our SYN).
    pub(crate) fn mark_acked(&self) {
        self.ack_received.store(true, Ordering::Release);
        self.ack_waker.wake();
    }

    /// Resolve once the stream has been ACKed by the peer (or the open
    /// attempt has failed). Used by `Session::open` to honour the configured
    /// open timeout.
    pub(crate) async fn wait_acked(&self) -> Result<(), Error> {
        poll_fn(|cx| {
            if self.ack_received.load(Ordering::Acquire) {
                return Poll::Ready(Ok(()));
            }
            if self.state.is_reset() {
                return Poll::Ready(Err(Error::StreamReset(self.id)));
            }
            if !self.state.write_open() {
                return Poll::Ready(Err(Error::SessionClosed));
            }
            self.ack_waker.register(cx.waker());
            // Re-check after registering to avoid races with notifiers.
            if self.ack_received.load(Ordering::Acquire) {
                Poll::Ready(Ok(()))
            } else if self.state.is_reset() {
                Poll::Ready(Err(Error::StreamReset(self.id)))
            } else if !self.state.write_open() {
                Poll::Ready(Err(Error::SessionClosed))
            } else {
                Poll::Pending
            }
        })
        .await
    }

    /// Force the stream into a session-closed state.
    pub(crate) fn force_close(&self) {
        self.state.close_read();
        self.state.close_write();
        self.send_window.close();
        self.recv_waker.wake();
        self.ack_waker.wake();
    }

    // ------------------------------------------------------------------
    // Outbound (driven by the user's poll_read / poll_write)
    // ------------------------------------------------------------------

    pub(crate) fn poll_read(
        self: &Arc<Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        loop {
            if self.state.is_reset() {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "stream reset",
                )));
            }

            // Try to drain the queue.
            let consumed = {
                let mut q = self.recv.lock();
                q.read_into(buf)
            };

            if consumed > 0 {
                if let Some(delta) = self.recv_window.on_consume(consumed as u32) {
                    let _ = self.out_tx.send(Frame::window_update(self.id, delta));
                }
                return Poll::Ready(Ok(()));
            }

            if !self.state.read_open() {
                // EOF: peer FIN or session shutdown
                return Poll::Ready(Ok(()));
            }

            self.recv_waker.register(cx.waker());

            // Re-check after registering to avoid the lost-wakeup window.
            let still_empty = self.recv.lock().is_empty();
            if !still_empty {
                continue;
            }
            if !self.state.read_open() || self.state.is_reset() {
                continue;
            }
            return Poll::Pending;
        }
    }

    pub(crate) fn poll_write(
        self: &Arc<Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.state.is_reset() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "stream reset",
            )));
        }
        if !self.state.write_open() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "write half closed",
            )));
        }
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let max_frame = self.config.max_frame_size as usize;
        let want = buf.len().min(max_frame);
        let want = want.min(u32::MAX as usize) as u32;

        match self.send_window.poll_acquire(cx, want) {
            AcquireOutcome::Closed => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "send window closed",
            ))),
            AcquireOutcome::Pending => Poll::Pending,
            AcquireOutcome::Acquired(n) => {
                let payload = Bytes::copy_from_slice(&buf[..n as usize]);
                let frame = Frame::data(self.id, Flags::empty(), payload);
                if self.out_tx.send(frame).is_err() {
                    // Session writer is gone. Refund the credit so accounting
                    // stays consistent (not strictly required since we are
                    // about to error out, but keeps invariants tidy).
                    self.send_window.grant(n);
                    self.state.close_write();
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "session is shutting down",
                    )));
                }
                Poll::Ready(Ok(n as usize))
            }
        }
    }

    /// Send a `FIN` frame and close the write half. Idempotent.
    pub(crate) fn local_fin(&self) {
        if self.state.is_reset() {
            return;
        }
        if !self.state.close_write() {
            return;
        }
        if !self.fin_sent.swap(true, Ordering::AcqRel) {
            let _ = self.out_tx.send(Frame::fin(self.id));
        }
    }

    /// Send a `RST` frame and tear the stream down. Idempotent.
    pub(crate) fn local_reset(&self) {
        if !self.state.mark_reset() {
            return;
        }
        if !self.fin_sent.swap(true, Ordering::AcqRel) {
            let _ = self.out_tx.send(Frame::rst(self.id));
        }
        self.send_window.close();
        self.recv_waker.wake();
        self.ack_waker.wake();
        let _ = self.closer_tx.send(self.id);
    }

    /// Called from `Stream::drop`. Sends a graceful `FIN` if the write half
    /// is still open, then asks the session to drop its registry entry.
    pub(crate) fn on_user_drop(&self) {
        if self.state.write_open() && !self.state.is_reset() {
            self.local_fin();
        }
        let _ = self.closer_tx.send(self.id);
    }
}
