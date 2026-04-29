//! Periodic `Ping` keepalive task.
//!
//! Each tick of `Config::keepalive_interval` we emit a `Ping` carrying a
//! monotonically incrementing nonce, then wait up to `keepalive_timeout`
//! for the matching reply. Failure to receive the reply trips a graceful
//! shutdown with [`ErrorCode::Timeout`].

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, interval_at, timeout};
use tracing::{debug, trace, warn};

use crate::error::ErrorCode;
use crate::protocol::Frame;

use super::inner::SessionInner;

pub(crate) async fn run(
    inner: Arc<SessionInner>,
    interval: Duration,
    deadline: Duration,
    mut pong_rx: mpsc::UnboundedReceiver<u32>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut ticker = interval_at(Instant::now() + interval, interval);
    let mut nonce: u32 = 0;

    loop {
        tokio::select! {
            biased;

            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { break; }
            }

            _ = ticker.tick() => {
                nonce = nonce.wrapping_add(1);
                if inner.out_tx.send(Frame::ping(nonce)).is_err() {
                    break;
                }

                match timeout(deadline, await_pong(nonce, &mut pong_rx)).await {
                    Ok(()) => {
                        trace!(nonce, "pong received");
                    }
                    Err(_) => {
                        warn!(deadline_ms = deadline.as_millis() as u64, "keepalive timeout");
                        inner.initiate_shutdown(ErrorCode::Timeout);
                        break;
                    }
                }
            }
        }
    }
    debug!("keepalive task exiting");
}

async fn await_pong(expected: u32, pong_rx: &mut mpsc::UnboundedReceiver<u32>) {
    while let Some(got) = pong_rx.recv().await {
        if got == expected {
            return;
        }
    }
}
