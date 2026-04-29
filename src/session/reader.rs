//! Inbound frame loop.
//!
//! Pulls frames from the connection, validates them, and dispatches:
//!
//! * `Data` frames are appended to the matching stream's recv buffer; flag
//!   bits drive the SYN/ACK/FIN/RST state transitions.
//! * `WindowUpdate` adds credit to the matching stream's send window.
//! * `Ping` requests are echoed; replies are forwarded to the keepalive
//!   task via the keepalive channel (carried inside `SessionInner`).
//! * `GoAway` flips `peer_gone` and starts a graceful local shutdown.

use std::sync::Arc;

use futures_util::StreamExt;
use tokio::io::AsyncRead;
use tokio::sync::watch;
use tokio_util::codec::FramedRead;
use tracing::{debug, trace, warn};

use crate::error::{Error, ErrorCode};
use crate::protocol::{Flags, Frame, FrameCodec};
use crate::stream::{Origin, StreamInner};
use crate::util::id::StreamId;

use super::inner::SessionInner;

pub(crate) async fn run<R>(
    inner: Arc<SessionInner>,
    reader: R,
    pong_tx: tokio::sync::mpsc::UnboundedSender<u32>,
    mut shutdown: watch::Receiver<bool>,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    let codec = FrameCodec::new(inner.config.max_frame_size);
    let mut framed = FramedRead::new(reader, codec);

    loop {
        tokio::select! {
            biased;

            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { break; }
            }

            maybe_frame = framed.next() => {
                match maybe_frame {
                    None => {
                        debug!("reader: peer closed connection");
                        inner.initiate_shutdown(ErrorCode::Normal);
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(error = %e, "reader: protocol error");
                        let code = match e {
                            Error::Protocol(_) => ErrorCode::ProtocolError,
                            _ => ErrorCode::InternalError,
                        };
                        inner.initiate_shutdown(code);
                        break;
                    }
                    Some(Ok(frame)) => {
                        if let Err(e) = dispatch(&inner, frame, &pong_tx) {
                            warn!(error = %e, "reader: dispatch failed");
                            let code = match e {
                                Error::Protocol(_) => ErrorCode::ProtocolError,
                                _ => ErrorCode::InternalError,
                            };
                            inner.initiate_shutdown(code);
                            break;
                        }
                    }
                }
            }
        }
    }

    debug!("reader task exiting");
}

fn dispatch(
    inner: &Arc<SessionInner>,
    frame: Frame,
    pong_tx: &tokio::sync::mpsc::UnboundedSender<u32>,
) -> Result<(), Error> {
    match frame {
        Frame::Data {
            stream_id,
            flags,
            payload,
        } => handle_data(inner, stream_id, flags, payload),
        Frame::WindowUpdate {
            stream_id, delta, ..
        } => {
            if let Some(s) = inner.registry.get(stream_id) {
                s.grant_send_credit(delta);
            }
            Ok(())
        }
        Frame::Ping { flags, opaque } => {
            if flags.contains(Flags::ACK) {
                let _ = pong_tx.send(opaque);
            } else {
                let _ = inner.out_tx.send(Frame::pong(opaque));
            }
            Ok(())
        }
        Frame::GoAway { error_code } => {
            inner.note_peer_gone(error_code);
            inner.initiate_shutdown(error_code);
            Ok(())
        }
    }
}

fn handle_data(
    inner: &Arc<SessionInner>,
    stream_id: StreamId,
    flags: Flags,
    payload: bytes::Bytes,
) -> Result<(), Error> {
    if stream_id == 0 {
        return Err(Error::Protocol("data on stream id 0"));
    }

    if flags.contains(Flags::SYN) {
        // Peer is opening a new stream. The id parity must match the peer's
        // role.
        if inner.role.owns(stream_id) {
            return Err(Error::Protocol("peer SYN with our parity"));
        }
        if inner.peer_gone.load(std::sync::atomic::Ordering::Acquire) {
            // Refuse new streams once we have observed GoAway.
            let _ = inner.out_tx.send(Frame::rst(stream_id));
            return Ok(());
        }
        if inner.registry.len() >= inner.config.max_streams {
            let _ = inner.out_tx.send(Frame::rst(stream_id));
            return Ok(());
        }
        let stream_inner = StreamInner::new(
            stream_id,
            Origin::Remote,
            inner.config.clone(),
            inner.out_tx.clone(),
            inner.closer_tx.clone(),
        );
        inner
            .registry
            .insert(stream_id, stream_inner.clone())
            .map_err(|_| Error::Protocol("duplicate stream id"))?;
        if inner.accept_tx.send(stream_inner.clone()).is_err() {
            // Acceptor is gone; reset to be polite.
            inner.registry.remove(stream_id);
            let _ = inner.out_tx.send(Frame::rst(stream_id));
            return Ok(());
        }
        // Fall through so payload (if any) is still pushed.
    }

    let Some(stream) = inner.registry.get(stream_id) else {
        // Unknown stream: accept ACK for already-closed streams silently;
        // for everything else, send RST.
        if !flags.contains(Flags::ACK) && !flags.contains(Flags::RST) {
            let _ = inner.out_tx.send(Frame::rst(stream_id));
        }
        trace!(stream = stream_id, "data for unknown stream");
        return Ok(());
    };

    if flags.contains(Flags::ACK) {
        stream.mark_acked();
    }

    if !payload.is_empty() {
        if !stream.can_recv() {
            stream.local_reset();
            return Err(Error::Protocol("data after FIN"));
        }
        stream.push_data(payload);
    }

    if flags.contains(Flags::FIN) {
        stream.remote_fin();
    }
    if flags.contains(Flags::RST) {
        stream.remote_reset();
    }

    Ok(())
}
