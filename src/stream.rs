//! Stream abstraction for multiplexing
//!
//! This module provides the [`Stream`] struct for representing individual data streams
//! in network multiplexing. Each stream has a unique stream ID and implements async
//! read/write interfaces, supporting concurrent processing of multiple streams.
//!
//! # Features
//!
//! - **Async I/O**: Implements [`AsyncRead`] and [`AsyncWrite`] traits
//! - **State Management**: Uses bit flags to track stream read/write state
//! - **Auto Cleanup**: Automatically notifies stream manager when stream closes
//! - **Thread Safety**: Supports safe cross-thread transfer

use std::{
    cmp,
    future::Future,
    pin::Pin,
    sync::OnceLock,
    task::{Context, Poll},
};

use bitflags::bitflags;
use parking_lot::RwLock;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{broadcast, mpsc, oneshot},
};
use tokio_util::bytes::{Buf, Bytes};

use crate::{
    alloc::StreamId,
    error::Error,
    frame::Frame,
    msg::{self, Message},
};

// Async Future type for writing frames
type WriteFrameFuture = Pin<Box<dyn Future<Output = Result<usize, Error>> + Send + Sync>>;

/// Multiplexed stream
///
/// Represents a data stream in network multiplexing, supporting async read/write operations.
/// Each stream has a unique stream ID and communicates with the stream manager through
/// message channels.
pub struct Stream {
    stream_id: StreamId,
    status: RwLock<StreamFlags>,
    read_buf: Bytes,
    cur_write_fut: Option<WriteFrameFuture>,

    _shutdown_rx: broadcast::Receiver<()>,

    msg_tx: mpsc::UnboundedSender<Message>,
    frame_rx: mpsc::UnboundedReceiver<Frame>,
    remote_fin_rx: oneshot::Receiver<()>,
    close_tx: mpsc::UnboundedSender<StreamId>,

    close_once: OnceLock<()>,
}

bitflags! {
    // Stream state flags
    //
    // Used to track stream read/write state, supporting half-close operations.
    struct StreamFlags: u8 {
        // Read permission flag
        const R = 1 << 0;
        // Write permission flag
        const W = 1 << 1;

        // Read/Write permission flags (R | W)
        const V = Self::R.bits() | Self::W.bits();
    }
}

impl Stream {
    /// Close the stream
    ///
    /// Sends a FIN message to the remote peer and disables write operations.
    /// The stream will be automatically cleaned up when both read and write
    /// operations are disabled.
    pub fn close(&self) {
        self.close_once.get_or_init(|| {
            self.deny_rw(StreamFlags::W);
            let _ = msg::send_fin(self.msg_tx.clone(), self.stream_id);
        });
    }

    // Create a new stream and listen remote fin signal.
    pub(crate) fn new(
        stream_id: StreamId,
        shutdown_rx: broadcast::Receiver<()>,
        msg_tx: mpsc::UnboundedSender<Message>,
        frame_rx: mpsc::UnboundedReceiver<Frame>,
        close_tx: mpsc::UnboundedSender<StreamId>,
        remote_fin_rx: oneshot::Receiver<()>,
    ) -> Self {
        Self {
            stream_id,
            status: RwLock::new(StreamFlags::V),
            read_buf: Bytes::new(),
            cur_write_fut: None,
            _shutdown_rx: shutdown_rx,
            msg_tx,
            frame_rx,
            close_tx,
            remote_fin_rx,
            close_once: OnceLock::new(),
        }
    }

    // Deny read/write permissions for the stream
    //
    // Removes the specified flags from the stream's status. If all permissions
    // are removed (both read and write), the stream will be automatically
    // closed and cleaned up.
    fn deny_rw(&self, flags: StreamFlags) {
        let mut status_guard = self.status.write();
        *status_guard -= flags & StreamFlags::V;

        if !status_guard.contains(StreamFlags::V) {
            let _ = self.close_tx.send(self.stream_id);
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        if self.status.read().contains(StreamFlags::W) {
            let _ = msg::send_fin(self.msg_tx.clone(), self.stream_id);
        }
        self.deny_rw(StreamFlags::V);
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        loop {
            if !this.status.read().contains(StreamFlags::R) {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "stream has been closed",
                )));
            }

            if !this.read_buf.is_empty() {
                let to_copy = cmp::min(this.read_buf.len(), buf.remaining());
                buf.put_slice(&this.read_buf[..to_copy]);
                this.read_buf.advance(to_copy);
                return Poll::Ready(Ok(()));
            }

            match Pin::new(&mut this.frame_rx).poll_recv(cx) {
                Poll::Ready(Some(frame)) => {
                    this.read_buf = Bytes::from(frame.data);
                    continue;
                }
                Poll::Pending => {
                    match Pin::new(&mut this.remote_fin_rx).poll(cx) {
                        Poll::Ready(_) => {
                            this.deny_rw(StreamFlags::R);
                            return Poll::Ready(Ok(()));
                        }
                        Poll::Pending => {}
                    }
                    return Poll::Pending;
                }
                Poll::Ready(None) => {
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        if !self.status.read().contains(StreamFlags::W) {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stream is closed for writing",
            )));
        }

        if self.cur_write_fut.is_none() {
            let msg_tx = self.msg_tx.clone();
            let stream_id = self.stream_id;
            let data = buf.to_vec();

            self.cur_write_fut =
                Some(
                    Box::pin(async move { msg::send_psh(msg_tx, stream_id, &data).await })
                        as WriteFrameFuture,
                );
            return Poll::Ready(Ok(buf.len()));
        }

        match self.cur_write_fut.as_mut().unwrap().as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(_)) => {
                let msg_tx = self.msg_tx.clone();
                let stream_id = self.stream_id;
                let data = buf.to_vec();

                self.cur_write_fut =
                    Some(
                        Box::pin(async move { msg::send_psh(msg_tx, stream_id, &data).await })
                            as WriteFrameFuture,
                    );
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e.to_string()))),
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        if let Some(fut) = self.cur_write_fut.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok(_)) => {
                    self.cur_write_fut = None;
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(Err(e)) => {
                    self.cur_write_fut = None;
                    Poll::Ready(Err(std::io::Error::other(e.to_string())))
                }
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let res = match &mut self.cur_write_fut {
            Some(fut) => match fut.as_mut().poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok(_)) => {
                    self.cur_write_fut = None;
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(Err(e)) => {
                    self.cur_write_fut = None;
                    Poll::Ready(Err(std::io::Error::other(e.to_string())))
                }
            },
            None => Poll::Ready(Ok(())),
        };
        if !res.is_pending() {
            self.close();
        }
        res
    }
}
