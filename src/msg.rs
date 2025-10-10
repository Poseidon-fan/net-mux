use tokio::sync::{mpsc, oneshot};

use crate::{alloc::StreamId, error::Error, frame::Frame};

// Internal message structure for net-mux protocol communication.
//
// `Message` encapsulates a frame along with a callback channel for returning
// transmission results. This allows asynchronous communication between streams
// and the session manager while maintaining proper error handling and result
// propagation.
#[derive(Debug)]
pub(crate) struct Message {
    pub frame: Frame,
    pub res_tx: oneshot::Sender<Result<usize, Error>>,
}

impl Message {
    // Creates a new `Message` with its associated callback channel.
    pub fn new(frame: Frame) -> (Self, oneshot::Receiver<Result<usize, Error>>) {
        let (res_tx, res_rx) = oneshot::channel();
        (Self { frame, res_tx }, res_rx)
    }
}

// Sends a frame wrapped in a message and waits for the transmission result.
//
// This is a helper function that encapsulates the common pattern of creating
// a message, sending it through the message channel, and waiting for the result.
async fn send_frame(msg_tx: mpsc::Sender<Message>, frame: Frame) -> Result<usize, Error> {
    let (msg, res_rx) = Message::new(frame);

    // Send the message to the session manager
    msg_tx
        .send(msg)
        .await
        .map_err(|_| Error::SendMessageFailed)?;

    // Wait for the transmission result
    // TODO(Poseidon): Add timeout handling for better error recovery
    res_rx.await.map_err(|_| Error::SendMessageFailed)?
}

// Sends a SYN (synchronize) frame to establish a new stream.
pub(crate) async fn send_syn(
    msg_tx: mpsc::Sender<Message>,
    stream_id: StreamId,
) -> Result<usize, Error> {
    send_frame(msg_tx, Frame::new_syn(stream_id)).await
}

// Sends a PSH (push) frame containing data.
pub(crate) async fn send_psh(
    msg_tx: mpsc::Sender<Message>,
    stream_id: StreamId,
    data: &[u8],
) -> Result<usize, Error> {
    send_frame(msg_tx, Frame::new_psh(stream_id, data)).await
}

// Sends a FIN (finish) frame to close a stream.
pub(crate) async fn send_fin(
    msg_tx: mpsc::Sender<Message>,
    stream_id: StreamId,
) -> Result<usize, Error> {
    send_frame(msg_tx, Frame::new_fin(stream_id)).await
}

// Sends a ACK (acknowledgment) frame to acknowledge a stream.
pub(crate) async fn send_ack(
    msg_tx: mpsc::Sender<Message>,
    stream_id: StreamId,
) -> Result<usize, Error> {
    send_frame(msg_tx, Frame::new_ack(stream_id)).await
}
