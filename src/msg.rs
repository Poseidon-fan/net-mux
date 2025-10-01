use tokio::sync::{mpsc, oneshot};

use crate::{StreamId, error::Error, frame::Frame};

#[derive(Debug)]
pub(crate) struct Message {
    pub frame: Frame,
    pub res_tx: oneshot::Sender<Result<usize, Error>>,
}

impl Message {
    pub fn new(frame: Frame) -> (Self, oneshot::Receiver<Result<usize, Error>>) {
        let (res_tx, res_rx) = oneshot::channel();
        (Self { frame, res_tx }, res_rx)
    }
}

async fn send_frame(msg_tx: mpsc::Sender<Message>, frame: Frame) -> Result<usize, Error> {
    let (msg, res_rx) = Message::new(frame);
    msg_tx
        .send(msg)
        .await
        .map_err(|_| Error::SendMessageFailed)?;
    res_rx.await.map_err(|_| Error::SendMessageFailed)?
}

pub(crate) async fn send_syn(
    msg_tx: mpsc::Sender<Message>,
    stream_id: StreamId,
) -> Result<usize, Error> {
    send_frame(msg_tx, Frame::new_syn(stream_id)).await
}

pub(crate) async fn send_psh(
    msg_tx: mpsc::Sender<Message>,
    stream_id: StreamId,
    data: &[u8],
) -> Result<usize, Error> {
    send_frame(msg_tx, Frame::new_psh(stream_id, data)).await
}

pub(crate) async fn send_fin(
    msg_tx: mpsc::Sender<Message>,
    stream_id: StreamId,
) -> Result<usize, Error> {
    send_frame(msg_tx, Frame::new_fin(stream_id)).await
}
