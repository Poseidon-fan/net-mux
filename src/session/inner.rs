//! Shared session state. Exists behind an `Arc` so all background tasks and
//! every cloned [`Session`](super::Session) handle observe the same data.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};
use tokio::task::JoinSet;
use tracing::{debug, trace};

use crate::config::Config;
use crate::error::ErrorCode;
use crate::protocol::Frame;
use crate::stream::StreamInner;
use crate::util::id::{Role, StreamIdAllocator};

use super::manager::StreamRegistry;

pub(crate) struct SessionInner {
    pub(crate) config: Arc<Config>,
    pub(crate) role: Role,
    pub(crate) id_alloc: StreamIdAllocator,
    pub(crate) registry: StreamRegistry,

    /// Send half of the outbound frame queue. Cloned into every stream and
    /// every task that needs to emit frames.
    pub(crate) out_tx: mpsc::UnboundedSender<Frame>,

    /// Stream-close signal: the consumer side runs in the closer task and
    /// removes entries from the registry.
    pub(crate) closer_tx: mpsc::UnboundedSender<u32>,

    /// New inbound streams ready for `accept()`.
    pub(crate) accept_tx: mpsc::UnboundedSender<Arc<StreamInner>>,
    pub(crate) accept_rx: AsyncMutex<mpsc::UnboundedReceiver<Arc<StreamInner>>>,

    /// Shutdown trip wire. Initial value is `false`; transitions to `true`
    /// exactly once.
    pub(crate) shutdown_tx: watch::Sender<bool>,

    /// Set to `true` once the peer sent us `GoAway` or we initiated shutdown.
    pub(crate) is_closing: AtomicBool,

    /// Set to `true` after `GoAway` has been observed from either side; new
    /// `open()` calls are then rejected.
    pub(crate) peer_gone: AtomicBool,

    /// Background tasks. Drained by `close().await`.
    pub(crate) tasks: AsyncMutex<Option<JoinSet<()>>>,
}

impl SessionInner {
    pub(crate) fn shutdown_rx(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.is_closing.load(Ordering::Acquire)
    }

    /// Idempotent shutdown initiation:
    /// 1. Best-effort enqueue a `GoAway`.
    /// 2. Force-close every registered stream so blocked user code unblocks.
    /// 3. Trip the shutdown watch so background tasks exit their select loops.
    pub(crate) fn initiate_shutdown(&self, code: ErrorCode) {
        if self.is_closing.swap(true, Ordering::AcqRel) {
            return;
        }
        debug!(code = %code, "initiating session shutdown");

        let _ = self.out_tx.send(Frame::go_away(code));

        for stream in self.registry.drain() {
            stream.force_close();
        }
        let _ = self.shutdown_tx.send(true);
    }

    /// Mark the peer as having gone away. New `open()` calls are rejected
    /// from now on but established streams continue to function until the
    /// session is fully closed.
    pub(crate) fn note_peer_gone(&self, code: ErrorCode) {
        if !self.peer_gone.swap(true, Ordering::AcqRel) {
            trace!(code = %code, "peer GoAway observed");
        }
    }
}
