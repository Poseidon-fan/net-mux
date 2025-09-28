use bitflags::bitflags;
use parking_lot::Mutex;
use tokio::sync::{broadcast, mpsc};

use crate::{StreamId, frame::Frame, msg::Message};

pub struct Stream {
    stream_id: StreamId,
    shutdown_rx: broadcast::Receiver<()>,

    status: Mutex<StreamFlags>,

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
    fn deny_rw(&self, flags: StreamFlags) {
        let mut status_guard = self.status.lock();
        *status_guard -= flags & StreamFlags::V;

        if !status_guard.contains(StreamFlags::V) {
            let _ = self.close_tx.send(self.stream_id);
        }
    }
}
