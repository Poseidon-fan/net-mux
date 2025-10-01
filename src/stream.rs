use std::{
    cmp,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use bitflags::bitflags;
use parking_lot::RwLock;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{broadcast, mpsc},
};
use tokio_util::bytes::{Buf, Bytes};

use crate::{
    StreamId,
    error::Error,
    frame::Frame,
    msg::{self, Message},
};

type WriteFrameFuture = Pin<Box<dyn Future<Output = Result<usize, Error>> + Send>>;

pub struct Stream {
    stream_id: StreamId,
    status: RwLock<StreamFlags>,
    read_buf: Bytes,
    cur_write_fut: Option<WriteFrameFuture>,

    shutdown_rx: broadcast::Receiver<()>,

    msg_tx: mpsc::Sender<Message>,
    frame_rx: mpsc::Receiver<Frame>,

    close_tx: mpsc::UnboundedSender<StreamId>,
}

bitflags! {
    struct StreamFlags: u8 {
        const R = 1 << 0;
        const W = 1 << 1;

        const V = Self::R.bits() | Self::W.bits();
    }
}

impl Stream {
    pub fn close(&self) {
        self.deny_rw(StreamFlags::W);
    }

    pub(crate) fn new(
        stream_id: StreamId,
        shutdown_rx: broadcast::Receiver<()>,
        msg_tx: mpsc::Sender<Message>,
        frame_rx: mpsc::Receiver<Frame>,
        close_tx: mpsc::UnboundedSender<StreamId>,
    ) -> Self {
        Self {
            stream_id,
            status: RwLock::new(StreamFlags::V),
            read_buf: Bytes::new(),
            cur_write_fut: None,
            shutdown_rx,
            msg_tx,
            frame_rx,
            close_tx,
        }
    }

    fn deny_rw(&self, flags: StreamFlags) {
        let mut status_guard = self.status.write();
        *status_guard -= flags & StreamFlags::V;

        if !status_guard.contains(StreamFlags::V) {
            let _ = self.close_tx.send(self.stream_id);
        }
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
                    return Poll::Pending;
                }
                Poll::Ready(None) => {
                    unreachable!()
                }
            }
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let this = self.get_mut();
        if !this.status.read().contains(StreamFlags::W) {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stream is closed for writing",
            )));
        }

        let mut fut = match this.cur_write_fut.take() {
            Some(fut) => fut,
            None => {
                let msg_tx = this.msg_tx.clone();
                let stream_id = this.stream_id;
                let data = buf.to_vec();
                Box::pin(async move { msg::send_psh(msg_tx, stream_id, &data).await })
                    as WriteFrameFuture
            }
        };

        match fut.as_mut().poll(cx) {
            Poll::Ready(res) => {
                this.cur_write_fut = None;
                match res {
                    Ok(_) => Poll::Ready(Ok(buf.len())),
                    Err(e) => Poll::Ready(Err(std::io::Error::other(e.to_string()))),
                }
            }
            Poll::Pending => {
                this.cur_write_fut = Some(fut);
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.deny_rw(StreamFlags::W);
        Poll::Ready(Ok(()))
    }
}
