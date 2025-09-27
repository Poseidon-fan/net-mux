use tokio::sync::oneshot;

use crate::{error::Error, frame::Frame};

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
