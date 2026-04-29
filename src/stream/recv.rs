//! Per-stream inbound buffer.

use std::collections::VecDeque;

use bytes::{Buf, Bytes};
use tokio::io::ReadBuf;

/// FIFO queue of `Bytes` chunks waiting to be consumed by the reader.
///
/// The session's reader task pushes onto the back; the user's `poll_read`
/// pops from the front. Each chunk is consumed in place via `Bytes::advance`
/// so partial reads do not require re-allocating.
#[derive(Debug, Default)]
pub(crate) struct RecvBuffer {
    queue: VecDeque<Bytes>,
}

impl RecvBuffer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, chunk: Bytes) {
        if !chunk.is_empty() {
            self.queue.push_back(chunk);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.queue.iter().all(Bytes::is_empty)
    }

    /// Drain bytes into the supplied [`ReadBuf`]. Returns the number of
    /// bytes copied.
    pub(crate) fn read_into(&mut self, buf: &mut ReadBuf<'_>) -> usize {
        let mut total = 0;
        while buf.remaining() > 0 {
            let Some(front) = self.queue.front_mut() else {
                break;
            };
            if front.is_empty() {
                self.queue.pop_front();
                continue;
            }
            let n = buf.remaining().min(front.len());
            buf.put_slice(&front[..n]);
            front.advance(n);
            total += n;
            if front.is_empty() {
                self.queue.pop_front();
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_n(buf: &mut RecvBuffer, n: usize) -> Vec<u8> {
        let mut out = vec![0u8; n];
        let mut rb = ReadBuf::new(&mut out);
        let read = buf.read_into(&mut rb);
        out.truncate(read);
        out
    }

    #[test]
    fn drains_in_order() {
        let mut b = RecvBuffer::new();
        b.push(Bytes::from_static(b"hello "));
        b.push(Bytes::from_static(b"world"));
        assert_eq!(read_n(&mut b, 100), b"hello world".to_vec());
        assert!(b.is_empty());
    }

    #[test]
    fn supports_partial_reads() {
        let mut b = RecvBuffer::new();
        b.push(Bytes::from_static(b"abcdef"));
        assert_eq!(read_n(&mut b, 3), b"abc".to_vec());
        assert!(!b.is_empty());
        assert_eq!(read_n(&mut b, 100), b"def".to_vec());
    }

    #[test]
    fn ignores_empty_pushes() {
        let mut b = RecvBuffer::new();
        b.push(Bytes::new());
        assert!(b.is_empty());
    }
}
