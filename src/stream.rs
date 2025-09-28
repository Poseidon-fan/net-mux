use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bitflags::bitflags;
use parking_lot::RwLock;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{broadcast, mpsc},
};

use crate::{StreamId, error::Error, frame::Frame, msg::Message};

pub struct Stream {
    stream_id: StreamId,
    shutdown_rx: broadcast::Receiver<()>,

    status: RwLock<StreamFlags>,

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
            shutdown_rx,
            status: RwLock::new(StreamFlags::V),
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

    pub(crate) async fn send_frame(&self, frame: Frame) -> Result<usize, Error> {
        let (msg, res_rx) = Message::new(frame);
        self.msg_tx
            .send(msg)
            .await
            .map_err(|_| Error::SendMessageFailed)?;
        res_rx.await.map_err(|_| Error::SendMessageFailed)?
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        todo!()
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        todo!()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        todo!()
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        todo!()
    }
}
