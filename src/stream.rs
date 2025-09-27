use tokio::sync::{broadcast, mpsc};

use crate::{StreamId, frame::Frame, msg::Message};

pub struct Stream {
    stream_id: StreamId,
    shutdown_rx: broadcast::Receiver<()>,

    msg_tx: mpsc::Sender<Message>,
    frame_rx: mpsc::Receiver<Frame>,

    close_tx: mpsc::Sender<StreamId>,
}
