//! Outbound frame loop.
//!
//! Drains the per-session `mpsc<Frame>` and serialises each frame onto the
//! connection. On shutdown the queue is drained best-effort so the peer
//! observes the trailing `GoAway`.

use std::sync::Arc;

use futures_util::SinkExt;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch};
use tokio_util::codec::FramedWrite;
use tracing::{debug, trace, warn};

use crate::error::ErrorCode;
use crate::protocol::{Frame, FrameCodec};

use super::inner::SessionInner;

pub(crate) async fn run<W>(
    inner: Arc<SessionInner>,
    mut out_rx: mpsc::UnboundedReceiver<Frame>,
    writer: W,
    mut shutdown: watch::Receiver<bool>,
) where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let codec = FrameCodec::new(inner.config.max_frame_size);
    let mut sink = FramedWrite::new(writer, codec);

    loop {
        tokio::select! {
            biased;

            changed = shutdown.changed() => {
                if changed.is_err() {
                    break;
                }
                if *shutdown.borrow() {
                    break;
                }
            }

            maybe_frame = out_rx.recv() => {
                let Some(frame) = maybe_frame else { break; };
                match write_one(&mut sink, frame).await {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "writer task: frame send failed");
                        inner.initiate_shutdown(ErrorCode::InternalError);
                        break;
                    }
                }
            }
        }
    }

    // Drain best-effort so any GoAway / final FIN reaches the peer.
    drain(&mut sink, &mut out_rx).await;

    if let Err(e) = sink.get_mut().shutdown().await {
        trace!(error = %e, "writer: connection shutdown errored");
    }

    debug!("writer task exiting");
}

async fn write_one<W>(
    sink: &mut FramedWrite<W, FrameCodec>,
    frame: Frame,
) -> Result<(), crate::error::Error>
where
    W: AsyncWrite + Unpin,
{
    sink.feed(frame).await?;
    sink.flush().await?;
    Ok(())
}

async fn drain<W>(
    sink: &mut FramedWrite<W, FrameCodec>,
    out_rx: &mut mpsc::UnboundedReceiver<Frame>,
) where
    W: AsyncWrite + Unpin,
{
    while let Ok(frame) = out_rx.try_recv() {
        if let Err(e) = sink.feed(frame).await {
            trace!(error = %e, "writer drain: feed errored");
            return;
        }
    }
    if let Err(e) = sink.flush().await {
        trace!(error = %e, "writer drain: flush errored");
    }
}
